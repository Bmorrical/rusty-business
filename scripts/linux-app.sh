#!/usr/bin/env bash
# Builds the Linux downloads, from this checkout alone.
#
#   scripts/linux-app.sh   builds the .deb, the .rpm, and the .tar.gz, and
#                          their checksums
#
# Run it on 64-bit Linux, the way the Release workflow does in Ubuntu 22.04.
# A program built against a newer C library does not start on an older one,
# so the packages are built on the oldest system they are for.
#
# Brad, 2026-11-17, to a request for Linux downloads (.deb, .rpm, and
# .tar.gz): "I got you in coming build"
#
# Everything it fetches or builds stays in apps/desktop/src-tauri/target,
# which git ignores: its own Rust and Node, each pinned by version and
# checksum, in target/linux/tools. It installs nothing and changes no setting
# of this computer's. What it needs from the system, WebKitGTK, OpenSSL, and a
# C compiler, is installed once by a person who may, and when any of it is
# missing the script stops and gives the line that installs it.
#
# The packages carry no model engine yet. The app starts llama.cpp's
# llama-server when it finds it on the computer, and otherwise the window
# says so.
#
# Safe to run again: what is already fetched and still matches its pin is
# kept.
set -euo pipefail
cd "$(dirname "$0")/.."

# Run again with nothing from this account's environment but who it is, so a
# PATH, a RUSTFLAGS, a NODE_OPTIONS, or a PKG_CONFIG_PATH left in a shell
# cannot change what is built.
if [ "${LINUX_APP_CLEAN:-}" != 1 ]; then
  exec /usr/bin/env -i LINUX_APP_CLEAN=1 HOME="$HOME" USER="${USER:-}" LOGNAME="${LOGNAME:-}" \
    TMPDIR="${TMPDIR:-/tmp}" LANG=C.UTF-8 PATH=/usr/bin:/bin:/usr/sbin:/sbin \
    /bin/bash scripts/linux-app.sh "$@"
fi

fail() {
  echo "$1" >&2
  exit 1
}

[ "$#" -eq 0 ] || fail "scripts/linux-app.sh takes nothing after its name."
case "$(uname -s)/$(uname -m)" in
  Linux/x86_64) ;;
  *) fail "The Linux downloads are built on 64-bit Linux, on an Intel or AMD processor." ;;
esac

# What the build needs from the system. Only someone who may install
# packages can add them, so the script names them and stops.
NEEDS="sudo apt-get install build-essential curl pkg-config libwebkit2gtk-4.1-dev libssl-dev"
{ command -v cc && command -v curl && pkg-config --exists webkit2gtk-4.1 openssl; } >/dev/null 2>&1 ||
  fail "The build needs WebKitGTK, OpenSSL, and a C compiler. On Ubuntu or Debian, run: $NEEDS"

RUST=1.100.0
RUSTUP=1.29.3
RUSTUP_SHA256=b1c8cb14e30889e7e2999e6b69e62d9e9b9ffc0f4df8180564d306d9680e6c55
NODE=22.23.3
NODE_SHA256=1084aa36196bba4c3a5e69a1ee388a6e4ff729dad09445fbcd434b28fe3c24af

DEB=Rusty-Business-linux-x64.deb
RPM=Rusty-Business-linux-x64.rpm
TGZ=Rusty-Business-linux-x64.tar.gz
DESKTOP=apps/desktop
TAURI=$DESKTOP/src-tauri
WORK=$(pwd -P)/$TAURI/target/linux

mkdir -p "$WORK/downloads" "$WORK/tools"

# `check SUM FILE` is true when the file's SHA-256 is SUM.
check() {
  [ "$(sha256sum "$2" | cut -c1-64)" = "$1" ]
}

# `fetch URL SUM NAME` downloads into target/linux/downloads, over HTTPS
# only, and keeps the file only when its checksum is the one pinned here.
fetch() {
  if [ -f "$WORK/downloads/$3" ] && check "$2" "$WORK/downloads/$3"; then
    return
  fi
  rm -f "$WORK/downloads/$3" "$WORK/downloads/$3.part"
  echo "Downloading $1"
  curl --fail --location --silent --show-error --proto '=https' --proto-redir '=https' --tlsv1.2 \
    --output "$WORK/downloads/$3.part" "$1"
  check "$2" "$WORK/downloads/$3.part" || fail "$1 is not the file pinned in scripts/linux-app.sh."
  mv "$WORK/downloads/$3.part" "$WORK/downloads/$3"
}

