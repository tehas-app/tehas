# shellcheck shell=bash
# The factory's GitHub App, as a shell acts as it. The token itself is made and
# kept by console/src/github.rs (docs/specs/2026-10-24-github-app.md). Sourced
# with lib/config.sh, used after tehas_load.
#
#   tehas_github_app        0 when the instance names an app
#                           (TEHAS_GITHUB_APP_ID), 1 when it does not
#   tehas_github_app_env    with an app: drop GH_TOKEN, put $TEHAS_HOME/shim first
#                           on PATH and give git a github.com credential helper
#                           that asks the shim, as environment-level config, so
#                           every gh and git push acts as the app. Writes no file.
#                           host/env.sh calls it.
#   tehas_github_env        with an app: export GH_TOKEN, a token with at least
#                           10 minutes left, for a command that hands it on to a
#                           container. 1 when none could be had. Without an app
#                           it changes nothing.
#   tehas_github_renew      with an app: keep the kept token fresh in the
#                           background, asking every 5 minutes, while a
#                           container reads it; prints the renewer's pid, or
#                           nothing without an app. Kill it when the container
#                           ends. The renewing is `tehas-cycle renew`.

tehas_github_app() { [ -n "${TEHAS_GITHUB_APP_ID:-}" ]; }

# The helper key is the one tehas_git_credential_helper_key in lib/sandbox.sh
# prints; test/github_app_test.sh pins the two equal. The empty value first
# drops a helper git's files name, as the container's environment does.
tehas_github_app_env() {
  tehas_github_app || return 0
  local key=credential.https://github.com.helper
  unset GH_TOKEN
  case ":${PATH:-}:" in *":$TEHAS_HOME/shim:"*) ;; *) export PATH="$TEHAS_HOME/shim:${PATH:-}" ;; esac
  export GIT_CONFIG_COUNT=2 GIT_CONFIG_KEY_0=$key GIT_CONFIG_VALUE_0='' \
    GIT_CONFIG_KEY_1=$key GIT_CONFIG_VALUE_1="!$TEHAS_HOME/shim/gh auth git-credential"
}

tehas_github_env() {
  tehas_github_app || return 0
  GH_TOKEN=$("${TEHAS_CONSOLE_CMD:-${BASH_SOURCE[0]%/*}/../libexec/console}" tehasbot github-token) || return 1
  export GH_TOKEN
}

tehas_github_renew() {
  tehas_github_app || return 0
  "$TEHAS_HOME/libexec/tehas-cycle" renew --cmd "$TEHAS_HOME/libexec/github-token" \
    --every "${TEHAS_GITHUB_RENEW_EVERY:-300}" --retry 30 </dev/null >/dev/null &
  printf '%s\n' "$!"
}
