### Changes

- The sandbox base image is built from pinned inputs: ubuntu by digest, and
  node, gh, bun and rustup at the versions and checksums of `versions.env`
  rather than from install scripts. pi, ctx7 and parallel-cli come from the npm
  trees in `host/npm`, which `install.sh` now installs to `~/tehas/npm`. The
  first stage builds with `RUST_VERSION` instead of stable. A change to any of
  these pins asks for a sandbox build.

### Adopter action

The next `tehas update` rebuilds the sandbox image. The build passes the npm
trees as a named build context, so it needs Docker's BuildKit builder (buildx),
which the base build already needed.
