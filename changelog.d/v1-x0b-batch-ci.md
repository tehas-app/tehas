### Changes

- The gate workflow runs on pull requests into `main` and pushes to `main`
  only. A pull request onto a release train runs no gate; the train's pull
  request into `main` gates the batch.
- The release workflow builds and signs on a clean GitHub-hosted runner and
  installs its own Rust, jq and minisign at pinned versions.
