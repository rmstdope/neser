#!/bin/sh
set -e

if [ ! -d dist ]; then
    echo "Error: dist/ not found. Run 'bash scripts/build_web.sh' first." >&2
    exit 1
fi

# Symlink roms into dist/ so the Python file server can list them
rm -f dist/roms
ln -s ../web/roms dist/roms

cd dist
python3 -m http.server 8000

