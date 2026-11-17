#!/usr/bin/env bash
# Puts the API and worker images on GitHub's container registry, where the
# desktop app is to pull them from when it starts the stack. The Deploy
# workflow runs it after a green gate, already signed in to the registry:
#
#   scripts/publish-images.sh build amd64    on an Intel or AMD computer
#   scripts/publish-images.sh build arm64    on an Arm computer
#   scripts/publish-images.sh join           once both builds are up
#
# `build` builds both services for the kind of computer it runs on, and
# pushes each under the commit's tag with that kind after it. Nothing is
# emulated: asked for the other kind, it stops. `join` puts each service's
# two under the commit's own tag, checks the result holds exactly those two,
# and prints each one's digest: the checksum the app pins.
#
# It reads GITHUB_REPOSITORY and GITHUB_SHA, which GitHub sets. REGISTRY is
# ghcr.io unless set, and is there to try the script against a registry on
# this computer. Nothing here signs in, and nothing is removed.
set -euo pipefail
cd "$(dirname "$0")/.."

SERVICES=(api worker)
ARCHES=(amd64 arm64)

fail() {
    echo "publish-images: $*" >&2
    exit 1
}

registry="${REGISTRY:-ghcr.io}"
[[ "${registry}" =~ ^[a-z0-9.-]+(:[0-9]+)?$ ]] ||
    fail "REGISTRY is not a plain host name: ${registry}"
repository="${GITHUB_REPOSITORY:-}"
[[ "${repository}" =~ ^[A-Za-z0-9-]+/[A-Za-z0-9_-][A-Za-z0-9._-]*$ ]] ||
    fail "GITHUB_REPOSITORY is not owner/name: ${repository}"
commit="${GITHUB_SHA:-}"
[[ "${commit}" =~ ^[0-9a-f]{40}$ ]] ||
    fail "GITHUB_SHA is not a whole commit: ${commit}"

# An image's name must be lower case. The owner's name on GitHub need not be.
prefix="${registry}/$(printf '%s' "${repository}" | tr '[:upper:]' '[:lower:]')"
source_url="https://github.com/${repository}"
tag="sha-${commit}"

# The kind of computer this is, in the names Docker uses.
this_arch() {
    case "$(uname -m)" in
        x86_64 | amd64) echo amd64 ;;
        aarch64 | arm64) echo arm64 ;;
        *) echo "unknown ($(uname -m))" ;;
    esac
}

build() {
    local arch="$1" service image
    case "${arch}" in
        amd64 | arm64) ;;
        *) fail "build takes amd64 or arm64, not: ${arch}" ;;
    esac
    [ "$(this_arch)" = "${arch}" ] ||
        fail "this computer is $(this_arch), so it does not build ${arch}: nothing is emulated"
    for service in "${SERVICES[@]}"; do
        image="${prefix}/${service}:${tag}-${arch}"
        docker build \
            --file infra/docker/service.Dockerfile \
            --build-arg "SERVICE=${service}" \
            --platform "linux/${arch}" \
            --provenance=false \
            --sbom=false \
            --label "org.opencontainers.image.source=${source_url}" \
            --label "org.opencontainers.image.revision=${commit}" \
            --label "org.opencontainers.image.licenses=MIT" \
            --label "org.opencontainers.image.title=Rusty Business™ ${service}" \
            --tag "${image}" \
            .
        docker push "${image}"
    done
}

join() {
    local service name digest platforms pinned
    if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
        printf '### The images to pin\n\n' >>"${GITHUB_STEP_SUMMARY}"
    fi
    for service in "${SERVICES[@]}"; do
        name="${prefix}/${service}"
        docker buildx imagetools create \
            --tag "${name}:${tag}" \
            --annotation "index:org.opencontainers.image.source=${source_url}" \
            --annotation "index:org.opencontainers.image.revision=${commit}" \
            --annotation "index:org.opencontainers.image.licenses=MIT" \
            --annotation "index:org.opencontainers.image.description=The Rusty Business™ ${service}, for the desktop app's Docker stack" \
            "${name}:${tag}-${ARCHES[0]}" "${name}:${tag}-${ARCHES[1]}"
        digest="$(docker buildx imagetools inspect "${name}:${tag}" --format '{{.Manifest.Digest}}')"
        [[ "${digest}" =~ ^sha256:[0-9a-f]{64}$ ]] ||
            fail "the registry gave ${name}:${tag} no plain digest: ${digest}"
        pinned="${name}@${digest}"
        platforms="$(docker buildx imagetools inspect "${pinned}" \
            --format '{{range .Manifest.Manifests}}{{.Platform.OS}}/{{.Platform.Architecture}}{{"\n"}}{{end}}' |
            sort | tr '\n' ' ')"
        [ "${platforms}" = "linux/${ARCHES[0]} linux/${ARCHES[1]} " ] ||
            fail "${pinned} should hold linux/${ARCHES[0]} and linux/${ARCHES[1]}, and holds: ${platforms}"
        echo "${service} ${pinned}"
        if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
            printf -- '- %s: `%s`\n' "${service}" "${pinned}" >>"${GITHUB_STEP_SUMMARY}"
        fi
    done
}

case "${1:-}" in
    build)
        [ "$#" -eq 2 ] || fail "use: scripts/publish-images.sh build amd64|arm64"
        build "$2"
        ;;
    join)
        [ "$#" -eq 1 ] || fail "use: scripts/publish-images.sh join"
        join
        ;;
    *) fail "use: scripts/publish-images.sh build amd64|arm64, or join" ;;
esac