# --- The tools: Node and Rust of the folder's own ---------------------------

node_home=$WORK/tools/node-$NODE
if [ ! -x "$node_home/bin/node" ]; then
  fetch "https://nodejs.org/dist/v$NODE/node-v$NODE-linux-x64.tar.gz" "$NODE_SHA256" node.tar.gz
  rm -rf "$WORK/tools/node-$NODE" "$WORK/tools/node-v$NODE-linux-x64"
  tar -xzf "$WORK/downloads/node.tar.gz" -C "$WORK/tools"
  mv "$WORK/tools/node-v$NODE-linux-x64" "$node_home"
fi

export RUSTUP_HOME=$WORK/tools/rustup
export CARGO_HOME=$WORK/tools/cargo
export RUSTUP_TOOLCHAIN=$RUST
if [ ! -x "$CARGO_HOME/bin/rustup" ]; then
  fetch "https://static.rust-lang.org/rustup/archive/$RUSTUP/x86_64-unknown-linux-gnu/rustup-init" "$RUSTUP_SHA256" rustup-init
  chmod 755 "$WORK/downloads/rustup-init"
  # --no-modify-path: the account's shell profile is left as it is.
  "$WORK/downloads/rustup-init" -y --quiet --no-modify-path --profile minimal --default-toolchain none
fi
"$CARGO_HOME/bin/rustup" toolchain list | grep -q "^$RUST-" ||
  "$CARGO_HOME/bin/rustup" toolchain install "$RUST" --profile minimal --no-self-update

export PATH=$CARGO_HOME/bin:$node_home/bin:/usr/bin:/bin:/usr/sbin:/sbin
export npm_config_cache=$WORK/tools/npm-cache
export npm_config_userconfig=$WORK/tools/npmrc
export npm_config_audit=false npm_config_fund=false npm_config_update_notifier=false

# --- The packages -----------------------------------------------------------

APP=$(node -p "require('./$TAURI/tauri.conf.json').productName")
VERSION=$(node -p "require('./$TAURI/tauri.conf.json').version")

(cd "$DESKTOP" && { [ -d node_modules ] || npm ci; } && npm run tauri -- build --bundles deb,rpm)

deb="$TAURI/target/release/bundle/deb/${APP}_${VERSION}_amd64.deb"
rpm="$TAURI/target/release/bundle/rpm/${APP}-${VERSION}-1.x86_64.rpm"
# What the .deb puts under /usr: the program, its menu entry, and its icons.
usr="$TAURI/target/release/bundle/deb/${APP}_${VERSION}_amd64/data/usr"
[ -f "$deb" ] || fail "Tauri made no $deb."
[ -f "$rpm" ] || fail "Tauri made no $rpm."
[ -x "$usr/bin/rusty-business" ] || fail "Tauri left no $usr/bin/rusty-business."

rm -f "$WORK/$DEB" "$WORK/$DEB.sha256" "$WORK/$RPM" "$WORK/$RPM.sha256" "$WORK/$TGZ" "$WORK/$TGZ.sha256"
cp "$deb" "$WORK/$DEB"
cp "$rpm" "$WORK/$RPM"

# The .tar.gz holds the same files in one folder, for a system that takes
# neither package. Its owner is no one in particular, and its order is fixed.
folder=${TGZ%.tar.gz}
rm -rf "$WORK/tarball"
mkdir -p "$WORK/tarball/$folder"
cp -R "$usr/." "$WORK/tarball/$folder/"
tar --create --gzip --file "$WORK/$TGZ" --directory "$WORK/tarball" \
  --owner=0 --group=0 --numeric-owner --sort=name "$folder"
rm -rf "$WORK/tarball"

for file in "$DEB" "$RPM" "$TGZ"; do
  (cd "$WORK" && sha256sum "$file" >"$file.sha256")
done
echo "Built $WORK/$DEB, $WORK/$RPM, and $WORK/$TGZ"
