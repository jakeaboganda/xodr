#!/bin/sh
# Export maps to OpenUSD, flatten each with any catalogues, and open the
# first one in the browser. Ctrl-C stops the server.
#
#     PORT=8001 sh usd/run.sh [CATALOGUE.usda ...] MAP.xodr [MAP.xodr ...]
#
# Needs uv (https://docs.astral.sh/uv/), which fetches OpenUSD's Python
# module the first time.
set -eu

cd "$(dirname "$0")/.."
port=${PORT:-8001}
command -v uv >/dev/null || {
    echo "usd/run.sh needs uv: https://docs.astral.sh/uv/getting-started/installation/" >&2
    exit 2
}

maps=
catalogues=
for arg in "$@"; do
    case $arg in
        *.xodr) maps="$maps $arg" ;;
        *.usda) catalogues="$catalogues $arg" ;;
        *) echo "not a .xodr or a .usda: $arg" >&2; exit 2 ;;
    esac
done
[ -n "$maps" ] || {
    echo "usage: sh usd/run.sh [CATALOGUE.usda ...] MAP.xodr [MAP.xodr ...]" >&2
    exit 2
}

cargo build --release -q -p xodr-usd
mkdir -p target/usd
first=
for map in $maps; do
    name=$(basename "$map" .xodr)
    ./target/release/xodr_usd "$map" "target/usd/$name.usda" >/dev/null
    # shellcheck disable=SC2086
    uv run -q usd/flatten.py "usd/web/$name.usda" "target/usd/$name.usda" $catalogues
    echo "usd/web/$name.usda"
    first=${first:-$name}
done
url=http://localhost:$port/?file=$first.usda

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
exec uv run -q --no-project python -m http.server --bind 127.0.0.1 --directory usd/web "$port"
