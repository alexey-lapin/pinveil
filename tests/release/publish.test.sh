#!/usr/bin/env bash
# Tests for packaging/publish.sh against fake `gh` and `docker` executables and local Git
# repositories. Nothing here reaches GitHub or a registry.
#
#   tests/release/publish.test.sh
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
publish="$root/packaging/publish.sh"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
passed=0

fail() { echo "FAIL: $*" >&2; exit 1; }
ok() { passed=$((passed + 1)); echo "ok - $*"; }

# --- fakes -------------------------------------------------------------------------------
# State lives in $FAKE: release/state (missing|true|false), release/assets/*, stable
# (published stable tags, one per line), tags/<tag> (image digests), and failure switches.
mkdir -p "$work/bin"
cat >"$work/bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
state=$FAKE/release
mkdir -p "$state/assets"
[ -f "$state/state" ] || echo missing >"$state/state"
case "$1 $2" in
  "release view")
    s=$(cat "$state/state"); [ "$s" = missing ] && exit 1; echo "$s" ;;
  "release create")
    shift 3
    while [ $# -gt 0 ] && [ "${1#--}" = "$1" ]; do cp "$1" "$state/assets/"; shift; done
    echo true >"$state/state" ;;
  "release upload")
    shift 3
    while [ $# -gt 0 ] && [ "${1#--}" = "$1" ]; do cp "$1" "$state/assets/"; shift; done ;;
  "release download")
    dir=$(echo "$@" | sed 's/.*--dir \([^ ]*\).*/\1/')
    pattern=$(echo "$@" | sed 's/.*--pattern \([^ ]*\).*/\1/')
    # shellcheck disable=SC2086
    cp "$state"/assets/$pattern "$dir/" ;;
  "release edit")
    echo false >"$state/state"
    [ -f "$FAKE/fail-edit" ] && exit 1
    true ;;
  "release list")
    cat "$FAKE/stable" 2>/dev/null || true ;;
  *) echo "fake gh: unexpected $*" >&2; exit 2 ;;
