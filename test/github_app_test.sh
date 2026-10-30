#!/usr/bin/env bash
# Tests for lib/github-app.sh and libexec/github-token: the GitHub App's
# installation token, made from a signed JWT and kept until 10 minutes are
# left. openssl and curl are stubs: no key is read, no network is asked.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

token_cmd="$T_ROOT/libexec/github-token"
lib=". '$T_ROOT/lib/config.sh' && . '$T_ROOT/lib/github-app.sh' && tehas_load"
kept=$T/home/tehas/state/github/token
key=$T/home/.config/tehas/github-app.pem

# openssl: base64 reads stdin and writes its base64; dgst signs with a fixed
# signature, and records the key it was given.
t_stub openssl 'case "$1" in
base64) base64 -w0 ;;
dgst) cat >/dev/null; [ -z "${SIGN_FAILS:-}" ] || exit 1; printf "signature" ;;
esac'
# curl: the token exchange answers a token that runs out in an hour, unless
# CURL_FAILS or CURL_ANSWER says otherwise; the repository list names
# acme/widget. The answer of a case that wants to tell two of them apart is a
# word with a counter of its own on the end, which no other case sees.
t_stub curl 'for a in "$@"; do case "$a" in *Authorization:*) printf "%s\n" "$a" >>"$T/calls/auth" ;; esac; done
[ -z "${CURL_FAILS:-}" ] || exit 22
case "$*" in
*access_tokens*) if [ -n "${CURL_ANSWER:-}" ]; then printf "%s" "$CURL_ANSWER"; else
  n=$(($(cat "$T/mints" 2>/dev/null || echo 0) + 1)); echo "$n" >"$T/mints"
  printf "{\"token\":\"ghs_fresh%s\",\"expires_at\":\"%s\"}" "$n" "$(date -u -d "+1 hour" +%FT%TZ)"; fi ;;
*installation/repositories*) printf "{\"repositories\":[{\"full_name\":\"acme/widget\"}]}" ;;
*/app) printf "%s" "${APP_ANSWER:-{\"slug\":\"tehas-app-bot\"\}}" ;;
esac'

