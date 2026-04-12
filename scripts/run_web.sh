#!/bin/sh

# Symlink roms into dist/ so the Python file server can list them
rm -f dist/roms
ln -s ../web/roms dist/roms

cd dist
python3 -m http.server 8000

