#!/usr/bin/env bash
# Release staging and publishing steps (implementation plan, Phase 6), one command each:
#
#   packaging/publish.sh stage-release  <tag> <prerelease:true|false> <dist-dir>
#   packaging/publish.sh publish-release <tag>
#   packaging/publish.sh tag-image      <image> <digest> <version> <prerelease:true|false>
#   packaging/publish.sh push-manifest  <repo> <token> <source> <destination> <tag> <version>
#
# Every step is idempotent and keyed to the tested artifacts: the checksums in
# <dist-dir>/checksums.txt and the tested image digest. Each destination's outcome is
# written to $GITHUB_STEP_SUMMARY (stdout outside CI) as one of: published, unchanged,
# skipped, refused, not published, or unknown (after checking the remote where possible).
# A failed step exits non-zero after reporting every destination it touched, so a retry
# knows exactly what is live.
#
# Stable channels never move backwards: `latest`, the `<major>.<minor>` alias, the Homebrew
# formula, and the Scoop manifest are only updated when no newer stable release exists,
# so retrying an old release's publish job can't downgrade them.
#
# Tests: tests/release/publish.test.sh runs every command against fake gh and docker
# executables and local Git repositories.
set -euo pipefail

summary=${GITHUB_STEP_SUMMARY:-/dev/stdout}
note() { echo "- $*" >>"$summary"; }
die() { echo "publish: $*" >&2; exit 1; }

sha256_check() { # runs `sha256sum -c` in the current directory
  if command -v sha256sum >/dev/null; then sha256sum -c --quiet "$@"; else shasum -a 256 -c --quiet "$@"; fi
}

# Every published stable version (no drafts, no prereleases), oldest first. Fails if the
# list can't be read: an unreadable list must never look like an empty one.
stable_versions() {
  local tags
  tags=$(gh release list --exclude-drafts --exclude-pre-releases --limit 200 --json tagName \
    --jq '.[].tagName') || return 1
  printf '%s\n' "$tags" | sed '/^$/d; s/^v//' | sort -V
}

# Whether <version> is at least <other> (empty <other> counts as older).
at_least() {
  [ -z "$2" ] || [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -n 1)" = "$1" ]
}

stage_release() {
  local tag=$1 prerelease=$2 dist=$3
  (cd "$dist" && sha256_check checksums.txt) || die "the tested archives don't match checksums.txt"
  local draft err
  err=$(mktemp)
  if ! draft=$(gh release view "$tag" --json isDraft --jq .isDraft 2>"$err"); then
    # Only "not found" means there is no release yet; any other error is unknown.
    grep -qxF 'release not found' "$err" || die "cannot read release $tag: $(cat "$err")"
    draft=missing
  fi
  rm -f "$err"
  case "$draft" in
    missing)
      local flags=(--draft --verify-tag --generate-notes --title "$tag")
      [ "$prerelease" = true ] && flags+=(--prerelease)
      gh release create "$tag" "$dist"/pinveil_* "$dist/checksums.txt" "${flags[@]}"
      note "staged: draft release $tag" ;;
    true)
      # Still a draft, so nothing public refers to its assets yet: replace them.
      gh release upload "$tag" "$dist"/pinveil_* "$dist/checksums.txt" --clobber
      note "staged: replaced the assets of draft release $tag" ;;
    false)
      # Already public: never replace. A retry is fine only if it is the same build.
      local published
      published=$(mktemp -d)
      gh release download "$tag" --dir "$published" --pattern checksums.txt
      if cmp -s "$published/checksums.txt" "$dist/checksums.txt"; then
        note "unchanged: release $tag is already published with these checksums"
      else
        note "refused: release $tag is already published with different archives"
        die "release $tag is published with other archives; a rebuild can't replace them"
      fi ;;
    *) die "unexpected state for release $tag: $draft" ;;
  esac
  # The staged copies must be the tested bytes.
  local staged
  staged=$(mktemp -d)
  gh release download "$tag" --dir "$staged" --pattern 'pinveil_*'
  cp "$dist/checksums.txt" "$staged/"
  (cd "$staged" && sha256_check checksums.txt) || die "the staged assets of $tag differ from the tested ones"
  note "verified: staged assets of $tag match checksums.txt"
}

publish_release() {
  local tag=$1
  if [ "$(gh release view "$tag" --json isDraft --jq .isDraft)" = false ]; then
    note "unchanged: release $tag was already published"
    return
  fi
  if gh release edit "$tag" --draft=false >/dev/null; then
    note "published: release $tag"
    return
  fi
  case "$(gh release view "$tag" --json isDraft --jq .isDraft 2>/dev/null || echo unknown)" in
    false) note "published: release $tag (confirmed after an error)"; return ;;
    true) note "not published: release $tag is still a draft" ;;
    *) note "unknown: release $tag; check whether it is still a draft" ;;
  esac
  return 1
}