app=(TEHAS_REPO=acme/widget TEHAS_GITHUB_APP_ID=5452597 TEHAS_GITHUB_APP_INSTALLATION=174742089 "TEHAS_BOT_LOGIN=tehas-app-bot[bot]")
reset() {
  rm -rf "$T"/calls/* "$T/home/tehas/state" "$T/home/.config" "$T/mints"
  mkdir -p "$T/home/.config/tehas"
  printf 'a key\n' >"$key"
  chmod 600 "$key"
  T_ENV=()
  t_instance "${app[@]}"
}
keep() { # <seconds left> [a word for the token]: a kept token that runs out then
  local word=${2:-ghs_kept}
  mkdir -p "$T/home/tehas/state/github"
  printf '%s\n' "$word" >"$kept"
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

t_case "TEHAS_GITHUB_APP_KEY names the key and is kept from the environment; else the installed one"
reset
t_run "$lib && tehas_github_app_key"
t_eq "$t_out" "$key"
T_ENV=(TEHAS_GITHUB_APP_KEY=/elsewhere/app.pem)
t_run "$lib && tehas_github_app_key"
T_ENV=()
t_eq "$t_out" "/elsewhere/app.pem"

t_case "the slug is read from GET /app with the JWT; an answer with none is 1"
reset
t_run "$lib && tehas_github_app_slug"
t_eq "$t_rc" 0
t_eq "$t_out" "tehas-app-bot"
t_contains "$(t_calls curl)" "https://api.github.com/app"
t_contains "$(t_calls auth)" "Authorization: Bearer "
T_ENV=("APP_ANSWER={}")
t_run "$lib && tehas_github_app_slug"
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "held no slug"

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

# An installed home for the shim and env.sh: the real commands at their
# installed paths, gh a stub that says the token it was given.
installed() {
  local h=$T/home/tehas
  mkdir -p "$h/lib" "$h/libexec" "$h/bin" "$h/shim"
  cp "$T_ROOT/lib/config.sh" "$T_ROOT/lib/github-app.sh" "$T_ROOT/lib/paths.sh" "$h/lib/"
  cp "$T_ROOT/libexec/github-token" "$h/libexec/"
  cp "$T_ROOT/host/shim/gh" "$h/shim/gh"
  cp "$T_ROOT/host/env.sh" "$h/env.sh"
  printf '#!/usr/bin/env bash\necho "gh $* token=${GH_TOKEN:-none}"\n' >"$h/bin/gh"
  chmod +x "$h/bin/gh" "$h/shim/gh" "$h/libexec/github-token"
}

t_case "tehas_github_app_env drops GH_TOKEN, puts the shim first and gives git the shim's helper"
reset
t_run "GH_TOKEN=ghp_user; $lib && tehas_github_app_env && tehas_github_app_env
  echo \"token=\${GH_TOKEN:-none}\"; echo \"\$PATH\" | tr : '\n' | grep -c -x \"\$TEHAS_HOME/shim\"; echo \"\${PATH%%:*}\"
  echo \"\$GIT_CONFIG_COUNT|\$GIT_CONFIG_KEY_0|\$GIT_CONFIG_VALUE_0|\$GIT_CONFIG_KEY_1|\$GIT_CONFIG_VALUE_1\""
t_eq "$t_rc" 0
t_eq "$t_out" "token=none
1
$T/home/tehas/shim
2|credential.https://github.com.helper||credential.https://github.com.helper|!$T/home/tehas/shim/gh auth git-credential"

t_case "the helper key is the one lib/sandbox.sh names for a container"
t_run ". '$T_ROOT/lib/sandbox.sh' && tehas_git_credential_helper_key"
t_eq "$t_out" "credential.https://github.com.helper"

t_case "without an app, tehas_github_app_env and tehas_github_env change nothing"
reset
t_instance TEHAS_REPO=acme/widget
t_run "GH_TOKEN=ghp_user; before=\$PATH; $lib && tehas_github_app_env && tehas_github_env && echo \"\$GH_TOKEN \${GIT_CONFIG_COUNT:-unset} \$([ \"\$PATH\" = \"\$before\" ] && echo same)\""
t_eq "$t_out" "ghp_user unset same"

t_case "tehas_github_env exports the app's token, and fails when there is none"
reset
t_run "$lib && tehas_github_env && echo \"\$GH_TOKEN\""
t_contains "$t_out" "ghs_fresh"
reset
T_ENV=(CURL_FAILS=1)
t_run "$lib && tehas_github_env || echo failed"
t_contains "$t_out" "failed"

t_case "the shim gives gh the app's token, and each call asks for one"
reset
installed
t_run "TEHAS_GITHUB_APP_ID=5452597 GH_TOKEN=ghp_user $T/home/tehas/shim/gh pr list --repo acme/widget"
t_eq "$t_rc" 0
t_contains "$t_out" "gh pr list --repo acme/widget token=ghs_fresh"
t_lacks "$t_out" "ghp_user"

t_case "the shim runs no gh when no token could be had"
reset
installed
T_ENV=(CURL_FAILS=1)
t_run "TEHAS_GITHUB_APP_ID=5452597 $T/home/tehas/shim/gh pr list"
t_eq "$t_rc" 1
t_lacks "$t_out" "gh pr list"

t_case "without an app the shim is gh with the token it was given"
reset
installed
t_run "GH_TOKEN=ghp_user $T/home/tehas/shim/gh api user"
t_eq "$t_out" "gh api user token=ghp_user"

# The poll's token (#938): the dispatcher takes one at the start of a poll and
# gives it to every gh child as TEHAS_GH_TOKEN, so the shim asks for none. What
# is under test is whether the token program ran, and the kept token of the
# case is `ghs_poll` while a token the program makes is `ghs_fresh<n>` - the
# words gh was given say which path the shim took, and the stub curl's calls
# say how often it asked. An exchange is counted by its own path: the stub
# records every call it makes, including the one that names the installation.
case_no_token() { # <app given: 1 for the host's, 0 for none>
  reset
  installed
  [ "$1" = 0 ] && t_instance TEHAS_REPO=acme/widget
  # The host's own state as tehas_load leaves it for the commands it runs: the
  # app's three names exported, no GH_TOKEN of a user left in the environment,
  # and a token kept with half an hour left. The kept one is ghs_poll and a
  # token the program makes is ghs_fresh<n>, so gh's own words say which path
  # the shim took, and the stub curl's calls say how often it asked. An
  # exchange is counted by its own path: the stub records every call it makes,
  # including the one that names the installation.
  T_ENV=(TEHAS_GITHUB_APP_ID=5452597 TEHAS_GITHUB_APP_INSTALLATION=174742089
    "TEHAS_BOT_LOGIN=tehas-app-bot[bot]")
  keep 3000 ghs_poll
}

t_case "given TEHAS_GH_TOKEN the shim exports it as GH_TOKEN and asks for no token"
case_no_token 1
t_run "TEHAS_GH_TOKEN=ghs_123456789 \"$T/home/tehas/shim/gh\" pr list --repo acme/widget"
t_eq "$t_rc" 0
t_contains "$t_out" "gh pr list --repo acme/widget token=ghs_123456789"
t_lacks "$t_out" "token=ghs_fresh"
t_eq "$(t_calls curl)" ""

t_case "without TEHAS_GH_TOKEN the shim still asks for a token at every call"
case_no_token 1
# TEHAS_GH_TOKEN is unset, as it is for a poll that took no token: a name the
# caller's own environment left would be read here as the poll's. GH_TOKEN is
# unset too, as tehas_github_app_env leaves it with an app named. Neither kept
# token is left standing between the two calls, so each of them makes its own.
t_run "unset TEHAS_GH_TOKEN GH_TOKEN
rm -f '$T/home/tehas/state/github/token' '$T/home/tehas/state/github/token.expires'
\"$T/home/tehas/shim/gh\" pr list --repo acme/widget
rm -f '$T/home/tehas/state/github/token' '$T/home/tehas/state/github/token.expires'
\"$T/home/tehas/shim/gh\" pr list --repo acme/widget"
t_eq "$t_rc" 0
t_eq "$t_out" "gh pr list --repo acme/widget token=ghs_fresh1
gh pr list --repo acme/widget token=ghs_fresh2"
# Two calls of the shim, and the token program made a token for each of them.
t_eq "$(t_calls curl | grep -c access_tokens)" 2

t_case "an empty TEHAS_GH_TOKEN asks for a token as the name being unset does"
case_no_token 1
t_run "unset GH_TOKEN
rm -f '$T/home/tehas/state/github/token' '$T/home/tehas/state/github/token.expires'
TEHAS_GH_TOKEN= \"$T/home/tehas/shim/gh\" pr list"
t_eq "$t_rc" 0
t_contains "$t_out" "gh pr list token=ghs_fresh1"
t_eq "$(t_calls curl | grep -c access_tokens)" 1

t_case "with no app named, TEHAS_GH_TOKEN still reaches gh as its own token"
case_no_token 0
t_run "TEHAS_GH_TOKEN=ghs_123456789 \"$T/home/tehas/shim/gh\" api user"
t_eq "$t_rc" 0
t_eq "$t_out" "gh api user token=ghs_123456789"
t_eq "$(t_calls curl)" ""

t_case "env.sh with an app: the shim is first, git asks it, and the user token is gone"
reset
installed
mkdir -p "$T/home/.config/tehas"
printf 'GH_TOKEN=ghp_user\n' >"$T/home/.config/tehas/env"
t_run ". $T/home/tehas/env.sh; command -v gh; echo \"token=\${GH_TOKEN:-none}\"; git config --get-all credential.https://github.com.helper | tail -n 1"
t_eq "$t_out" "$T/home/tehas/shim/gh
token=none
!$T/home/tehas/shim/gh auth git-credential"

t_case "env.sh without an app leaves gh and the user token as they were"
reset
installed
t_instance TEHAS_REPO=acme/widget
printf 'GH_TOKEN=ghp_user\n' >"$T/home/.config/tehas/env"
t_run ". $T/home/tehas/env.sh; command -v gh; echo \"token=\${GH_TOKEN:-none}\""
t_eq "$t_out" "$T/home/tehas/bin/gh
token=ghp_user"

t_case "the renewer is tehas-cycle renew, starts at once, and stops when killed"
reset
mkdir -p "$T/home/tehas/libexec"
cat >"$T/home/tehas/libexec/tehas-cycle" <<STUB
#!/bin/bash
printf '%s\n' "\$*" >"$T/renew.args"
exec sleep 20
STUB
chmod 755 "$T/home/tehas/libexec/tehas-cycle"
start=$SECONDS
t_run "$lib && pid=\$(tehas_github_renew 2>/dev/null); echo \"\$pid\""
t_eq "$(((SECONDS - start) < 10))" "1"
pid=$t_out
for _ in $(seq 1 50); do [ -s "$T/renew.args" ] && break; sleep 0.1; done
t_eq "$(cat "$T/renew.args")" "renew --cmd $T/home/tehas/libexec/github-token --every 300 --retry 30"
kill "$pid" 2>/dev/null
sleep 0.3
t_eq "$(kill -0 "$pid" 2>/dev/null && echo alive || echo stopped)" "stopped"

t_case "without an app the renewer starts nothing"
reset
t_instance TEHAS_REPO=acme/widget
t_run "$lib && pid=\$(tehas_github_renew); echo \"[\$pid]\""
t_eq "$t_out" "[]"

t_done
