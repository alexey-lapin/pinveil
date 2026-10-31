#!/usr/bin/env bash
# Release smoke test for a server image (implementation plan, Phase 6 step 3).
#
#   packaging/container-smoke.sh <image>
#
# Runs the image with the SQLite store on a named volume, creates a message, restarts the
# container, and checks the message survived and opens exactly once.
set -euo pipefail

image=$1
name="pinveil-smoke-$$"
volume="pinveil-smoke-$$"
work=$(mktemp -d)
cleanup() {
  docker rm -f "$name" >/dev/null 2>&1 || true
  docker volume rm -f "$volume" >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap cleanup EXIT
fail() {
  echo "container smoke: $*" >&2
  docker logs "$name" >&2 || true
  exit 1
}

docker volume create "$volume" >/dev/null
docker run -d --name "$name" -p 127.0.0.1::8080 -v "$volume:/data" \
  -e PINVEIL_STORE__MODE=sqlite -e PINVEIL_STORE__PATH=/data/pinveil.db \
  -e PINVEIL_PIN__PEPPER=container-smoke "$image" >/dev/null

wait_ready() {
  base="http://$(docker port "$name" 8080/tcp | head -1)"
  for _ in $(seq 1 100); do
    if curl -fsS "$base/api/config" -o "$work/config.json" 2>/dev/null; then return; fi
    sleep 0.2
  done
  fail "the server did not come up"
}
wait_ready
grep -q '"storeMode":"sqlite"' "$work/config.json" || fail "store mode is not sqlite"

head -c 512 /dev/urandom >"$work/blob"
id=$(curl -fsS "$base/api/messages" -F "blob=@$work/blob;filename=blob" -F pin=123456 -F ttl=600 |
  sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
[ -n "$id" ] || fail "create failed"

docker restart "$name" >/dev/null
wait_ready
curl -fsS "$base/api/messages/$id/retrieve" -H "X-Message-Pin: 123456" -o "$work/back" ||
  fail "the message did not survive a restart"
cmp -s "$work/blob" "$work/back" || fail "the message changed"
status=$(curl -s -o /dev/null -w '%{http_code}' "$base/api/messages/$id/retrieve" -H "X-Message-Pin: 123456")
[ "$status" = 404 ] || fail "second retrieve returned $status"
echo "container smoke test passed for $image"
