# shellcheck shell=bash
# The factory's GitHub App: an installation token, made from the app's key and
# kept fresh in one file (docs/specs/2026-10-24-github-app.md). Sourced after
# lib/config.sh has loaded the instance.
#
#   tehas_github_app        0 when the instance names an app
#                           (TEHAS_GITHUB_APP_ID), 1 when it does not
#   tehas_github_app_key    the path of the app's private key
#   tehas_github_app_env    with an app: drop GH_TOKEN, put $TEHAS_HOME/shim first
#                           on PATH and give git a github.com credential helper
#                           that asks the shim, as environment-level config, so
#                           every gh and git push acts as the app. Writes no file.
#                           host/env.sh calls it.
#   tehas_github_env        with an app: export GH_TOKEN, a token with at least
#                           10 minutes left, for a command that hands it on to a
#                           container. 1 when none could be had. Without an app
#                           it changes nothing.
#   tehas_github_token      print a token that has at least 10 minutes left,
#                           making one when the kept one has less. 1 with one
#                           line on stderr when it cannot: no app, no key, the
#                           exchange failed or answered no token. A token is
#                           only ever printed to stdout, never logged.
#
# The token is $TEHAS_HOME/state/github-token (mode 600), its expiry
# github-token.expires (epoch seconds). One mint at a time: github-token.lock,
# waited for up to 30 seconds. GitHub is asked at TEHAS_GITHUB_API
# [https://api.github.com, from the environment; the tests point it away].

tehas_github_app() { [ -n "${TEHAS_GITHUB_APP_ID:-}" ]; }
tehas_github_app_key() { printf '%s\n' "$HOME/.config/tehas/github-app.pem"; }

_tehas_b64url() { openssl base64 -A | tr '+/' '-_' | tr -d '='; }

# The app's JSON Web Token, valid for 9 minutes and back-dated one, as GitHub
# allows for a clock a little ahead of its own.
_tehas_github_jwt() {
  local now head body sig
  now=$(date +%s)
  head=$(printf '{"alg":"RS256","typ":"JWT"}' | _tehas_b64url) || return 1
  body=$(printf '{"iat":%d,"exp":%d,"iss":"%s"}' $((now - 60)) $((now + 540)) "$TEHAS_GITHUB_APP_ID" | _tehas_b64url) || return 1
  sig=$(printf '%s.%s' "$head" "$body" | openssl dgst -sha256 -sign "$(tehas_github_app_key)" | _tehas_b64url) || return 1
  [ -n "$sig" ] || return 1
  printf '%s.%s.%s\n' "$head" "$body" "$sig"
}

# Print the kept token when it has at least $1 seconds left.
_tehas_github_kept() {
  local file=$TEHAS_HOME/state/github-token until token
  until=$(cat "$file.expires" 2>/dev/null) || return 1
  [[ $until =~ ^[0-9]{1,12}$ ]] || return 1
  [ "$until" -ge $(($(date +%s) + $1)) ] || return 1
  token=$(cat "$file" 2>/dev/null) || return 1
  [ -n "$token" ] || return 1
  printf '%s\n' "$token"
}

tehas_github_token() {
  local state=$TEHAS_HOME/state api=${TEHAS_GITHUB_API:-https://api.github.com} key jwt exchange token expires until
  tehas_github_app || { echo "tehas: no GitHub App is set for this instance (TEHAS_GITHUB_APP_ID)" >&2; return 1; }
  _tehas_github_kept 600 && return 0
  key=$(tehas_github_app_key)
  [ -r "$key" ] || { echo "tehas: the GitHub App's key is not readable at $key" >&2; return 1; }
  mkdir -p "$state" || return 1
  (
    # Another command may have made one while this one waited.
    flock -w 30 9 || { echo "tehas: another command is making the GitHub token" >&2; exit 1; }
    _tehas_github_kept 600 && exit 0
    jwt=$(_tehas_github_jwt) || { echo "tehas: the GitHub App's token could not be signed with $key" >&2; exit 1; }
    exchange=$(curl -fsS --max-time 30 -X POST \
      -H "Authorization: Bearer $jwt" -H 'Accept: application/vnd.github+json' \
      "$api/app/installations/$TEHAS_GITHUB_APP_INSTALLATION/access_tokens" 2>/dev/null) ||
      { echo "tehas: GitHub did not give the app a token for installation $TEHAS_GITHUB_APP_INSTALLATION" >&2; exit 1; }
    token=$(jq -r '.token // empty | strings' <<<"$exchange" 2>/dev/null)
    expires=$(jq -r '.expires_at // empty | strings' <<<"$exchange" 2>/dev/null)
    # date reads an empty string as today's midnight, so an expiry that is not
    # there, or not ahead, is no expiry.
    until=$([ -n "$expires" ] && date -u -d "$expires" +%s 2>/dev/null)
    [ -n "$token" ] && [[ $until =~ ^[0-9]{1,12}$ ]] && [ "$until" -gt "$(date +%s)" ] ||
      { echo "tehas: GitHub's answer for the app's token held no token and expiry" >&2; exit 1; }
    # Written beside the old one and moved over it: a reader sees the old token
    # or the new one, never half of one.
    if ! (umask 077 && printf '%s\n' "$token" >"$state/github-token.new" && printf '%s\n' "$until" >"$state/github-token.expires.new") ||
      ! mv -f "$state/github-token.new" "$state/github-token" ||
      ! mv -f "$state/github-token.expires.new" "$state/github-token.expires"; then
      rm -f "$state/github-token.new" "$state/github-token.expires.new"
      echo "tehas: the app's token could not be kept in $state" >&2
      exit 1
    fi
    printf '%s\n' "$token"
  ) 9>>"$state/github-token.lock"
}

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
  GH_TOKEN=$(tehas_github_token) || return 1
  export GH_TOKEN
}
