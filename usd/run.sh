#!/bin/sh
# Export maps to OpenUSD, flatten each with any catalogues, and open the
# first one in the browser. Ctrl-C stops the server.
#
#     PORT=8001 sh usd/run.sh [CATALOGUE.usda ...] MAP.xodr [MAP.xodr ...]
#
# The first run installs OpenUSD's Python module into target/usd-venv.
set -eu

cd "$(dirname "$0")/.."
port=${PORT:-8001}
venv=target/usd-venv

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

if [ ! -x "$venv/bin/python" ]; then
    python3 -m venv "$venv"
    "$venv/bin/pip" install -q --disable-pip-version-check usd-core==26.8
fi

cargo build --release -q -p xodr-usd
mkdir -p target/usd
first=
for map in $maps; do
    name=$(basename "$map" .xodr)
    ./target/release/xodr_usd "$map" "target/usd/$name.usda" >/dev/null
    # shellcheck disable=SC2086
    "$venv/bin/python" usd/flatten.py "usd/web/$name.usda" "target/usd/$name.usda" $catalogues
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
exec python3 -m http.server --bind 127.0.0.1 --directory usd/web "$port"
