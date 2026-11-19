### Changes

- `install.sh` stops with exit 78 before it copies anything when the instance's `sandbox.Dockerfile` cannot be read. It used to copy the new programs first and then say nothing was installed, so `tehas update` put the old version back over half-installed programs.
- `install.sh` refuses an argument it does not take with exit 64 and a usage line. A mistyped `--check`, such as `--chek` or `--tests --check`, used to run a real install.
- The manual says what the store's dry run before the first copy does: it takes the write lock for a moment and needs room in `state/backup` for up to two copies of the database.