esac
EOF
cat >"$work/bin/docker" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
mkdir -p "$FAKE/tags"
case "$1 $2 $3" in
  "buildx imagetools inspect")
    tag=${4##*:}
    [ -f "$FAKE/tags/$tag" ] || exit 1
    printf '{"digest":"%s"}\n' "$(cat "$FAKE/tags/$tag")" ;;
  "buildx imagetools create")
    tag=${5##*:}
    digest=${6##*@}
    if [ "$(cat "$FAKE/fail-tag" 2>/dev/null)" = "$tag" ]; then
      # "lost-ack": the registry took the change but the client saw an error.
      [ "$(cat "$FAKE/fail-mode")" = lost-ack ] && echo "$digest" >"$FAKE/tags/$tag"
      exit 1
    fi
    echo "$digest" >"$FAKE/tags/$tag" ;;
  *) echo "fake docker: unexpected $*" >&2; exit 2 ;;
esac
EOF
chmod +x "$work/bin/gh" "$work/bin/docker"
export PATH="$work/bin:$PATH"

reset() {
  export FAKE="$work/fake-$1"
  rm -rf "$FAKE"
  mkdir -p "$FAKE"
  export GITHUB_STEP_SUMMARY="$FAKE/summary.md"
  : >"$GITHUB_STEP_SUMMARY"
}
summary_has() { grep -qF -- "$1" "$GITHUB_STEP_SUMMARY" || fail "summary lacks '$1': $(cat "$GITHUB_STEP_SUMMARY")"; }

# Tested archives and their checksums.
dist="$work/dist"
mkdir -p "$dist"
echo one >"$dist/pinveil_1.2.0_linux_amd64.tar.gz"
echo two >"$dist/pinveil_1.2.0_windows_amd64.zip"
(cd "$dist" && if command -v sha256sum >/dev/null; then sha256sum pinveil_* >checksums.txt; else shasum -a 256 pinveil_* >checksums.txt; fi)

# --- stage-release -----------------------------------------------------------------------
reset stage-new
"$publish" stage-release v1.2.0 false "$dist"
summary_has "staged: draft release v1.2.0"
summary_has "verified: staged assets of v1.2.0"
ok "stage-release creates a draft and verifies it"

"$publish" stage-release v1.2.0 false "$dist"
summary_has "replaced the assets of draft release v1.2.0"
ok "stage-release replaces a draft's assets on a re-run"

echo false >"$FAKE/release/state"
"$publish" stage-release v1.2.0 false "$dist"
summary_has "unchanged: release v1.2.0 is already published with these checksums"
ok "stage-release leaves a published release with the same checksums alone"

rebuilt="$work/rebuilt"
mkdir -p "$rebuilt"
echo other >"$rebuilt/pinveil_1.2.0_linux_amd64.tar.gz"
cp "$dist/pinveil_1.2.0_windows_amd64.zip" "$rebuilt/"
(cd "$rebuilt" && if command -v sha256sum >/dev/null; then sha256sum pinveil_* >checksums.txt; else shasum -a 256 pinveil_* >checksums.txt; fi)
if "$publish" stage-release v1.2.0 false "$rebuilt" 2>/dev/null; then fail "a rebuild replaced a published release"; fi
summary_has "refused: release v1.2.0 is already published with different archives"
cmp -s "$FAKE/release/assets/pinveil_1.2.0_linux_amd64.tar.gz" "$dist/pinveil_1.2.0_linux_amd64.tar.gz" ||
  fail "the published asset was overwritten"
ok "stage-release refuses to replace a published release with a rebuild"

# --- publish-release ---------------------------------------------------------------------
reset publish
mkdir -p "$FAKE/release"
echo true >"$FAKE/release/state"
"$publish" publish-release v1.2.0
summary_has "published: release v1.2.0"
"$publish" publish-release v1.2.0
summary_has "unchanged: release v1.2.0 was already published"
ok "publish-release publishes once, then is a no-op"

reset publish-lost-ack
mkdir -p "$FAKE/release"
echo true >"$FAKE/release/state"
touch "$FAKE/fail-edit"
"$publish" publish-release v1.2.0
summary_has "published: release v1.2.0 (confirmed after an error)"
ok "publish-release confirms a release whose acknowledgment was lost"

# --- tag-image ---------------------------------------------------------------------------
digest=sha256:aaa
reset image
"$publish" tag-image ghcr.io/x/pinveil "$digest" 1.2.0 false
for tag in 1.2.0 1.2 latest; do
  [ "$(cat "$FAKE/tags/$tag")" = "$digest" ] || fail "tag $tag not set"
  summary_has "published: ghcr.io/x/pinveil:$tag at $digest"
done
"$publish" tag-image ghcr.io/x/pinveil "$digest" 1.2.0 false
summary_has "unchanged: ghcr.io/x/pinveil:latest already at $digest"
ok "tag-image sets the version, minor, and latest tags, then is a no-op"

reset image-prerelease
"$publish" tag-image ghcr.io/x/pinveil "$digest" 1.3.0-rc.1 true
if [ ! -f "$FAKE/tags/1.3.0-rc.1" ] || [ -f "$FAKE/tags/latest" ] || [ -f "$FAKE/tags/1.3" ]; then
  fail "a prerelease moved a stable tag"
fi
ok "tag-image never moves stable tags for a prerelease"

reset image-partial
echo latest >"$FAKE/fail-tag"
echo refused >"$FAKE/fail-mode"
if "$publish" tag-image ghcr.io/x/pinveil "$digest" 1.2.0 false; then fail "a failed tag was reported as success"; fi
summary_has "published: ghcr.io/x/pinveil:1.2.0 at $digest"
summary_has "published: ghcr.io/x/pinveil:1.2 at $digest"
summary_has "unknown: ghcr.io/x/pinveil:latest"
ok "tag-image reports what is live after a failure partway"

reset image-lost-ack
echo latest >"$FAKE/fail-tag"
echo lost-ack >"$FAKE/fail-mode"
"$publish" tag-image ghcr.io/x/pinveil "$digest" 1.2.0 false
summary_has "published: ghcr.io/x/pinveil:latest at $digest (confirmed after an error)"
ok "tag-image confirms a tag whose acknowledgment was lost"

reset image-taken
mkdir -p "$FAKE/tags"
echo sha256:other >"$FAKE/tags/1.2.0"
if "$publish" tag-image ghcr.io/x/pinveil "$digest" 1.2.0 false 2>/dev/null; then fail "a version tag was moved"; fi
summary_has "refused: ghcr.io/x/pinveil:1.2.0 already points at sha256:other"
[ ! -f "$FAKE/tags/latest" ] || fail "latest moved after a refusal"
ok "tag-image never moves an existing version tag"

reset image-older
printf 'v1.2.0\nv1.3.0\n' >"$FAKE/stable"
"$publish" tag-image ghcr.io/x/pinveil "$digest" 1.2.0 false
summary_has "skipped: ghcr.io/x/pinveil:latest stays on newer 1.3.0"
if [ -f "$FAKE/tags/latest" ] || [ ! -f "$FAKE/tags/1.2" ]; then
  fail "an old retry moved latest, or lost its minor tag"
fi
printf 'v1.2.0\nv1.2.5\nv1.3.0\n' >"$FAKE/stable"
rm -f "$FAKE/tags/1.2"
"$publish" tag-image ghcr.io/x/pinveil "$digest" 1.2.0 false
summary_has "skipped: ghcr.io/x/pinveil:1.2 stays on newer 1.2.5"
ok "tag-image never moves latest or a minor alias backwards"

# --- push-manifest -----------------------------------------------------------------------
reset manifest
git init -q --bare "$work/remotes/homebrew-tap.git"
seed="$work/seed"
git clone -q "$work/remotes/homebrew-tap.git" "$seed" 2>/dev/null
echo existing >"$seed/README.md"
git -C "$seed" add README.md
git -C "$seed" -c user.name=t -c user.email=t@t commit -qm seed
git -C "$seed" push -q origin HEAD
export PUBLISH_GIT_BASE="file://$work/remotes"
echo "formula v1" >"$work/pinveil.rb"
read_remote() { git --git-dir "$work/remotes/homebrew-tap.git" show "HEAD:Formula/pinveil.rb"; }

"$publish" push-manifest homebrew-tap token "$work/pinveil.rb" Formula/pinveil.rb v1.2.0 1.2.0
summary_has "published: homebrew-tap Formula/pinveil.rb for v1.2.0"
[ "$(read_remote)" = "formula v1" ] || fail "the first formula was not pushed"
ok "push-manifest publishes a new, untracked file"

"$publish" push-manifest homebrew-tap token "$work/pinveil.rb" Formula/pinveil.rb v1.2.0 1.2.0
summary_has "unchanged: homebrew-tap already has this Formula/pinveil.rb for v1.2.0"
ok "push-manifest is a no-op on a retry"

echo "formula v2" >"$work/pinveil.rb"
"$publish" push-manifest homebrew-tap token "$work/pinveil.rb" Formula/pinveil.rb v1.3.0 1.3.0
[ "$(read_remote)" = "formula v2" ] || fail "a changed formula was not pushed"
ok "push-manifest publishes a changed file"

printf 'v1.2.0\nv1.3.0\n' >"$FAKE/stable"
echo "formula old" >"$work/old.rb"
"$publish" push-manifest homebrew-tap token "$work/old.rb" Formula/pinveil.rb v1.2.0 1.2.0
summary_has "skipped: homebrew-tap Formula/pinveil.rb stays on newer 1.3.0"
[ "$(read_remote)" = "formula v2" ] || fail "an old retry downgraded the formula"
ok "push-manifest never moves a channel backwards"

echo "all $passed checks passed"
