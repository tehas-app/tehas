#!/usr/bin/env bash
# Tests for lib/github-app.sh and libexec/github-token: the GitHub App's
# installation token, made from a signed JWT and kept until 10 minutes are
# left. openssl and curl are stubs: no key is read, no network is asked.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

token_cmd="$T_ROOT/libexec/github-token"
lib=". '$T_ROOT/lib/config.sh' && . '$T_ROOT/lib/github-app.sh' && tehas_load"
kept=$T/home/tehas/state/github-token
key=$T/home/.config/tehas/github-app.pem

# openssl: base64 reads stdin and writes its base64; dgst signs with a fixed
# signature, and records the key it was given.
t_stub openssl 'case "$1" in
base64) base64 -w0 ;;
dgst) cat >/dev/null; [ -z "${SIGN_FAILS:-}" ] || exit 1; printf "signature" ;;
esac'
# curl: the token exchange answers a token that runs out in an hour, unless
# CURL_FAILS or CURL_ANSWER says otherwise; the repository list names
# acme/widget.
t_stub curl 'for a in "$@"; do case "$a" in *Authorization:*) printf "%s\n" "$a" >>"$T/calls/auth" ;; esac; done
[ -z "${CURL_FAILS:-}" ] || exit 22
case "$*" in
*access_tokens*) if [ -n "${CURL_ANSWER:-}" ]; then printf "%s" "$CURL_ANSWER"; else printf "{\"token\":\"ghs_fresh%s\",\"expires_at\":\"%s\"}" "$(date +%s%N)" "$(date -u -d "+1 hour" +%FT%TZ)"; fi ;;
*installation/repositories*) printf "{\"repositories\":[{\"full_name\":\"acme/widget\"}]}" ;;
esac'

