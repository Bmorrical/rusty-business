#!/usr/bin/env bash
# Puts the Mac app, the Windows installer, and the Linux packages on a GitHub
# release, where the README's download links find them, and the list
# Install and restart reads. The Release workflow runs it after a green gate,
# with the .dmg and the .app.tar.gz from `make dmg`, the .exe from
# scripts/windows-app.sh, the .deb, the .rpm, and the .tar.gz from
# scripts/linux-app.sh, and their checksums, and latest.json from
# scripts/updater.mjs, downloaded into one folder:
#
#   scripts/release.sh publish dist
#
# It checks that the app's three version numbers agree, that the folder
# holds the six downloads and their checksums and latest.json and nothing
# else, and that each checksum matches its download. The workflow has checked
# latest.json's signatures already, with scripts/updater.mjs. It stops when
# the version's tag is already on GitHub: a release that is out is never
# changed, so publishing again needs a new version. Then it makes release
# v<version> at this commit, with the thirteen files, marked as the newest,
# which is the one the README links and Install and restart reads.
#
# It reads GITHUB_REPOSITORY and GITHUB_SHA, which GitHub sets, GH_TOKEN,
# which the workflow passes on, and MAC_SIGNED, true or false, from the sign
# job, which picks the notes' steps to open the Mac app. Nothing here removes,
# edits, or replaces a release or a tag.
set -euo pipefail
cd "$(dirname "$0")/.."

DMG=Rusty-Business-mac-arm64.dmg
APP_TGZ=Rusty-Business-mac-arm64.app.tar.gz
EXE=Rusty-Business-windows-x64-setup.exe
DEB=Rusty-Business-linux-x64.deb
RPM=Rusty-Business-linux-x64.rpm
TGZ=Rusty-Business-linux-x64.tar.gz
# The six, in the order the release lists them, and the list Install and
# restart reads, which goes last.
DOWNLOADS=("${DMG}" "${APP_TGZ}" "${EXE}" "${DEB}" "${RPM}" "${TGZ}")
MANIFEST=latest.json
USAGE="use: scripts/release.sh publish <folder>"

fail() {
    echo "release: $*" >&2
    exit 1
}

repository="${GITHUB_REPOSITORY:-}"
[[ "${repository}" =~ ^[A-Za-z0-9-]+/[A-Za-z0-9_-][A-Za-z0-9._-]*$ ]] ||
    fail "GITHUB_REPOSITORY is not owner/name: ${repository}"
commit="${GITHUB_SHA:-}"
[[ "${commit}" =~ ^[0-9a-f]{40}$ ]] ||
    fail "GITHUB_SHA is not a whole commit: ${commit}"
[ -n "${GH_TOKEN:-}" ] || fail "GH_TOKEN is not set, so no release can be made"
mac_signed="${MAC_SIGNED:-}"
[[ "${mac_signed}" =~ ^(true|false)$ ]] ||
    fail "MAC_SIGNED is true or false, from the sign job, and is: ${mac_signed}"

