#!/usr/bin/env bash
# Builds the Windows installer, from this checkout alone.
#
#   scripts/windows-app.sh   builds the installer and its checksum
#
# Run it in Git Bash on 64-bit Windows, the way the Release workflow does.
#
# Brad, 2026-11-17: "this should be must easier for the user to boot up, run
# it in isolation, one command to rule it all, start a makefile"
#
# Everything it fetches or builds stays in apps/desktop/src-tauri/target,
# which git ignores: its own Rust and Node, each pinned by version and
# checksum, in target/windows/tools. It changes no setting of this computer's,
# and it clears the settings a shell may hold that change what is built.
# Two things come from outside the folder: Microsoft's C++ build tools, which
# Rust needs on Windows and GitHub's Windows computers already have, and the
# installer maker Tauri fetches and checks itself, which it keeps in this
# account's local app data folder.
#
# The window has no engine on Windows yet, so the installer carries none, and
# the window says so.
#
# Safe to run again: what is already fetched and still matches its pin is
# kept.
set -euo pipefail
cd "$(dirname "$0")/.."

fail() {
  echo "$1" >&2
  exit 1
}

[ "$#" -eq 0 ] || fail "scripts/windows-app.sh takes nothing after its name."
case "$(uname -s)/$(uname -m)" in
  MINGW64_NT-*/x86_64 | MSYS_NT-*/x86_64) ;;
  *) fail "The Windows installer is built on 64-bit Windows, in Git Bash." ;;
esac

RUST=1.100.0
RUSTUP=1.29.3
RUSTUP_SHA256=38027d51d69a5d05493627a0634dbea52d6cd793c8f072d2373d3f4163eec7db
NODE=22.23.3
NODE_SHA256=2b0ff57b049cda1bbcea2240eec20467018713c1efe1f7360c2681859b90ed71

EXE=Rusty-Business-windows-x64-setup.exe
DESKTOP=apps/desktop
TAURI=$DESKTOP/src-tauri
WORK=$(pwd -P)/$TAURI/target/windows
SYSTEM=$(cygpath -u "${SYSTEMROOT:-${SystemRoot:-C:\\Windows}}")/System32

# What a shell may hold that would change the build: Rust's and Cargo's
# settings, Node's and npm's, Tauri's, the C compiler's and linker's, and a
# CARGO_TARGET_DIR that would put the build somewhere else.
while IFS= read -r name; do
  case "$name" in
    CARGO* | RUST* | NODE* | NPM* | npm_* | TAURI* | VITE* | CC | CXX | CC_* | CXX_* | CFLAGS* | CXXFLAGS* | \
      CPPFLAGS* | LDFLAGS* | AR | CL | _CL_ | LINK | _LINK_ | INCLUDE | LIB | LIBPATH | VCINSTALLDIR | VCToolsInstallDir)
      unset "$name"
      ;;
  esac
done < <(compgen -e)
export PATH=/mingw64/bin:/usr/bin:/bin:$SYSTEM:$SYSTEM/Wbem:$SYSTEM/WindowsPowerShell/v1.0

mkdir -p "$WORK/downloads" "$WORK/tools"

# `check SUM FILE` is true when the file's SHA-256 is SUM.
check() {
  [ "$(sha256sum "$2" | cut -c1-64)" = "$1" ]
}

# `fetch URL SUM NAME` downloads into target/windows/downloads, over HTTPS
# only, and keeps the file only when its checksum is the one pinned here.
fetch() {
  if [ -f "$WORK/downloads/$3" ] && check "$2" "$WORK/downloads/$3"; then
    return
  fi
  rm -f "$WORK/downloads/$3" "$WORK/downloads/$3.part"
  echo "Downloading $1"
  curl --fail --location --silent --show-error --proto '=https' --proto-redir '=https' --tlsv1.2 \
    --output "$WORK/downloads/$3.part" "$1"
  check "$2" "$WORK/downloads/$3.part" || fail "$1 is not the file pinned in scripts/windows-app.sh."
  mv "$WORK/downloads/$3.part" "$WORK/downloads/$3"
}

# --- The tools: Node and Rust of the folder's own ---------------------------

node_home=$WORK/tools/node-$NODE
if [ ! -f "$node_home/node.exe" ]; then
  fetch "https://nodejs.org/dist/v$NODE/node-v$NODE-win-x64.zip" "$NODE_SHA256" node.zip
  rm -rf "$WORK/tools/node-$NODE" "$WORK/tools/node-v$NODE-win-x64"
  # Windows' own tar reads a .zip; Git Bash has no unzip.
  "$SYSTEM/tar.exe" -xf "$(cygpath -w "$WORK/downloads/node.zip")" -C "$(cygpath -w "$WORK/tools")"
  mv "$WORK/tools/node-v$NODE-win-x64" "$node_home"
fi

# Rust is a Windows program, so it is told its folders the Windows way.
export RUSTUP_HOME CARGO_HOME
RUSTUP_HOME=$(cygpath -w "$WORK/tools/rustup")
CARGO_HOME=$(cygpath -w "$WORK/tools/cargo")
export RUSTUP_TOOLCHAIN=$RUST
cargo_bin=$WORK/tools/cargo/bin
if [ ! -f "$cargo_bin/rustup.exe" ]; then
  fetch "https://static.rust-lang.org/rustup/archive/$RUSTUP/x86_64-pc-windows-msvc/rustup-init.exe" \
    "$RUSTUP_SHA256" rustup-init.exe
  # --no-modify-path: the account's own PATH is left as it is.
  "$WORK/downloads/rustup-init.exe" -y --quiet --no-modify-path --profile minimal --default-toolchain none
fi
"$cargo_bin/rustup.exe" toolchain list | grep -q "^$RUST-" ||
  "$cargo_bin/rustup.exe" toolchain install "$RUST" --profile minimal --no-self-update

export PATH=$cargo_bin:$node_home:$PATH
export npm_config_cache npm_config_userconfig
npm_config_cache=$(cygpath -w "$WORK/tools/npm-cache")
npm_config_userconfig=$(cygpath -w "$WORK/tools/npmrc")
export npm_config_audit=false npm_config_fund=false npm_config_update_notifier=false

# --- The installer ----------------------------------------------------------

APP=$(node -p "require('./$TAURI/tauri.conf.json').productName")
VERSION=$(node -p "require('./$TAURI/tauri.conf.json').version")

(cd "$DESKTOP" && { [ -d node_modules ] || npm ci; } && npm run tauri -- build --bundles nsis)

built="$TAURI/target/release/bundle/nsis/${APP}_${VERSION}_x64-setup.exe"
[ -f "$built" ] || fail "Tauri made no $built."
rm -f "$WORK/$EXE" "$WORK/$EXE.sha256"
cp "$built" "$WORK/$EXE"
# Written out by hand: Git Bash's sha256sum marks the name with a *, and the
# line here must read the same as the Mac's.
printf '%s  %s\n' "$(sha256sum "$WORK/$EXE" | cut -c1-64)" "$EXE" >"$WORK/$EXE.sha256"
echo "Built $WORK/$EXE"
