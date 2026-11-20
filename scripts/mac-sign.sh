#!/usr/bin/env bash
# Signs the Mac app and its .dmg with a Developer ID, and has Apple notarize
# both. It runs in the Release run's sign job, on a Mac of its own, with the
# .dmg the mac job built. It is a switch, off to start:
#
#   MAC_SIGNING unset, empty, or off   the .dmg stays as `make dmg` made it,
#                                       signed for itself alone
#   MAC_SIGNING=on                      signed, notarized, and stapled
#
# Brad, 2026-11-19: "keep shipping self-signed as an option until all the
# peices are ready, when ready we can light it up". Then: "self signed for
# now".
#
# With the switch on, it reads five secrets from its environment, and stops
# before it signs anything when one is missing, so a release never ships the
# download unsigned by mistake:
#
#   MAC_CERTIFICATE           the Developer ID Application certificate with
#                             its key, as a .p12, in base64
#   MAC_CERTIFICATE_PASSWORD  the .p12's password
#   MAC_NOTARY_KEY            an App Store Connect API key: the .p8's text
#   MAC_NOTARY_KEY_ID         that key's ID
#   MAC_NOTARY_ISSUER         that key's issuer ID
#
# The job that builds the app runs the packages it fetches, and never sees a
# secret. This one runs none of them: it takes the app out of the .dmg, signs
# every program file in it, the engine's included, with the hardened runtime
# and a secure timestamp, and no entitlement, then signs the app, which seals
# them. The engine runs under the hardened runtime as it is, once every part
# carries the same signer. Apple notarizes the app, the ticket is stapled to
# it, the .app's .tar.gz (the file Install and restart puts in place) and the
# .dmg are packed again around it the way `make dmg` packs them, and the .dmg
# is signed, notarized, and stapled too.
#
# It signs only on GitHub's own Mac, in a Release run. The certificate goes
# into a keychain of its own, made for this run in the runner's temporary
# folder, and the keychain and the key's file are removed when it ends. It
# never touches the keychain of the account it runs in.
#
# It tells the next job what it did, signed=true or signed=false, so the
# release notes give the right steps to open the app.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH=/usr/bin:/bin:/usr/sbin:/sbin

fail() {
  echo "$1" >&2
  exit 1
}

# `tell NAME VALUE` passes a value on to the next job, in a GitHub run.
tell() {
  if [ -n "${GITHUB_OUTPUT:-}" ]; then
    echo "$1=$2" >>"$GITHUB_OUTPUT"
  fi
}

[ "$#" -eq 0 ] || fail "scripts/mac-sign.sh takes no argument. The switch is MAC_SIGNING, on or off."

case "${MAC_SIGNING:-}" in
  "" | off)
    echo "Mac signing is off, so the .dmg stays as make dmg made it, signed for itself alone."
    tell signed false
    exit 0
    ;;
  on) ;;
  *) fail "MAC_SIGNING is on or off, and it is \"$MAC_SIGNING\". Nothing was signed." ;;
esac

# --- Everything it needs, before it does anything ----------------------------

[ "${GITHUB_ACTIONS:-}" = true ] && [ -n "${RUNNER_TEMP:-}" ] && [ -d "${RUNNER_TEMP:-}" ] ||
  fail "Mac signing runs only in GitHub's Release run, never on a computer of its own. Nothing was signed."

missing=""
for name in MAC_CERTIFICATE MAC_CERTIFICATE_PASSWORD MAC_NOTARY_KEY MAC_NOTARY_KEY_ID MAC_NOTARY_ISSUER; do
  [ -n "${!name:-}" ] || missing="$missing $name"
done
[ -z "$missing" ] || fail "Mac signing is on, and these secrets are missing:$missing. Nothing was signed."
[[ "$MAC_NOTARY_KEY_ID" =~ ^[A-Z0-9]{10}$ ]] ||
  fail "MAC_NOTARY_KEY_ID is not an App Store Connect key ID, ten letters and digits. Nothing was signed."
