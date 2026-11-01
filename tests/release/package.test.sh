#!/usr/bin/env bash
# Tests for packaging/package.sh in a temporary copy of the repository layout, with stand-in
# binaries and a stand-in notices generator. Nothing is built.
#
#   tests/release/package.test.sh
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
passed=0

fail() { echo "FAIL: $*" >&2; exit 1; }
ok() { passed=$((passed + 1)); echo "ok - $*"; }

target=x86_64-unknown-linux-musl
mkdir -p "$work/packaging" "$work/assets" "$work/target/$target/release"
cp "$root/packaging/package.sh" "$work/packaging/"
cp "$root/LICENSE" "$root/NOTICE" "$work/"
cp "$root/assets/README.md" "$work/assets/"
echo cli >"$work/target/$target/release/pinveil"
echo server >"$work/target/$target/release/pinveil-server"
# The stand-in writes the crate's name, or fails when FAIL_NOTICES names that crate.
cat >"$work/packaging/third-party.py" <<'EOF'
import os, sys
if os.environ.get("FAIL_NOTICES") == sys.argv[1]:
    sys.exit("no license or notice file in: example-crate")
open(sys.argv[3], "w").write(sys.argv[1] + "\n")
EOF

out=$("$work/packaging/package.sh" "$target" 1.2.0)
[ "$out" = "cli=dist/pinveil_1.2.0_linux_amd64.tar.gz
server=dist/pinveil-server_1.2.0_linux_amd64.tar.gz" ] || fail "unexpected output: $out"
[ "$(tar -xzOf "$work/dist/pinveil_1.2.0_linux_amd64.tar.gz" ./THIRD-PARTY-NOTICES.txt)" = pinveil-cli ] ||
  fail "the CLI archive lacks the CLI's notices"
[ "$(tar -xzOf "$work/dist/pinveil-server_1.2.0_linux_amd64.tar.gz" ./pinveil-server)" = server ] ||
  fail "the server archive lacks the server binary"
ok "package.sh writes a CLI and a server archive and prints both paths"

for crate in pinveil-cli pinveil-server; do
  rm -rf "$work/dist"
  if out=$(FAIL_NOTICES=$crate "$work/packaging/package.sh" "$target" 1.2.0 2>/dev/null); then
    fail "package.sh succeeded although the $crate notices failed: $out"
  fi
  if printf '%s\n' "$out" | grep -q "^server="; then
    fail "package.sh printed the server archive after the $crate notices failed"
  fi
  ok "package.sh fails when the $crate notices fail"
done

echo "all $passed checks passed"
