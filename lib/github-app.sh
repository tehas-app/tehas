# shellcheck shell=bash
# The factory's GitHub App, as a login shell acts as it. The token itself is
# made and kept by console/src/github.rs (docs/specs/2026-10-24-github-app.md).
# host/env.sh sources it after tehas_load.
#
#   tehas_github_app        0 when the instance names an app
#                           (TEHAS_GITHUB_APP_ID), 1 when it does not
#   tehas_github_app_env    with an app: drop GH_TOKEN, put $TEHAS_HOME/shim first
#                           on PATH and give git a github.com credential helper
#                           that asks the shim, as environment-level config, so
#                           every gh and git push acts as the app. Writes no file.

tehas_github_app() { [ -n "${TEHAS_GITHUB_APP_ID:-}" ]; }

# The key is the one console/core/src/sandbox.rs gives a container's git. The
# empty value first drops a helper git's files name, as the container's
# environment does.
tehas_github_app_env() {
  tehas_github_app || return 0
  local key=credential.https://github.com.helper
  unset GH_TOKEN
  case ":${PATH:-}:" in *":$TEHAS_HOME/shim:"*) ;; *) export PATH="$TEHAS_HOME/shim:${PATH:-}" ;; esac
  export GIT_CONFIG_COUNT=2 GIT_CONFIG_KEY_0=$key GIT_CONFIG_VALUE_0='' \
    GIT_CONFIG_KEY_1=$key GIT_CONFIG_VALUE_1="!$TEHAS_HOME/shim/gh auth git-credential"
}
