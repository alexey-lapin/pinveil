#!/usr/bin/env bash
# Release smoke test for one target's staged archives (implementation plan, Phase 6 step 3).
#
#   packaging/smoke.sh <cli-archive> <server-archive> <version> <tty-run> [tls]
#
# Extracts each archive into its own directory and checks:
#   - each holds exactly its one binary and the license files, and both report <version>,
#   - the server binary has no fault-injection interface,
#   - the server serves its embedded pages, scripts, and a valid wordlist,
#   - a create and retrieve round trip between the staged server and CLI, driven on a
#     pseudo-terminal by tty-run (crates/pty-probe), over plain HTTP to loopback,
#   - with `tls`: the same round trip over HTTPS through a private CA that is trusted in
#     the operating system's store (installing it needs admin rights, as on CI runners).
set -euo pipefail

absolute() { echo "$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"; }
cli_archive=$(absolute "$1")
server_archive=$(absolute "$2")
version=$3
tty_run=$(absolute "$4")
mode=${5:-plain}

here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
pids=()
cleanup() {
  for pid in ${pids[@]+"${pids[@]}"}; do kill "$pid" 2>/dev/null || true; done
  # Leave the work directory first: Windows can't remove the current directory. A temp
  # dir a just-killed process still holds is not a test failure.
  cd /
  rm -rf "$work" 2>/dev/null || true
}
trap cleanup EXIT

case "$(uname -s)" in
  MINGW* | MSYS* | CYGWIN*) exe=.exe python=python ;;
  *) exe='' python=python3 ;;
esac
fail() { echo "smoke: $*" >&2; exit 1; }

# extract <archive> <dir> <binary>: unpacks into its own directory, so a license file one
# archive lacks can't be hidden by the other's copy, and checks the exact contents.
extract() {
  local archive=$1 dir=$2 bin=$3
  mkdir -p "$dir"
  case "$archive" in
    *.zip)
      if command -v 7z >/dev/null; then (cd "$dir" && 7z x -bd "$archive" >/dev/null)
      else (cd "$dir" && unzip -q "$archive"); fi ;;
    *) tar -C "$dir" -xzf "$archive" ;;
  esac
  local expected actual
  expected=$(printf '%s\n' "$bin$exe" LICENSE NOTICE THIRD-PARTY-NOTICES.txt WORDLIST-LICENSE.md | sort)
  actual=$(cd "$dir" && find . -type f | sed 's|^\./||' | sort)
  [ "$actual" = "$expected" ] || fail "$(basename "$archive") holds: $(echo "$actual" | tr "\n" " ")"
  grep -q "^Applies to tokio " "$dir/THIRD-PARTY-NOTICES.txt" ||
    fail "$(basename "$archive"): third-party notices lack tokio"
}
extract "$cli_archive" "$work/cli" pinveil
extract "$server_archive" "$work/server" pinveil-server
grep -q "^Applies to aws-lc-sys " "$work/cli/THIRD-PARTY-NOTICES.txt" || fail "CLI notices lack aws-lc-sys"
grep -q "^Applies to libsqlite3-sys " "$work/server/THIRD-PARTY-NOTICES.txt" ||
  fail "server notices lack libsqlite3-sys"
server="$work/server/pinveil-server$exe"
cli="$work/cli/pinveil$exe"
echo "archive contents and license files: ok"

"$server" --version | grep -qx "pinveil-server $version (protocol 2)" || fail "server version"
"$cli" --version | grep -qx "pinveil $version (protocol 2)" || fail "CLI version"
echo "versions: ok"

# The fault hooks exist only in test builds; their names must not be in the binary.
for marker in consume.after_commit attempt.before_commit fail_at; do
  if grep -aq "$marker" "$server"; then fail "the server binary contains '$marker'"; fi
done
echo "no fault interface: ok"

PINVEIL_PIN__PEPPER=smoke-pepper "$server" --listen 127.0.0.1:0 >"$work/server.out" 2>"$work/server.err" &
pids+=($!)
for _ in $(seq 1 100); do
  grep -q "listening on" "$work/server.out" 2>/dev/null && break
  sleep 0.1
