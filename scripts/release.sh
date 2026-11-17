#!/usr/bin/env bash
# Puts the Mac app, the Windows installer, and the Linux packages on a GitHub
# release, where the README's download links find them. The Release workflow
# runs it after a green gate, with the .dmg from `make dmg`, the .exe from
# scripts/windows-app.sh, the .deb, the .rpm, and the .tar.gz from
# scripts/linux-app.sh, and their checksums downloaded into one folder:
#
#   scripts/release.sh publish dist
#
# It checks that the app's three version numbers agree, that the folder
# holds the five downloads and their checksums and nothing else, and that each
# checksum matches its download. It stops when the version's tag is already
# on GitHub: a release that is out is never changed, so publishing again
# needs a new version. Then it makes release v<version> at this commit, with
# the ten files, marked as the newest, which is the one the README links.
#
# It reads GITHUB_REPOSITORY and GITHUB_SHA, which GitHub sets, and GH_TOKEN,
# which the workflow passes on. Nothing here removes, edits, or replaces a
# release or a tag.
set -euo pipefail
cd "$(dirname "$0")/.."

DMG=Rusty-Business-mac-arm64.dmg
EXE=Rusty-Business-windows-x64-setup.exe
DEB=Rusty-Business-linux-x64.deb
RPM=Rusty-Business-linux-x64.rpm
TGZ=Rusty-Business-linux-x64.tar.gz
# The five, in the order the release lists them.
DOWNLOADS=("${DMG}" "${EXE}" "${DEB}" "${RPM}" "${TGZ}")
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
    local version="$1"
    cat <<NOTES
Rusty Business™ ${version}, for a Mac, for Windows, and for Linux.

On a Mac with Apple silicon and macOS 14 or later, with its model engine inside:

1. Download ${DMG}, open it, and drag Rusty Business™ to Applications.
2. Open the app. It is not signed yet, so macOS stops it the first time: open System Settings, then Privacy & Security, and choose Open Anyway.

On 64-bit Windows 10 or 11. The app has no model engine for Windows yet, so the window opens and says so.

1. Download ${EXE} and open it.
2. It is not signed yet, so Windows may stop it the first time: choose More info, then Run anyway.

On 64-bit Linux on an Intel or AMD processor. The app has no model engine for Linux yet, so the window opens and says so.

- On Ubuntu 22.04 or later, or Debian 12 or later, download ${DEB} and run: sudo apt install ./${DEB}
- On Fedora, download ${RPM} and run: sudo dnf install ./${RPM}
- Anywhere else, download ${TGZ}, unpack it, and run ${TGZ%.tar.gz}/bin/rusty-business. It needs WebKitGTK 4.1, OpenSSL 3, and glibc 2.35 or later from the system.

To check a download first, put it and its .sha256 file in one folder. On a Mac, run:

    shasum -a 256 -c ${DMG}.sha256

On Windows, in PowerShell, run this, which prints True when the two match:

    (Get-FileHash ${EXE}).Hash -eq (Get-Content ${EXE}.sha256).Split()[0]

On Linux, run this, with the name of the file you downloaded:

    sha256sum -c ${DEB}.sha256

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
    listed="$(cd "${dir}" && ls -A | LC_ALL=C sort | tr '\n' ' ')"
    expected="$(printf '%s\n' "${DOWNLOADS[@]}" "${DOWNLOADS[@]/%/.sha256}" | LC_ALL=C sort | tr '\n' ' ')"
    [ "${listed}" = "${expected}" ] ||
        fail "${dir} should hold ${DMG}, ${EXE}, ${DEB}, ${RPM}, ${TGZ}, and their .sha256 files, and nothing else, and holds: ${listed}"
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
