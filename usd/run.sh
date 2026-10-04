#!/bin/sh
# Export maps to OpenUSD, serve the viewer on localhost and open the first
# one in the browser. Ctrl-C stops the server.
#
#     sh usd/run.sh MAP.xodr [MAP.xodr ...]
#
# PORT defaults to 8001.
set -eu

[ $# -gt 0 ] || { echo "usage: sh usd/run.sh MAP.xodr [MAP.xodr ...]" >&2; exit 2; }
cd "$(dirname "$0")/.."
port=${PORT:-8001}

cargo build --release -q -p xodr-usd
for map in "$@"; do
    ./target/release/xodr_usd "$map" "usd/web/$(basename "$map" .xodr).usda"
done
url=http://localhost:$port/?file=$(basename "$1" .xodr).usda

(
    for _ in $(seq 50); do
        curl -fs -o /dev/null "http://localhost:$port/" && break
        sleep 0.1
    done
    if command -v xdg-open >/dev/null; then
        xdg-open "$url" >/dev/null 2>&1
    elif command -v open >/dev/null; then
        open "$url"
    fi
) &

echo "viewer at $url"
exec python3 -m http.server --bind 127.0.0.1 --directory usd/web "$port"
