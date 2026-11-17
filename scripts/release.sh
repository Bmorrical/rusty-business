#!/usr/bin/env bash
# Puts the Mac app on a GitHub release, where the README's download link
# finds it. The Release workflow runs it after a green gate, with the .dmg
# and its checksum from `make dmg` downloaded into one folder:
#
#   scripts/release.sh publish dist
#
# It checks that the app's three version numbers agree, that the folder
# holds the .dmg and its checksum and nothing else, and that the checksum
# matches the .dmg. It stops when the version's tag is already on GitHub: a
# release that is out is never changed, so publishing again needs a new
# version. Then it makes release v<version> at this commit, with the two
# files, marked as the newest, which is the one the README links.
#
# It reads GITHUB_REPOSITORY and GITHUB_SHA, which GitHub sets, and GH_TOKEN,
# which the workflow passes on. Nothing here removes, edits, or replaces a
# release or a tag.
set -euo pipefail
cd "$(dirname "$0")/.."

DMG=Rusty-Business-mac-arm64.dmg
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
Rusty Business™ ${version} for a Mac with Apple silicon and macOS 14 or later, with its model engine inside.

1. Download ${DMG}, open it, and drag Rusty Business™ to Applications.
2. Open the app. It is not signed yet, so macOS stops it the first time: open System Settings, then Privacy & Security, and choose Open Anyway.

To check the download first, put both files in one folder and run:

    shasum -a 256 -c ${DMG}.sha256

Built from https://github.com/${repository}/commit/${commit} by the Release workflow, after a green gate.
NOTES
}

publish() {
    local dir="$1" version tag listed name sum found
    version="$(version_of_app)"
    tag="v${version}"

    [ -d "${dir}" ] || fail "${dir} is not a folder"
    listed="$(cd "${dir}" && ls -A | LC_ALL=C sort | tr '\n' ' ')"
    [ "${listed}" = "${DMG} ${DMG}.sha256 " ] ||
        fail "${dir} should hold ${DMG} and ${DMG}.sha256 and nothing else, and holds: ${listed}"
    for name in "${DMG}" "${DMG}.sha256"; do
        [ -f "${dir}/${name}" ] && [ ! -L "${dir}/${name}" ] && [ -s "${dir}/${name}" ] ||
            fail "${dir}/${name} is not a plain file with something in it"
    done
    sum="$(sha256_of "${dir}/${DMG}")"
    [[ "${sum}" =~ ^[0-9a-f]{64}$ ]] || fail "${DMG} gave no plain checksum: ${sum}"
    [ "$(cat "${dir}/${DMG}.sha256")" = "${sum}  ${DMG}" ] ||
        fail "${DMG}.sha256 does not give the checksum of the ${DMG} beside it"

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
        "${dir}/${DMG}" "${dir}/${DMG}.sha256"
    echo "Released ${tag}: https://github.com/${repository}/releases/tag/${tag}"
    if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
        printf '### Released %s\n\n- `%s`: `%s`\n' "${tag}" "${DMG}" "${sum}" >>"${GITHUB_STEP_SUMMARY}"
    fi
}

case "${1:-}" in
    publish)
        [ "$#" -eq 2 ] || fail "${USAGE}"
        publish "$2"
        ;;
    *) fail "${USAGE}" ;;
esac
