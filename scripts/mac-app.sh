#!/usr/bin/env bash
# Builds the Mac app with its engine inside, from this checkout alone.
#
#   scripts/mac-app.sh app   builds the app and opens it    (or: make app)
#   scripts/mac-app.sh dmg   builds the app and packs a .dmg (or: make dmg),
#                            and the .app as a .tar.gz, the file Install and
#                            restart puts in place
#
# Brad, 2026-11-17: "this should be must easier for the user to boot up, run
# it in isolation, one command to rule it all, start a makefile"
#
# Everything it fetches or builds stays in apps/desktop/src-tauri/target/mac,
# which git ignores: its own Rust, Node, and Python, each pinned by version and
# checksum, and the engine's packages, each pinned by hash. It installs nothing
# in the home folder or on the system, and it runs with none of this account's
# own tools or settings. Apple's command line tools are the one thing it needs
# from the Mac, and macOS installs those itself when asked.
#
# Safe to run again: what is already fetched and still matches its pin is
# kept, and the engine is built again only when its pins or this script change.
set -euo pipefail
cd "$(dirname "$0")/.."

# Run again with nothing from this account's environment but who it is, so a
# PATH, a RUSTFLAGS, a NODE_OPTIONS, or a PYTHONPATH left in a shell cannot
# change what is built.
if [ "${MAC_APP_CLEAN:-}" != 1 ]; then
  exec /usr/bin/env -i MAC_APP_CLEAN=1 HOME="$HOME" USER="${USER:-}" LOGNAME="${LOGNAME:-}" \
    TMPDIR="${TMPDIR:-/tmp}" LANG=en_US.UTF-8 PATH=/usr/bin:/bin:/usr/sbin:/sbin \
    /bin/bash scripts/mac-app.sh "$@"
fi

fail() {
  echo "$1" >&2
  exit 1
}

case "${1:-}" in
  app | dmg) goal=$1 ;;
  *) fail "Say what to build: scripts/mac-app.sh app, or scripts/mac-app.sh dmg." ;;
esac

# The oldest macOS the app runs on. MLX ships nothing older, and the app's
# own setting, bundle.macOS.minimumSystemVersion, must say the same.
MACOS=14.0

RUST=1.100.0
RUSTUP=1.29.3
RUSTUP_SHA256=63752a1919a3a4793173ec70f8f212621a7ce38ba6a246095984482f4de20f9f
NODE=22.23.3
NODE_SHA256=23b25245dcfb9af7262f8ff142e9e2e0af025368117329e7a7458a51e5922f53
PYTHON_URL=https://github.com/astral-sh/python-build-standalone/releases/download/20261003/cpython-3.12.15%2B20261003-aarch64-apple-darwin-install_only_stripped.tar.gz
PYTHON_SHA256=ad8d0c637c0a36b967b310e2c07254f4d2ca8cabaa7699e55ed6290aceb481a2

DMG=Rusty-Business-mac-arm64.dmg
APP_TGZ=Rusty-Business-mac-arm64.app.tar.gz
DESKTOP=apps/desktop
TAURI=$DESKTOP/src-tauri
WORK=$(pwd -P)/$TAURI/target/mac

[ "$(uname -s)/$(uname -m)" = Darwin/arm64 ] || fail "The Mac app is built on a Mac with Apple silicon."
{ xcode-select --print-path && xcrun --find clang; } >/dev/null 2>&1 ||
  fail "Apple's command line tools are missing. Run xcode-select --install, then this again."

APP=$(/usr/bin/plutil -extract productName raw -o - "$TAURI/tauri.conf.json")
minimum=$(/usr/bin/plutil -extract bundle.macOS.minimumSystemVersion raw -o - "$TAURI/tauri.conf.json")
[ "$minimum" = "$MACOS" ] || fail "tauri.conf.json says macOS $minimum, and this script $MACOS."

mkdir -p "$WORK/downloads" "$WORK/tools"

# `check SUM FILE` is true when the file's SHA-256 is SUM.
check() {
  [ "$(shasum -a 256 "$2" | cut -c1-64)" = "$1" ]
}

