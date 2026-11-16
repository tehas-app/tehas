### Changes

- The gate workflow runs on pull requests into `main` and pushes to `main`
  only. A pull request onto a release train runs no gate; the train's pull
  request into `main` gates the batch.
- The release workflow builds and signs on a clean GitHub-hosted runner and
  installs its own Rust, jq and minisign at pinned versions.
- The release job signs only after a person approves its run: the signing key lives in the repository's `release` environment. `SECURITY.md` says how to report a vulnerability privately.