# The app's version, from the three files that say it. They must agree.
version_of_app() {
    local package tauri cargo
    package="$(sed -n -E 's/^  "version": "([^"]*)",$/\1/p' apps/desktop/package.json)"
    tauri="$(sed -n -E 's/^  "version": "([^"]*)",$/\1/p' apps/desktop/src-tauri/tauri.conf.json)"
    cargo="$(sed -n -E '/^\[package\]$/,/^\[/ s/^version = "([^"]*)"$/\1/p' apps/desktop/src-tauri/Cargo.toml)"
    [[ "${package}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] ||
        fail "apps/desktop/package.json gives no plain version, such as 1.2.3: ${package}"
    [ "${tauri}" = "${package}" ] && [ "${cargo}" = "${package}" ] ||
        fail "the app's versions differ: package.json ${package}, tauri.conf.json ${tauri}, Cargo.toml ${cargo}"
    printf '%s' "${package}"
}

sha256_of() {
    local sum
    if command -v sha256sum >/dev/null 2>&1; then
        sum="$(sha256sum "$1")"
    else
        sum="$(shasum -a 256 "$1")"
    fi
    printf '%s' "${sum%% *}"
}

notes_for() {
    local version="$1" mac_open
    if [ "${mac_signed}" = true ]; then
        mac_open="2. Open the app. It is signed, and notarized by Apple, so macOS asks once whether to open an app from the internet: choose Open."
    else
        mac_open="2. Open the app. It is not signed yet, so macOS stops it the first time: open System Settings, then Privacy & Security, and choose Open Anyway."
    fi
    cat <<NOTES
Rusty Business™ ${version}, for a Mac, for Windows, and for Linux.

On a Mac with Apple silicon and macOS 14 or later, with its model engine inside:

1. Download ${DMG}, open it, and drag Rusty Business™ to Applications.
${mac_open}

On 64-bit Windows 10 or 11. The installer carries no model engine yet: the app starts llama.cpp's llama-server when it finds it on this computer, and otherwise the window opens and says so.

1. Download ${EXE} and open it.
2. It is not signed yet, so Windows may stop it the first time: choose More info, then Run anyway.

On 64-bit Linux on an Intel or AMD processor. The three files carry the engine inside them, llama.cpp's llama-server, the Vulkan build pinned by checksum.

- On Ubuntu 22.04 or later, or Debian 12 or later, download ${DEB} and run: sudo apt install ./${DEB}
- On Fedora, download ${RPM} and run: sudo dnf install ./${RPM}
- Anywhere else, download ${TGZ}, unpack it, and run ${TGZ%.tar.gz}/bin/rusty-business. It needs WebKitGTK 4.1, OpenSSL 3, the Vulkan loader, the OpenMP runtime, and glibc 2.35 or later from the system.

To check a download first, put it and its .sha256 file in one folder. On a Mac, run:

    shasum -a 256 -c ${DMG}.sha256

On Windows, in PowerShell, run this, which prints True when the two match:

    (Get-FileHash ${EXE}).Hash -eq (Get-Content ${EXE}.sha256).Split()[0]

On Linux, run this, with the name of the file you downloaded:

    sha256sum -c ${DEB}.sha256

A copy of 0.8.0 or later that finds a newer version offers Install and restart: it reads ${MANIFEST}, fetches this system's file (on a Mac, ${APP_TGZ}), and installs it only when its signature matches the key built into the app. A copy unpacked from ${TGZ} updates by download.

Built from https://github.com/${repository}/commit/${commit} by the Release workflow, after a green gate.
NOTES
}

publish() {
    local dir="$1" version tag listed expected name sum found i
    local -a sums=() files=()
    version="$(version_of_app)"
    tag="v${version}"

    [ -d "${dir}" ] || fail "${dir} is not a folder"
    for name in "${DOWNLOADS[@]}"; do
        files+=("${dir}/${name}" "${dir}/${name}.sha256")
    done
    files+=("${dir}/${MANIFEST}")
    listed="$(cd "${dir}" && ls -A | LC_ALL=C sort | tr '\n' ' ')"
    expected="$(printf '%s\n' "${DOWNLOADS[@]}" "${DOWNLOADS[@]/%/.sha256}" "${MANIFEST}" | LC_ALL=C sort | tr '\n' ' ')"
    [ "${listed}" = "${expected}" ] ||
        fail "${dir} should hold ${DMG}, ${APP_TGZ}, ${EXE}, ${DEB}, ${RPM}, ${TGZ}, their .sha256 files, and ${MANIFEST}, and nothing else, and holds: ${listed}"
    for name in "${files[@]}"; do
        [ -f "${name}" ] && [ ! -L "${name}" ] && [ -s "${name}" ] ||
            fail "${name} is not a plain file with something in it"
    done
    for name in "${DOWNLOADS[@]}"; do
        sum="$(sha256_of "${dir}/${name}")"
        [[ "${sum}" =~ ^[0-9a-f]{64}$ ]] || fail "${name} gave no plain checksum: ${sum}"
        [ "$(cat "${dir}/${name}.sha256")" = "${sum}  ${name}" ] ||
            fail "${name}.sha256 does not give the checksum of the ${name} beside it"
        sums+=("${sum}")
    done

    # GitHub answers with every tag whose name starts with this one, so only
    # an exact match counts.
    found="$(gh api "repos/${repository}/git/matching-refs/tags/${tag}" \
        --jq ".[] | select(.ref == \"refs/tags/${tag}\") | .ref")" ||
        fail "could not ask GitHub whether ${tag} is already there"
    [ -z "${found}" ] ||
        fail "${tag} is already on GitHub, and a release that is out is never changed: raise the version to publish again"

    gh release create "${tag}" \
        --repo "${repository}" \
        --target "${commit}" \
        --title "Rusty Business™ ${version}" \
        --notes "$(notes_for "${version}")" \
        --latest \
        "${files[@]}"
    echo "Released ${tag}: https://github.com/${repository}/releases/tag/${tag}"
    if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
        {
            printf '### Released %s\n\n' "${tag}"
            for i in "${!DOWNLOADS[@]}"; do
                printf -- '- `%s`: `%s`\n' "${DOWNLOADS[i]}" "${sums[i]}"
            done
        } >>"${GITHUB_STEP_SUMMARY}"
    fi
}

case "${1:-}" in
    publish)
        [ "$#" -eq 2 ] || fail "${USAGE}"
        publish "$2"
        ;;
    *) fail "${USAGE}" ;;
esac