# `fetch URL SUM NAME` downloads into target/mac/downloads, over HTTPS only,
# and keeps the file only when its checksum is the one pinned here.
fetch() {
  if [ -f "$WORK/downloads/$3" ] && check "$2" "$WORK/downloads/$3"; then
    return
  fi
  rm -f "$WORK/downloads/$3" "$WORK/downloads/$3.part"
  echo "Downloading $1"
  curl --fail --location --silent --show-error --proto '=https' --proto-redir '=https' --tlsv1.2 \
    --output "$WORK/downloads/$3.part" "$1"
  check "$2" "$WORK/downloads/$3.part" || fail "$1 is not the file pinned in scripts/mac-app.sh."
  mv "$WORK/downloads/$3.part" "$WORK/downloads/$3"
}

# --- The tools: Node and Rust of the folder's own ---------------------------

node_home=$WORK/tools/node-$NODE
if [ ! -x "$node_home/bin/node" ]; then
  fetch "https://nodejs.org/dist/v$NODE/node-v$NODE-darwin-arm64.tar.gz" "$NODE_SHA256" node.tar.gz
  rm -rf "$WORK/tools/node-$NODE" "$WORK/tools/node-v$NODE-darwin-arm64"
  tar -xzf "$WORK/downloads/node.tar.gz" -C "$WORK/tools"
  mv "$WORK/tools/node-v$NODE-darwin-arm64" "$node_home"
fi

export RUSTUP_HOME=$WORK/tools/rustup
export CARGO_HOME=$WORK/tools/cargo
export RUSTUP_TOOLCHAIN=$RUST
if [ ! -x "$CARGO_HOME/bin/rustup" ]; then
  fetch "https://static.rust-lang.org/rustup/archive/$RUSTUP/aarch64-apple-darwin/rustup-init" "$RUSTUP_SHA256" rustup-init
  chmod 755 "$WORK/downloads/rustup-init"
  "$WORK/downloads/rustup-init" -y --quiet --no-modify-path --profile minimal --default-toolchain none
fi
"$CARGO_HOME/bin/rustup" toolchain list | grep -q "^$RUST-" ||
  "$CARGO_HOME/bin/rustup" toolchain install "$RUST" --profile minimal --no-self-update

export PATH=$CARGO_HOME/bin:$node_home/bin:/usr/bin:/bin:/usr/sbin:/sbin
export npm_config_cache=$WORK/tools/npm-cache
export npm_config_userconfig=$WORK/tools/npmrc
export npm_config_audit=false npm_config_fund=false npm_config_update_notifier=false

# --- The engine: Python, MLX, and nothing from this account -----------------

engine=$WORK/engine
python=$engine/bin/python3
pins=$(cat "$DESKTOP/engine/requirements.txt" scripts/mac-app.sh | shasum -a 256 | cut -c1-64)
if [ ! -x "$python" ] || [ "$(cat "$WORK/engine.pins" 2>/dev/null)" != "$pins" ]; then
  fetch "$PYTHON_URL" "$PYTHON_SHA256" python.tar.gz
  rm -rf "$WORK/engine" "$WORK/engine.part" "$WORK/engine.pins"
  mkdir "$WORK/engine.part"
  tar -xzf "$WORK/downloads/python.tar.gz" -C "$WORK/engine.part"
  mv "$WORK/engine.part/python" "$engine"
  rmdir "$WORK/engine.part"

  site=$WORK/engine/lib/python3.12/site-packages
  [ "$("$python" -I -c 'import sysconfig; print(sysconfig.get_paths()["purelib"])')" = "$site" ] ||
    fail "The pinned Python keeps its packages somewhere other than $site."
  # Wheels only, each matching a hash in requirements.txt, built for the
  # oldest macOS the app runs on rather than for this Mac.
  "$python" -I -m pip install --isolated --no-input --disable-pip-version-check \
    --cache-dir "$WORK/tools/pip-cache" --require-hashes --only-binary :all: \
    --no-compile --no-warn-script-location --index-url https://pypi.org/simple \
    --platform "macosx_${MACOS/./_}_arm64" --target "$site" \
    --requirement "$DESKTOP/engine/requirements.txt"
  "$python" -I -m pip check --disable-pip-version-check
  rm -rf "$WORK/engine/lib/python3.12/site-packages/bin"

  # Every program and library must run on the oldest macOS the app allows.
  # The shell scripts beside them have no such setting and are let be.
  while IFS= read -r -d '' file; do
    file -b "$file" | grep -q '^Mach-O' || continue
    needs=$(vtool -arch arm64 -show-build "$file" | awk '$1 == "minos" || $1 == "version" { print $2; exit }') ||
      fail "${file#"$engine/"} could not be read for the macOS it needs."
    [ -n "$needs" ] || fail "${file#"$engine/"} does not say which macOS it needs."
    awk -v needs="$needs" -v allowed="$MACOS" 'BEGIN {
      split(needs, n, "."); split(allowed, a, ".")
      exit !(n[1] + 0 < a[1] + 0 || (n[1] + 0 == a[1] + 0 && n[2] + 0 <= a[2] + 0))
    }' || fail "${file#"$engine/"} needs macOS $needs, newer than $MACOS."
  done < <(find "$engine" -type f \( -name '*.so' -o -name '*.dylib' -o -path '*/bin/*' \) -print0)

  # Compiled once here, so the app never writes inside itself.
  "$python" -I -m compileall -q -f -j0 --invalidation-mode unchecked-hash "$engine/lib"

  cat >"$engine/bin/mlx_lm.server" <<'WRAPPER'
