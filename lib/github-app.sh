# shellcheck shell=bash
# The factory's GitHub App: an installation token, made from the app's key and
# kept fresh in one file (docs/specs/2026-10-24-github-app.md). Sourced after
# lib/config.sh has loaded the instance.
#
#   tehas_github_app        0 when the instance names an app
#                           (TEHAS_GITHUB_APP_ID), 1 when it does not
#   tehas_github_app_key    the path of the app's private key:
#                           $TEHAS_GITHUB_APP_KEY when it is set (tehas setup
#                           checks a key before it installs it), else
#                           ~/.config/tehas/github-app.pem
#   tehas_github_app_slug   the app's slug, from GET /app with its JWT
#   tehas_github_app_mint   GitHub's whole answer for a new installation
#                           token: the token, its expiry and its permissions.
#                           Keeps nothing
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
#                           ends.
#   tehas_github_token      print a token that has at least 10 minutes left,
#                           making one when the kept one has less. 1 with one
#                           line on stderr when it cannot: no app, no key, the
#                           exchange failed or answered no token. A token is
#                           only ever printed to stdout, never logged.
#
# The token is $TEHAS_HOME/state/github/token (mode 600), its expiry
# token.expires (epoch seconds) beside it. The directory, not the file, is what
# a cycle's container is given read-only: a new token is moved over the old
# one, and a mount of the file would keep showing the old. One mint at a time:
# state/github/lock, waited for up to 30 seconds. GitHub is asked at TEHAS_GITHUB_API
# [https://api.github.com, from the environment; the tests point it away].

tehas_github_app() { [ -n "${TEHAS_GITHUB_APP_ID:-}" ]; }
tehas_github_app_key() { printf '%s\n' "${TEHAS_GITHUB_APP_KEY:-$HOME/.config/tehas/github-app.pem}"; }
tehas_github_dir() { printf '%s\n' "$TEHAS_HOME/state/github"; }

_tehas_b64url() { openssl base64 -A | tr '+/' '-_' | tr -d '='; }

# The app's JSON Web Token, valid for 9 minutes and back-dated one, as GitHub
# allows for a clock a little ahead of its own.
_tehas_github_jwt() {
  local now head body sig
  now=$EPOCHSECONDS
  head=$(printf '{"alg":"RS256","typ":"JWT"}' | _tehas_b64url) || return 1
  body=$(printf '{"iat":%d,"exp":%d,"iss":"%s"}' $((now - 60)) $((now + 540)) "$TEHAS_GITHUB_APP_ID" | _tehas_b64url) || return 1
  sig=$(printf '%s.%s' "$head" "$body" | openssl dgst -sha256 -sign "$(tehas_github_app_key)" | _tehas_b64url) || return 1
  [ -n "$sig" ] || return 1
  printf '%s.%s.%s\n' "$head" "$body" "$sig"
}

# Print the kept token when it has at least $1 seconds left.
_tehas_github_kept() {
  local file until token
  file=$(tehas_github_dir)/token
  until=$(cat "$file.expires" 2>/dev/null) || return 1
  [[ $until =~ ^[0-9]{1,12}$ ]] || return 1
  [ "$until" -ge $((EPOCHSECONDS + $1)) ] || return 1
  token=$(cat "$file" 2>/dev/null) || return 1
  [ -n "$token" ] || return 1
  printf '%s\n' "$token"
}

tehas_github_app_mint() {
  local api=${TEHAS_GITHUB_API:-https://api.github.com} key jwt
  key=$(tehas_github_app_key)
  [ -r "$key" ] || { echo "tehas: the GitHub App's key is not readable at $key" >&2; return 1; }
  jwt=$(_tehas_github_jwt) || { echo "tehas: the GitHub App's token could not be signed with $key" >&2; return 1; }
  curl -fsS --max-time 30 -X POST \
    -H "Authorization: Bearer $jwt" -H 'Accept: application/vnd.github+json' \
    "$api/app/installations/$TEHAS_GITHUB_APP_INSTALLATION/access_tokens" 2>/dev/null ||
    { echo "tehas: GitHub did not give the app a token for installation $TEHAS_GITHUB_APP_INSTALLATION" >&2; return 1; }
}

tehas_github_app_slug() {
  local api=${TEHAS_GITHUB_API:-https://api.github.com} key jwt answer slug
  key=$(tehas_github_app_key)
  [ -r "$key" ] || { echo "tehas: the GitHub App's key is not readable at $key" >&2; return 1; }
  jwt=$(_tehas_github_jwt) || { echo "tehas: the GitHub App's token could not be signed with $key" >&2; return 1; }
  answer=$(curl -fsS --max-time 30 \
    -H "Authorization: Bearer $jwt" -H 'Accept: application/vnd.github+json' "$api/app" 2>/dev/null) ||
    { echo "tehas: GitHub did not answer GET /app for app $TEHAS_GITHUB_APP_ID" >&2; return 1; }
  slug=$(jq -r '.slug // empty | strings' <<<"$answer" 2>/dev/null)
  [[ $slug =~ ^[A-Za-z0-9][A-Za-z0-9-]*$ ]] || { echo "tehas: GitHub's answer for app $TEHAS_GITHUB_APP_ID held no slug" >&2; return 1; }
  printf '%s\n' "$slug"
}

tehas_github_token() {
  local state exchange token expires until
  tehas_github_app || { echo "tehas: no GitHub App is set for this instance (TEHAS_GITHUB_APP_ID)" >&2; return 1; }
  _tehas_github_kept 600 && return 0
  state=$(tehas_github_dir)
  [ -r "$(tehas_github_app_key)" ] || { echo "tehas: the GitHub App's key is not readable at $(tehas_github_app_key)" >&2; return 1; }
  mkdir -p "$state" || return 1
  (
    # Another command may have made one while this one waited.
    flock -w 30 9 || { echo "tehas: another command is making the GitHub token" >&2; exit 1; }
    _tehas_github_kept 600 && exit 0
    exchange=$(tehas_github_app_mint) || exit 1
    token=$(jq -r '.token // empty | strings' <<<"$exchange" 2>/dev/null)
    expires=$(jq -r '.expires_at // empty | strings' <<<"$exchange" 2>/dev/null)
    # date reads an empty string as today's midnight, so an expiry that is not
    # there, or not ahead, is no expiry.
    until=$([ -n "$expires" ] && date -u -d "$expires" +%s 2>/dev/null)
    [ -n "$token" ] && [[ $until =~ ^[0-9]{1,12}$ ]] && [ "$until" -gt "$EPOCHSECONDS" ] ||
      { echo "tehas: GitHub's answer for the app's token held no token and expiry" >&2; exit 1; }
    # Written beside the old one and moved over it: a reader sees the old token
    # or the new one, never half of one.
    if ! (umask 077 && printf '%s\n' "$token" >"$state/token.new" && printf '%s\n' "$until" >"$state/token.expires.new") ||
      ! mv -f "$state/token.new" "$state/token" ||
      ! mv -f "$state/token.expires.new" "$state/token.expires"; then
      rm -f "$state/token.new" "$state/token.expires.new"
      echo "tehas: the app's token could not be kept in $state" >&2
      exit 1
    fi
    printf '%s\n' "$token"
  ) 9>>"$state/lock"
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

tehas_github_renew() {
  tehas_github_app || return 0
  (
    trap 'exit 0' TERM
    while sleep "${TEHAS_GITHUB_RENEW_EVERY:-300}" & wait $!; do
      tehas_github_token >/dev/null || true
    done
  ) </dev/null >/dev/null 2>&1 &
  printf '%s\n' "$!"
}
