#!/bin/sh
# Downloads Lua 5.0.3 (lua.org, MIT licence), checks its SHA-256 and builds the `lua` program into
# ~/build/lua50 (nothing is installed system-wide). rgrf-lua-verify finds it there or via $LUA50.
set -eu
DEST="${HOME}/build/lua50"
SHA="1193a61b0e08acaa6eee0eecf29709179ee49c71baebc59b682a25c3b5a45671"
mkdir -p "$DEST"
cd "$DEST"
[ -f lua-5.0.3.tar.gz ] || curl -sSfLO https://www.lua.org/ftp/lua-5.0.3.tar.gz
echo "$SHA  lua-5.0.3.tar.gz" | shasum -a 256 -c -
rm -rf lua-5.0.3
tar xzf lua-5.0.3.tar.gz
make -C lua-5.0.3 >/dev/null
lua-5.0.3/bin/lua -v
echo "built: $DEST/lua-5.0.3/bin/lua"
