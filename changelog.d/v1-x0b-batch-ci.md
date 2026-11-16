### Changes

- The gate workflow runs on pull requests into `main` and pushes to `main`
  only. A pull request onto a release train runs no CI; the train's pull
  request into `main` gates the batch.
