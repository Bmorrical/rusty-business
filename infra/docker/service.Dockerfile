# syntax=docker/dockerfile:1

# One recipe for every server in services/. The build context is the
# repository root, because a service is built together with packages/shared:
#
#   docker build --file infra/docker/service.Dockerfile --build-arg SERVICE=api .
#
# SERVICE is the folder name under services/ (api, worker).

FROM rust:1-slim-trixie AS build
ARG SERVICE
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY packages packages
COPY services services
# The two caches hold downloaded crates and compiled output between builds.
# They never reach the image; only the finished program is copied out.
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/build/target,sharing=locked \
    test -n "${SERVICE}" \
    && cargo build --release --locked --package "rusty-${SERVICE}" \
    && cp "target/release/rusty-${SERVICE}" /service

FROM debian:trixie-slim
# The program runs as a user that owns nothing and cannot log in.
RUN useradd --system --uid 10001 --user-group --no-create-home \
        --shell /usr/sbin/nologin service
COPY --from=build /service /usr/local/bin/service
USER 10001:10001
ENTRYPOINT ["/usr/local/bin/service"]
