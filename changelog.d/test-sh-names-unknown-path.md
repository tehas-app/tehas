### Changes

- `scripts/test.sh` names the paths it was given when none of them holds a test file or a script to lint, so a runner probe such as the lean driver's can see which path it refused. The exit status stays 3.