# The digest an image reference points at; nothing if the registry says it doesn't exist.
# Any other failure (network, auth, a registry error) fails: unknown is not absent.
image_digest() {
  local out err
  err=$(mktemp)
  if out=$(docker buildx imagetools inspect "$1" --format '{{json .Manifest}}' 2>"$err"); then
    rm -f "$err"
    python3 -c 'import json,sys; print(json.load(sys.stdin)["digest"])' <<<"$out"
    return
  fi
  # Exactly buildx's missing-manifest line for this reference, or the registry's
  # MANIFEST_UNKNOWN code. Other errors can mention "not found" too (a missing credential
  # helper, an HTTP error page), and those must stay unknown.
  if grep -qxF "ERROR: $1: not found" "$err" || grep -qF 'MANIFEST_UNKNOWN' "$err"; then
    rm -f "$err"
    return 0
  fi
  echo "publish: cannot inspect $1: $(cat "$err")" >&2
  rm -f "$err"
  return 1
}

tag_image() {
  local image=$1 digest=$2 version=$3 prerelease=$4
  local tags=("$version")
  if [ "$prerelease" = false ]; then
    local stable newest minor newest_in_minor
    if ! stable=$(stable_versions); then
      note "unknown: can't list published releases, so no image tag was moved"
      die "cannot list releases"
    fi
    newest=$(printf '%s\n' "$stable" | tail -n 1)
    minor=${version%.*}
    newest_in_minor=$(printf '%s\n' "$stable" | grep -E "^${minor//./\\.}\.[0-9]+$" | tail -n 1 || true)
    if at_least "$version" "$newest_in_minor"; then tags+=("$minor")
    else note "skipped: $image:$minor stays on newer $newest_in_minor"; fi
    if at_least "$version" "$newest"; then tags+=(latest)
    else note "skipped: $image:latest stays on newer $newest"; fi
  fi
  # The version tag is immutable: it may be created, or found already at this digest.
  local current
  if ! current=$(image_digest "$image:$version"); then
    note "unknown: can't read $image:$version, so no image tag was moved"
    die "cannot inspect $image:$version"
  fi
  if [ -n "$current" ] && [ "$current" != "$digest" ]; then
    note "refused: $image:$version already points at $current, not the tested $digest"
    die "$image:$version is taken by another build"
  fi
  local failed=0 tag
  for tag in "${tags[@]}"; do
    if ! current=$(image_digest "$image:$tag"); then
      note "unknown: can't read $image:$tag; it was not moved"
      failed=1
      continue
    fi
    if [ "$current" = "$digest" ]; then
      note "unchanged: $image:$tag already at $digest"
    elif docker buildx imagetools create -t "$image:$tag" "$image@$digest" >/dev/null; then
      note "published: $image:$tag at $digest"
    else
      current=$(image_digest "$image:$tag") || current=unknown
      if [ "$current" = "$digest" ]; then
        note "published: $image:$tag at $digest (confirmed after an error)"
      elif [ -n "$current" ] && [ "$current" != unknown ]; then
        note "not published: $image:$tag still points at $current"
        failed=1
      else
        note "unknown: $image:$tag; check the registry"
        failed=1
      fi
    fi
  done
  return "$failed"
}

push_manifest() {
  local repo=$1 token=$2 source=$3 destination=$4 tag=$5 version=$6
  local stable newest
  if ! stable=$(stable_versions); then
    note "unknown: can't list published releases, so $repo $destination was not changed"
    die "cannot list releases"
  fi
  newest=$(printf '%s\n' "$stable" | tail -n 1)
  if ! at_least "$version" "$newest"; then
    note "skipped: $repo $destination stays on newer $newest"
    return
  fi
  [ -n "$token" ] || die "no token for $repo"
  local url=${PUBLISH_GIT_BASE:-https://x-access-token:$token@github.com/alexey-lapin}/$repo.git
  local work
  work=$(mktemp -d)
  git clone -q --depth 1 "$url" "$work"
  mkdir -p "$work/$(dirname "$destination")"
  cp "$source" "$work/$destination"
  git -C "$work" add "$destination"
  # The index, not the work tree: a first publication is a new, untracked file.
  if git -C "$work" diff --cached --quiet; then
    note "unchanged: $repo already has this $destination for $tag"
    return
  fi
  git -C "$work" -c user.name="github-actions[bot]" \
    -c user.email="github-actions[bot]@users.noreply.github.com" commit -qm "pinveil $tag"
  if git -C "$work" push -q origin HEAD; then
    note "published: $repo $destination for $tag"
    return
  fi
  local check
  check=$(mktemp -d)
  if git clone -q --depth 1 "$url" "$check" 2>/dev/null && cmp -s "$source" "$check/$destination"; then
    note "published: $repo $destination for $tag (confirmed after an error)"
    return
  elif [ -d "$check/.git" ]; then note "not published: $repo $destination is unchanged"
  else note "unknown: $repo $destination; check the repository"; fi
  return 1
}

command=${1:-}
shift || true
case "$command" in
  stage-release) stage_release "$@" ;;
  publish-release) publish_release "$@" ;;
  tag-image) tag_image "$@" ;;
  push-manifest) push_manifest "$@" ;;
  *) die "unknown command '$command'" ;;
esac