app=(TEHAS_REPO=acme/widget TEHAS_GITHUB_APP_ID=5452597 TEHAS_GITHUB_APP_INSTALLATION=174742089 "TEHAS_BOT_LOGIN=tehas-app-bot[bot]")
reset() {
  rm -rf "$T"/calls/* "$T/home/tehas/state" "$T/home/.config"
  mkdir -p "$T/home/.config/tehas"
  printf 'a key\n' >"$key"
  chmod 600 "$key"
  T_ENV=()
  t_instance "${app[@]}"
}
keep() { # <seconds left>: a kept token that runs out then
  mkdir -p "$T/home/tehas/state"
  printf 'ghs_kept\n' >"$kept"
  echo $(($(date +%s) + $1)) >"$kept.expires"
}

t_case "the three settings load together, and none at all is the token of before"
reset
t_run "$lib && echo \"\$TEHAS_GITHUB_APP_ID \$TEHAS_GITHUB_APP_INSTALLATION \$TEHAS_BOT_LOGIN\" && tehas_github_app && echo app"
t_eq "$t_rc" 0
t_eq "$t_out" "5452597 174742089 tehas-app-bot[bot]
app"
t_instance TEHAS_REPO=acme/widget
t_run "$lib && tehas_github_app || echo none"
t_eq "$t_out" "none"

t_case "one of the three without the others is refused, exit 78"
for one in "TEHAS_GITHUB_APP_ID=5452597" "TEHAS_GITHUB_APP_INSTALLATION=174742089" "TEHAS_BOT_LOGIN=tehas-app-bot[bot]"; do
  t_instance TEHAS_REPO=acme/widget "$one"
  t_run "$lib"
  t_eq "$t_rc" 78
  t_contains "$t_out" "are set together or not at all"
done

t_case "an ID that is not a number, or a login that is not one, is refused"
for bad in "TEHAS_GITHUB_APP_ID=app" "TEHAS_GITHUB_APP_ID=0" "TEHAS_GITHUB_APP_INSTALLATION=12x" "TEHAS_BOT_LOGIN=bot[bots]" "TEHAS_BOT_LOGIN=-x"; do
  t_instance "${app[@]}" "$bad"
  t_run "$lib"
  t_eq "$t_rc" 78
  t_contains "$t_out" "${bad%%=*} is not valid"
done

t_case "with no token kept, one is made, kept with mode 600, and printed"
reset
t_run "$token_cmd"
t_eq "$t_rc" 0
t_contains "$t_out" "ghs_fresh"
t_eq "$(cat "$kept")" "$t_out"
t_eq "$(stat -c %a "$kept")" 600
t_contains "$(t_calls curl)" "-X POST"
t_contains "$(t_calls curl)" "/app/installations/174742089/access_tokens"
t_eq "$(t_calls auth | wc -l)" 1

t_case "the JWT names the app and lasts at most ten minutes"
payload=$(t_calls auth | sed -n 's/.*Bearer [^.]*\.\([^.]*\)\..*/\1/p' | tr '_-' '/+')
while [ $((${#payload} % 4)) != 0 ]; do payload="$payload="; done
claims=$(base64 -d <<<"$payload")
t_eq "$(jq -r .iss <<<"$claims")" 5452597
t_eq "$(jq '.exp - .iat <= 600' <<<"$claims")" true
t_eq "$(jq ".iat <= $(date +%s)" <<<"$claims")" true

t_case "a kept token with more than 10 minutes left is printed and nothing is asked"
reset
keep 3000
t_run "$token_cmd"
t_eq "$t_rc" 0
t_eq "$t_out" "ghs_kept"
t_eq "$(t_calls curl)" ""

t_case "a kept token with less than 10 minutes left is made again"
reset
keep 300
t_run "$token_cmd"
t_eq "$t_rc" 0
t_contains "$t_out" "ghs_fresh"
t_eq "$(t_calls curl | grep -c access_tokens)" 1

t_case "an exchange that fails prints no token, keeps the old one and says why"
reset
keep 300
T_ENV=(CURL_FAILS=1)
t_run "$token_cmd"
t_eq "$t_rc" 1
t_lacks "$t_out" "ghs_"
t_contains "$t_out" "GitHub did not give the app a token for installation 174742089"
t_eq "$(cat "$kept")" "ghs_kept"

t_case "an answer with no token or no expiry is a failure, and nothing is kept"
for answer in '{"message":"Bad credentials"}' '{"token":"ghs_x"}' '{"token":"ghs_x","expires_at":"soon"}' 'not json'; do
  reset
  T_ENV=("CURL_ANSWER=$answer")
  t_run "$token_cmd"
  t_eq "$t_rc" 1
  t_contains "$t_out" "held no token and expiry"
  t_eq "$([ -e "$kept" ] && echo kept || echo none)" none
done

t_case "no readable key asks nothing of GitHub"
reset
rm -f "$key"
t_run "$token_cmd"
t_eq "$t_rc" 1
t_contains "$t_out" "the GitHub App's key is not readable at $key"
t_eq "$(t_calls curl)" ""

t_case "a key that cannot sign asks nothing of GitHub"
reset
T_ENV=(SIGN_FAILS=1)
t_run "$token_cmd"
t_eq "$t_rc" 1
t_contains "$t_out" "could not be signed"
t_eq "$(t_calls curl)" ""

t_case "an instance with no app is exit 78 and asks nothing"
reset
t_instance TEHAS_REPO=acme/widget
t_run "$token_cmd"
t_eq "$t_rc" 78
t_eq "$(t_calls curl)" ""

t_case "--check says the expiry and that the token reaches the repository, and never prints the token"
reset
t_run "$token_cmd --check"
t_eq "$t_rc" 0
t_contains "$t_out" "acting as tehas-app-bot[bot]"
t_contains "$t_out" "token good until 20"
t_contains "$t_out" "it reaches acme/widget"
t_lacks "$t_out" "ghs_"

t_case "--check of an installation that does not reach the repository is exit 1"
reset
t_instance TEHAS_REPO=acme/other TEHAS_GITHUB_APP_ID=5452597 TEHAS_GITHUB_APP_INSTALLATION=174742089 "TEHAS_BOT_LOGIN=tehas-app-bot[bot]"
t_run "$token_cmd --check"
t_eq "$t_rc" 1
t_contains "$t_out" "does not reach acme/other"

t_case "an argument it does not know is exit 64"
reset
t_run "$token_cmd --print"
t_eq "$t_rc" 64
t_run "$token_cmd --check extra"
t_eq "$t_rc" 64

t_case "two commands at once make one token"
reset
t_run "$token_cmd >$T/a & $token_cmd >$T/b & wait"
t_eq "$(t_calls curl | grep -c access_tokens)" 1
t_eq "$(cat "$T/a")" "$(cat "$T/b")"

t_done