[[ "$MAC_NOTARY_ISSUER" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$ ]] ||
  fail "MAC_NOTARY_ISSUER is not an App Store Connect issuer ID. Nothing was signed."
[[ "$MAC_NOTARY_KEY" == *"-----BEGIN PRIVATE KEY-----"* ]] ||
  fail "MAC_NOTARY_KEY is not the text of a .p8 key. Nothing was signed."

[ "$(uname -s)/$(uname -m)" = Darwin/arm64 ] || fail "Mac signing runs on a Mac with Apple silicon. Nothing was signed."

DMG=Rusty-Business-mac-arm64.dmg
APP_TGZ=Rusty-Business-mac-arm64.app.tar.gz
TAURI=apps/desktop/src-tauri
WORK=$(pwd -P)/$TAURI/target/mac
[ -f "$TAURI/tauri.conf.json" ] && [ -f "$WORK/$DMG" ] && [ -f "$WORK/$DMG.sha256" ] ||
  fail "There is no .dmg from make dmg, with its checksum, in $WORK. Nothing was signed."
(cd "$WORK" && shasum -a 256 -c -s "$DMG.sha256") || fail "The .dmg does not match its checksum. Nothing was signed."
APP=$(plutil -extract productName raw -o - "$TAURI/tauri.conf.json")

# --- A keychain for this run alone --------------------------------------------

umask 077
scratch=$(mktemp -d "$RUNNER_TEMP/mac-sign.XXXXXX")
keychain=$scratch/signing.keychain-db
mounted=""
# Only what this script made, and only here: the .dmg it opened, the
# keychain, and the key's file.
finish() {
  if [ -n "$mounted" ]; then
    hdiutil detach -quiet -force "$scratch/built" >/dev/null 2>&1 || true
  fi
  security delete-keychain "$keychain" >/dev/null 2>&1 || true
  rm -rf "$scratch"
}
trap finish EXIT

password=$(openssl rand -hex 24)
security create-keychain -p "$password" "$keychain"
security set-keychain-settings -lut 7200 "$keychain"
security unlock-keychain -p "$password" "$keychain"
printf '%s' "$MAC_CERTIFICATE" | base64 --decode >"$scratch/certificate.p12" ||
  fail "MAC_CERTIFICATE is not base64. Nothing was signed."
security import "$scratch/certificate.p12" -k "$keychain" -f pkcs12 -P "$MAC_CERTIFICATE_PASSWORD" -T /usr/bin/codesign >/dev/null ||
  fail "The .p12 did not open with MAC_CERTIFICATE_PASSWORD. Nothing was signed."