done
addr=$(sed -n 's/^listening on //p' "$work/server.out" | tr -d '\r')
[ -n "$addr" ] || fail "server did not start: $(cat "$work/server.err")"
port=${addr##*:}
base="http://127.0.0.1:$port"

curl -fsS "$base/" | grep -q 'id="app-config"' || fail "index page"
curl -fsS "$base/static/app.js" >/dev/null || fail "static script"
words=$(curl -fsS "$base/static/eff_large_wordlist.txt" | wc -l | tr -d ' ')
[ "$words" = 7776 ] || fail "wordlist has $words lines"
echo "embedded assets: ok"

# Sends a message through `server_url`, opens it again, and compares the text.
round_trip() {
  local url=$1; shift
  printf 'smoke text through %s\n' "$url" >"$work/in.txt"
  "$tty_run" --stdin "$work/in.txt" --stdout "$work/link.txt" -- \
    "$cli" send --server "$url" "$@" >"$work/send.json"
  local pin
  pin=$("$python" -c 'import json,re,sys; r=json.load(open(sys.argv[1])); assert r["exit"]==0, r["plain"]; print(re.search(r"PIN:\s*(\d{6})", r["plain"]).group(1))' "$work/send.json") ||
    fail "send through $url: $(cat "$work/send.json")"
  local link
  link=$(tr -d '\r\n' <"$work/link.txt")
  "$tty_run" --stdout "$work/out.txt" --answer "Link:" "$link" --answer "PIN:" "$pin" -- \
    "$cli" get "$@" >"$work/get.json"
  "$python" -c 'import json,sys; r=json.load(open(sys.argv[1])); assert r["exit"]==0, r["plain"]' "$work/get.json" ||
    fail "get through $url: $(cat "$work/get.json")"
  cmp -s "$work/in.txt" "$work/out.txt" || fail "round trip through $url changed the text"
}

round_trip "$base" --insecure-http
echo "round trip over loopback HTTP: ok"

if [ "$mode" = tls ]; then
  cd "$work"
  echo "creating a private CA and a localhost certificate"
  # Git Bash would rewrite "/CN=..." into a Windows path; only openssl's arguments are
  # exempt, other programs still need their paths converted.
  openssl() { MSYS2_ARG_CONV_EXCL='*' command openssl "$@"; }
  openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj "/CN=pinveil smoke test CA" \
    -keyout ca.key -out ca.crt -addext "basicConstraints=critical,CA:TRUE" \
    -addext "keyUsage=critical,keyCertSign,cRLSign" 2>/dev/null
  openssl req -newkey rsa:2048 -nodes -subj "/CN=localhost" -keyout leaf.key -out leaf.csr 2>/dev/null
  printf 'subjectAltName=DNS:localhost,IP:127.0.0.1\nextendedKeyUsage=serverAuth\n' >leaf.ext
  openssl x509 -req -in leaf.csr -CA ca.crt -CAkey ca.key -CAcreateserial -days 1 \
    -extfile leaf.ext -out leaf.crt 2>/dev/null
  case "$(uname -s)" in
    Linux)
      sudo cp ca.crt /usr/local/share/ca-certificates/pinveil-smoke.crt
      sudo update-ca-certificates >/dev/null ;;
    Darwin)
      sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain ca.crt ;;
    *) certutil -addstore -f Root ca.crt >/dev/null ;;
  esac
  echo "trusted the private CA in the system store"
  tls_port=$((port + 1))
  "$python" "$here/tls-proxy.py" leaf.crt leaf.key "$tls_port" "$port" >proxy.out 2>&1 &
  pids+=($!)
  for _ in $(seq 1 100); do grep -q ready proxy.out 2>/dev/null && break; sleep 0.1; done
  round_trip "https://localhost:$tls_port"
  echo "round trip over HTTPS with a private CA: ok"
fi
echo "smoke test passed for $(basename "$cli_archive") and $(basename "$server_archive")"
