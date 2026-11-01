#!/usr/bin/env bash
# Packages one target's release binaries into dist/.
#
#   packaging/package.sh <rust-target> <version>
#
# Writes two archives, named like the other alexey-lapin releases:
#   pinveil_<version>_<os>_<arch>.<tar.gz|zip>         the CLI (Homebrew and Scoop install it)
#   pinveil-server_<version>_<os>_<arch>.<tar.gz|zip>  the server
# Each holds its one binary, LICENSE, NOTICE, the wordlist's license note, and
# THIRD-PARTY-NOTICES.txt for every crate linked into that binary (packaging/third-party.py).
# Prints `cli=<path>` and `server=<path>`, relative to the repository root, for
# $GITHUB_OUTPUT.
set -euo pipefail

target=$1
version=$2
case "$target" in
  x86_64-unknown-linux-musl) os=linux arch=amd64 ext=tar.gz exe= ;;
  aarch64-unknown-linux-musl) os=linux arch=arm64 ext=tar.gz exe= ;;
  aarch64-apple-darwin) os=darwin arch=arm64 ext=tar.gz exe= ;;
  x86_64-pc-windows-msvc) os=windows arch=amd64 ext=zip exe=.exe ;;
  *) echo "unknown target $target" >&2; exit 1 ;;
esac

root=$(cd "$(dirname "$0")/.." && pwd)
python=$(command -v python3 || command -v python)

# package <binary> <crate>: one archive; prints its path relative to the repository root.
package() {
  local bin=$1 crate=$2
  local name="${bin}_${version}_${os}_${arch}"
  local stage="$root/dist/$name"
  rm -rf "$stage"
  mkdir -p "$stage"
  cp "$root/target/$target/release/$bin$exe" "$stage/"
  cp "$root/LICENSE" "$root/NOTICE" "$stage/"
  cp "$root/assets/README.md" "$stage/WORDLIST-LICENSE.md"
  "$python" "$root/packaging/third-party.py" "$crate" "$target" "$stage/THIRD-PARTY-NOTICES.txt"
  (
    cd "$root/dist"
    if [ "$ext" = zip ]; then
      # 7z ships on the Windows runners; zip may not.
      if command -v 7z >/dev/null; then 7z a -tzip -bd "$name.zip" "./$name/*" >/dev/null
      else (cd "$name" && zip -qr "../$name.zip" .); fi
    else
      tar -C "$name" -czf "$name.tar.gz" .
    fi
  )
  rm -rf "$stage"
  # Relative: Git Bash on Windows prints /d/... absolute paths that other tools
  # (upload-artifact) can't resolve.
  echo "dist/$name.$ext"
}

echo "cli=$(package pinveil pinveil-cli)"
echo "server=$(package pinveil-server pinveil-server)"