rm -f "$scratch/certificate.p12"
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$keychain" >/dev/null
# codesign finds the key through the search list, so this run's keychain goes
# first on it, ahead of the runner's own. Removing the keychain at the end
# takes it off the list again.
others=()
while IFS= read -r line; do
  line=${line#*\"}
  others+=("${line%\"}")
done < <(security list-keychains -d user)
security list-keychains -d user -s "$keychain" ${others[@]+"${others[@]}"}

found=$(security find-identity -v -p codesigning "$keychain" | grep '"Developer ID Application: ' || true)
[ "$(printf '%s' "$found" | grep -c .)" = 1 ] ||
  fail "The .p12 must hold exactly one Developer ID Application certificate, valid, with its key. Nothing was signed."
identity=$(printf '%s\n' "$found" | awk '{ print $2 }')
[[ "$identity" =~ ^[0-9A-F]{40}$ ]] || fail "The certificate gave no plain identity. Nothing was signed."

# `sign [OPTION...] PATH...` signs with this run's Developer ID and a secure
# timestamp from Apple.
sign() {
  codesign --force --timestamp --keychain "$keychain" --sign "$identity" "$@"
}

printf '%s\n' "$MAC_NOTARY_KEY" >"$scratch/notary.p8"
notary=(--key "$scratch/notary.p8" --key-id "$MAC_NOTARY_KEY_ID" --issuer "$MAC_NOTARY_ISSUER")

# `notarize FILE` sends the file to Apple and waits. Anything but Accepted
# prints Apple's log and stops the run.
notarize() {
  local answer status id
  echo "Sending $(basename "$1") to Apple to notarize, and waiting."
  answer=$(xcrun notarytool submit "$1" "${notary[@]}" --wait --timeout 30m --output-format json) || true
  status=$(printf '%s' "$answer" | plutil -extract status raw -o - - 2>/dev/null) || status=""
  if [ "$status" != Accepted ]; then
    id=$(printf '%s' "$answer" | plutil -extract id raw -o - - 2>/dev/null) || id=""
    if [[ "$id" =~ ^[0-9a-f-]{36}$ ]]; then
      xcrun notarytool log "$id" "${notary[@]}" >&2 || true
    fi
    fail "Apple did not accept $(basename "$1"): ${status:-no answer}. Nothing was published."
  fi
  echo "Apple accepted $(basename "$1")."
}

# --- The app, out of the .dmg -------------------------------------------------

mkdir "$scratch/built" "$scratch/app"
hdiutil attach -readonly -nobrowse -noautoopen -mountpoint "$scratch/built" "$WORK/$DMG" >/dev/null
mounted=1
[ -d "$scratch/built/$APP.app" ] || fail "The .dmg holds no $APP.app. Nothing was signed."
ditto "$scratch/built/$APP.app" "$scratch/app/$APP.app"
hdiutil detach "$scratch/built" >/dev/null
mounted=""
ln -s /Applications "$scratch/app/Applications"

app=$scratch/app/$APP.app
main=$app/Contents/MacOS/$(plutil -extract CFBundleExecutable raw -o - "$app/Contents/Info.plist")
[ -f "$main" ] || fail "The app has no program of its own at $main. Nothing was signed."

# Every program file inside it first, the engine's Python and its libraries
# among them, then the app, which seals them.
parts=0
while IFS= read -r -d '' path; do
  [ "$path" != "$main" ] || continue
  if file -b "$path" | grep -q '^Mach-O'; then
    sign --options runtime "$path"
    parts=$((parts + 1))
  fi
done < <(find "$app/Contents" -type f -print0)
[ "$parts" -gt 0 ] || fail "The app holds no program files but its own, so the engine is missing. Nothing was signed."
sign --options runtime "$app"
codesign --verify --deep --strict "$app"
echo "Signed the app and the $parts program files inside it."

ditto -c -k --keepParent "$app" "$scratch/app.zip"
notarize "$scratch/app.zip"
xcrun stapler staple "$app"
xcrun stapler validate "$app"
spctl --assess --type execute "$app"

# --- The .app's .tar.gz, packed again around the stapled app --------------------

rm -f "$WORK/$APP_TGZ" "$WORK/$APP_TGZ.sha256"
tar --no-mac-metadata --no-xattrs --no-acls --no-fflags -czf "$WORK/$APP_TGZ" -C "$scratch/app" "$APP.app"
(cd "$WORK" && shasum -a 256 "$APP_TGZ" >"$APP_TGZ.sha256")

# --- The .dmg, packed again around the stapled app ------------------------------

# Brad, 2026-11-20, of the 0.7.0 release run: "failed". Its Mac build packed
# the app, then stopped in hdiutil with no word of why, since -quiet closes
# hdiutil's error output as well as its own. So its own output goes and its
# errors stay, and a pack that fails is tried again, twice, because a build
# machine's disk is now and then busy for a moment. mac-app.sh holds the
# same lines, and packs the .dmg with them first.
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

rm -f "$WORK/$DMG" "$WORK/$DMG.sha256"
pack_dmg "$scratch/app"
sign "$WORK/$DMG"
codesign --verify --strict "$WORK/$DMG"
notarize "$WORK/$DMG"
xcrun stapler staple "$WORK/$DMG"
xcrun stapler validate "$WORK/$DMG"
spctl --assess --type open --context context:primary-signature "$WORK/$DMG"
(cd "$WORK" && shasum -a 256 "$DMG" >"$DMG.sha256")

tell signed true
echo "Signed, notarized, and stapled $WORK/$DMG"
