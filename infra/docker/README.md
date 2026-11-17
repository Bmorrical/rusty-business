# infra/docker

How the services become Docker images.

| File                 | What it is                                              |
| -------------------- | ------------------------------------------------------- |
| `service.Dockerfile` | One recipe for every server in [services](../../services) |

`make up` builds both images. To build one by hand, from the repository's
root:

```bash
docker build --file infra/docker/service.Dockerfile --build-arg SERVICE=api --tag rusty-business/api:dev .
```

How the recipe works:

- **Two stages.** The first compiles with the Rust toolchain. The second is
  a small Debian image that receives only the finished program, at
  `/usr/local/bin/service`.
- **The whole workspace is the build context**, because a service is built
  together with [packages/shared](../../packages/shared). The root
  `.dockerignore` is an allow-list, so the build sees `Cargo.toml`,
  `Cargo.lock`, `packages`, and `services`, and nothing else.
- **`--locked`.** The build uses exactly the versions in `Cargo.lock` and
  fails if the lock file is out of date.
- **The program runs as user 10001**, which owns nothing and cannot log in.
- **No curl in the image.** Each program checks its own health:
  `service healthcheck`.
