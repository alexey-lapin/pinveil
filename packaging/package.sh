#!/usr/bin/env bash
# Packages one target's release binaries into dist/.
#
#   packaging/package.sh <rust-target> <version>
#
# Archive names follow pinveil_<version>_<os>_<arch>.<tar.gz|zip>, the same scheme as the
# other alexey-lapin releases, so the Homebrew formula and Scoop manifest can point at them.
# Each archive holds both binaries, LICENSE, NOTICE, and the wordlist's license note.
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
name="pinveil_${version}_${os}_${arch}"
stage="$root/dist/$name"
rm -rf "$stage"
mkdir -p "$stage"
for bin in pinveil pinveil-server; do
  cp "$root/target/$target/release/$bin$exe" "$stage/"
done
cp "$root/LICENSE" "$root/NOTICE" "$stage/"
cp "$root/assets/README.md" "$stage/WORDLIST-LICENSE.md"

cd "$root/dist"
if [ "$ext" = zip ]; then
  # 7z ships on the Windows runners; zip may not.
  if command -v 7z >/dev/null; then 7z a -tzip -bd "$name.zip" "./$name/*" >/dev/null
  else (cd "$name" && zip -qr "../$name.zip" .); fi
else
  tar -C "$name" -czf "$name.tar.gz" .
fi
rm -rf "$stage"
echo "$root/dist/$name.$ext"