#!/bin/sh
# The engine this app carries. -I keeps this computer's own Python settings out, and -B writes nothing into the app.
exec "$(dirname "$0")/python3" -I -B -m mlx_lm.server "$@"
WRAPPER
  chmod 755 "$engine/bin/mlx_lm.server"

  # It starts, with no home folder of this account's and no network.
  rm -rf "$WORK/home"
  mkdir -p "$WORK/home"
  /usr/bin/env -i HOME="$WORK/home" HF_HOME="$WORK/home/hf" HF_HUB_OFFLINE=1 PATH=/usr/bin:/bin \
    "$engine/bin/mlx_lm.server" --help >/dev/null
  echo "$pins" >"$WORK/engine.pins"
fi

# --- The app, with the engine inside ----------------------------------------

(cd "$DESKTOP" && { [ -d node_modules ] || npm ci; } && npm run tauri -- build --bundles app)

built=$TAURI/target/release/bundle/macos/$APP.app
app=$WORK/app/$APP.app
rm -rf "$WORK/app"
mkdir -p "$WORK/app"
ditto "$built" "$app"
ditto "$engine" "$app/Contents/Resources/engine"
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"

if [ "$goal" = app ]; then
  echo "Opening $app"
  open "$app"
  exit 0
fi

# --- The .app as a .tar.gz, for Install and restart ------------------------
#
# Brad, 2026-11-19: "when can we do an in app install rather than a new dmg
# download?" The updater unpacks this and puts the one .app inside in place,
# so the archive holds that folder alone, with no Finder or extended
# attributes of this Mac's.

rm -f "$WORK/$APP_TGZ" "$WORK/$APP_TGZ.sha256"
tar --no-mac-metadata --no-xattrs --no-acls --no-fflags -czf "$WORK/$APP_TGZ" -C "$WORK/app" "$APP.app"
(cd "$WORK" && shasum -a 256 "$APP_TGZ" >"$APP_TGZ.sha256")

# --- The .dmg ---------------------------------------------------------------

# Brad, 2026-11-20, of the 0.7.0 release run: "failed". Its Mac build packed
# the app, then stopped in hdiutil with no word of why, since -quiet closes
# hdiutil's error output as well as its own. So its own output goes and its
# errors stay, and a pack that fails is tried again, twice, because a build
# machine's disk is now and then busy for a moment. mac-sign.sh holds the
# same lines, and packs the .dmg again with them.
pack_dmg() {
  local try
  for try in 1 2 3; do
    if hdiutil create -ov -volname "${APP}™" -srcfolder "$1" -format ULMO -o "$WORK/$DMG" >/dev/null; then
      hdiutil verify "$WORK/$DMG" >/dev/null
      return
    fi
    [ "$try" = 3 ] || { echo "hdiutil could not pack the .dmg, try $try of 3. Trying again in 20 seconds." >&2; sleep 20; }
  done
  fail "hdiutil could not pack the .dmg in 3 tries. What it said is above."
}

ln -s /Applications "$WORK/app/Applications"
rm -f "$WORK/$DMG" "$WORK/$DMG.sha256"
pack_dmg "$WORK/app"
(cd "$WORK" && shasum -a 256 "$DMG" >"$DMG.sha256")
echo "Built $WORK/$DMG and $WORK/$APP_TGZ"
