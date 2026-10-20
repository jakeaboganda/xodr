#!/bin/sh
# Build the viewer, serve it on localhost and open it in the browser.
# Ctrl-C stops the server.
#
#     sh viewer/run.sh [PORT]
#
# PORT defaults to 8000. Serving needs uv (https://docs.astral.sh/uv/).
set -eu

cd "$(dirname "$0")/.."
port=${1:-8000}
url=http://localhost:$port/

sh viewer/build.sh

(
    for _ in $(seq 50); do
        curl -fs -o /dev/null "$url" && break
        sleep 0.1
    done
    if command -v xdg-open >/dev/null; then
        xdg-open "$url" >/dev/null 2>&1
    elif command -v open >/dev/null; then
        open "$url"
    fi
) &

echo "viewer at $url"
exec uv run -q --no-project python -m http.server --bind 127.0.0.1 --directory viewer/web "$port"
