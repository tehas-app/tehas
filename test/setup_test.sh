#!/usr/bin/env bash
# Tests for libexec/setup, the first setup of an instance: what it asks
# GitHub and the model endpoint, the files it writes and their modes, that a
# second run changes nothing, that it never overwrites a setting without
# --force, and that no token, key or code is ever on a command line. The
# factory acts as a GitHub App, given (--app-id, --app-key-file) or made
# (--new-app). A scratch platform tree stands in for the install: its
# install.sh, the host's tehas and check-product record that they ran.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

plat=$T/plat
mkdir -p "$plat/libexec" "$plat/host"
cp "$T_ROOT/libexec/setup" "$plat/libexec/setup"
t_console || t_bad "no console: $t_console_why"
cp "$T_CONSOLE" "$plat/libexec/console"
cp -R "$T_ROOT/lib" "$plat/lib"
setup="$plat/libexec/setup"
cfg=$T/home/.config/tehas
build=$T/home/tehas/sandbox/build.sh

# The platform's own steps, each recording its run.
printf '#!/usr/bin/env bash\necho install.sh $* >>"$T/calls/steps"\n' >"$plat/install.sh"
mkdir -p "$T/home/tehas/bin"
printf '#!/usr/bin/env bash\necho $* >>"$T/calls/steps"\n' >"$T/home/tehas/bin/tehas"
printf '#!/usr/bin/env bash\necho $* >>"$T/calls/tehasbot"\n' >"$T/home/tehas/bin/tehasbot"
mkdir -p "$(dirname "$build")"
printf '#!/usr/bin/env bash\necho build $* >>"$T/calls/steps"\n' >"$build"
chmod +x "$plat/install.sh" "$T/home/tehas/bin/tehas" "$T/home/tehas/bin/tehasbot" "$build"

printf 'ghp_secret_token\n' >"$T/token"
printf 'sk_model_key\n' >"$T/model-key"
printf 'an app key\n' >"$T/app.pem"

# The shim the install puts first on PATH for the app: it records the call and
# hands gh a token of its own, as tehasbot github-token would.
mkdir -p "$T/home/tehas/shim"
printf '#!/usr/bin/env bash\necho "$*" >>"$T/calls/shim"\nGH_TOKEN=ghs_shim exec "$T/bin/gh" "$@"\n' >"$T/home/tehas/shim/gh"
chmod +x "$T/home/tehas/shim/gh"

# The key of git's credential helper for github.com, the one
# work_caps_test.sh pins on a container's git. A change to its spelling fails
# there or here, not on a host.
HK=credential.https://github.com.helper

# gh: the app's bot has the ID 9191, a new issue is number 7, a clone makes
# the checkout. The token it was given is kept.
stub_gh() {
  # The stubs are written with @T@ in place of the scratch directory: t_stub
  # echoes its body unquoted, so the path is substituted here, before it does.
  t_stub gh "$(printf '%s' 'case "$*" in
  "api users/"*"[bot] --jq .id") echo 9191 ;;
  "api repos/acme/widget/assignees/alice --silent") exit 0 ;;
  # The ids install.sh records of the allowed accounts, as t_stub gives them.
  "api users/"*" --jq .id") ;;
  "issue create --repo acme/widget"*) echo "https://github.com/acme/widget/issues/7" ;;
  "repo clone acme/widget "*) mkdir -p "${4}/.git" ;;
  *) exit 1 ;;
esac
[ -z "${GH_TOKEN:-}" ] || printf "%s\n" "$GH_TOKEN" >"@T@/gh-token"' | sed -e "s|@T@|$T|g")"
}

# curl: the endpoint's model list (MODELS, space separated). What it was given
# on stdin is kept: the model's key goes there and not on the line, and so do
# the app's JWT and the URL of a manifest's conversion. For the app: GET /app
# answers APP_ANSWER, by default the slug tehas-app; a minted token has the
# permissions PERMS, by default write on all three that setup needs; the app is
# installed on APP_REPOS (acme/widget acme/other), else GitHub answers 404;
# GET /users/<owner> says OWNER_TYPE (Organization); a conversion answers
# CONVERSION, or 404 with CONVERT_FAIL=1.
stub_curl() {
  t_stub curl 'in=$(cat)
case "$*" in
*access_tokens) printf "%s\n" "$in" >>"$T/calls/app-auth"; printf "{\"token\":\"ghs_minted\",\"expires_at\":\"2099-01-01T00:00:00Z\",\"permissions\":%s}\n" "${PERMS:-{\"contents\":\"write\",\"issues\":\"write\",\"pull_requests\":\"write\"\}}"; exit 0 ;;
*/app/installations) printf "%s\n" "$in" >>"$T/calls/app-auth"; printf "%s\n" "${INSTALLS:-[{\"id\":88\}]}"; exit 0 ;;
*/repos/*/installation)
  printf "%s\n" "$in" >>"$T/calls/app-auth"
  all="$*"; repo=${all##*/repos/}; repo=${repo%/installation}
  for r in ${APP_REPOS:-acme/widget acme/other}; do
    [ "$r" != "$repo" ] || { echo "{\"id\":88}"; exit 0; }
  done
  echo "curl: (22) The requested URL returned error: 404" >&2; exit 22 ;;
*/app) printf "%s\n" "$in" >>"$T/calls/app-auth"; printf "%s\n" "${APP_ANSWER:-{\"slug\":\"tehas-app\"\}}"; exit 0 ;;
esac
case $in in
*/app-manifests/*)
  printf "%s\n" "$in" >>"$T/calls/conversion"
  [ -z "${CONVERT_FAIL:-}" ] || { echo "curl: (22) The requested URL returned error: 404" >&2; exit 22; }
  printf "%s\n" "${CONVERSION:-{\"id\":9,\"slug\":\"widget-factory\",\"pem\":\"-----BEGIN RSA PRIVATE KEY-----\\nk\\n-----END RSA PRIVATE KEY-----\\n\",\"client_secret\":\"cs_secret\",\"webhook_secret\":\"wh_secret\"\}}"
  exit 0 ;;
*/users/*) printf "{\"type\":\"%s\"}\n" "${OWNER_TYPE:-Organization}"; exit 0 ;;
esac
printf "%s\n" "$in" >"$T/curl-stdin"
printf "{\"data\":["
sep=""
for m in ${MODELS:-m1}; do printf "%s{\"id\":\"%s\"}" "$sep" "$m"; sep=,; done
printf "]}\n"'
}
stub_all() {
  rm -rf "$T/calls" "$T/image-there" && mkdir -p "$T/calls"
  stub_gh
  stub_curl
  t_stub systemctl
  t_stub openssl 'case "$1" in dgst) cat >/dev/null; printf signature ;; esac'
  t_stub docker 'case "$1 $2" in "image inspect") [ -e "$T/image-there" ] ;; esac'
}
fresh() {
  rm -rf "$cfg" "$T/home/.gitconfig" "$T/home/tehas/repos" "$T/home/tehas/state"
  stub_all
}

args=(--repo acme/widget --labellers "alice bob" --app-id 77 --app-key-file "$T/app.pem" --endpoint http://gb1:8086/v1)
# Every run of setup reads the host's git config as the host has it: t_run
# starts from an empty environment and carries over only what T_ENV names, so
# git's environment-level config - the scope that outranks every file - is not
# there unless a case puts those entries in T_ENV itself. One entry per value,
# quoted, because t_run hands T_ENV to env as separate words.
run_setup() { t_run "$setup $(printf '%q ' "$@") </dev/null"; }
# run_setup_given <stdin line> [options]: what the owner pastes back.
run_setup_given() {
  local line=$1
  shift
  t_run "printf '%s\\n' $(printf '%q' "$line") | $setup $(printf '%q ' "$@")"
}
# The page in the one data link a run printed.
linked_page() { grep -o 'data:text/html;base64,[A-Za-z0-9+/=]*' <<<"$t_out" | head -n 1 | sed 's/^data:text\/html;base64,//' | base64 -d; }

t_case "a first run with an app writes the settings, the model list and the app's key, and starts the factory"
fresh
run_setup "${args[@]}"
t_eq "$t_rc" 0
t_eq "$(grep -v -e '^#' -e '^$' "$cfg/instance.env")" 'TEHAS_REPO=acme/widget
TEHAS_CONTROL_ISSUE=7
TEHAS_ALLOWED_LABELERS="alice bob"
PI_ENSEMBLE_SUBAGENT_PROVIDER=main
PI_ENSEMBLE_SUBAGENT_MODEL=m1
TEHAS_GITHUB_APP_ID=77'
t_eq "$(jq -cS . "$cfg/models.json")" '{"providers":{"main":{"api":"openai-completions","apiKey":"unused","baseUrl":"http://gb1:8086/v1","models":[{"id":"m1"}]}}}'
t_eq "$(cat "$cfg/github-app.pem")" "an app key"
t_eq "$(stat -c %a "$cfg/github-app.pem")" 600
t_eq "$(stat -c %a "$cfg/instance.env")" 600
t_eq "$([ -e "$cfg/env" ] && echo written)" ""
t_eq "$(stat -c %a "$cfg")" 700
t_eq "$(cat "$T/home/tehas/state/github/login")" "tehas-app[bot]"
t_eq "$(git config -f "$T/home/.gitconfig" user.name)" "tehas-app[bot]"
t_eq "$(git config -f "$T/home/.gitconfig" user.email)" "9191+tehas-app[bot]@users.noreply.github.com"
t_eq "$([ -d "$T/home/tehas/repos/widget/.git" ] && echo cloned)" cloned
t_eq "$(t_calls steps)" "install.sh
labels
build"
t_eq "$(t_calls tehasbot)" "github-token --check"
t_contains "$(t_calls systemctl)" "--user enable --now tehas-dispatch.timer tehas-dispatch.path"
t_contains "$t_out" "the app tehas-app is installed on acme/widget and may write to it"
t_contains "$t_out" "bin/tehas is not installed, so the product's gates were not run"
t_contains "$t_out" "http://127.0.0.1:8080/"
t_contains "$(t_calls curl)" "https://api.github.com/repos/acme/widget/installation"
t_contains "$(t_calls curl)" "https://api.github.com/app/installations/88/access_tokens"
t_contains "$(t_calls shim)" "repo clone acme/widget"
t_eq "$(cat "$T/gh-token")" "ghs_shim"
t_eq "$(t_calls app-auth | grep -c 'Authorization: Bearer ')" "$(t_calls app-auth | grep -c .)"

t_case "no token or key is on any command line, and the obsolete app settings are not written"
t_lacks "$(cat "$T"/calls/*)" "ghs_minted"
t_lacks "$(t_calls curl)" "Authorization"
t_lacks "$(cat "$cfg/instance.env")" "TEHAS_GITHUB_APP_INSTALLATION"
t_lacks "$(cat "$cfg/instance.env")" "TEHAS_BOT_LOGIN"

t_case "a second run changes nothing and makes no issue"
before=$(cd "$cfg" && sha256sum instance.env models.json github-app.pem; git config -f "$T/home/.gitconfig" -l)
stub_all
: >"$T/image-there"
run_setup "${args[@]}"
t_eq "$t_rc" 0
t_eq "$(cd "$cfg" && sha256sum instance.env models.json github-app.pem; git config -f "$T/home/.gitconfig" -l)" "$before"
t_lacks "$(t_calls gh)" "issue create"
t_lacks "$(t_calls steps)" "build"
t_contains "$t_out" "the sandbox image is there"

t_case "the settings and the app already there fill what the command line leaves out"
stub_all
: >"$T/image-there"
run_setup --endpoint http://gb1:8086/v1
t_eq "$t_rc" 0
t_eq "$(cd "$cfg" && sha256sum instance.env models.json github-app.pem; git config -f "$T/home/.gitconfig" -l)" "$before"

t_case "an owner GitHub assigns is taken; one it does not is 78 before the clone"
stub_all
: >"$T/image-there"
cp "$cfg/instance.env" "$T/instance.env.kept"
echo TEHAS_OWNER=alice >>"$cfg/instance.env"
run_setup --endpoint http://gb1:8086/v1
t_eq "$t_rc" 0
t_contains "$(t_calls shim)" "api repos/acme/widget/assignees/alice --silent"
stub_all
sed -i 's/^TEHAS_OWNER=alice$/TEHAS_OWNER=dave/' "$cfg/instance.env"
run_setup --endpoint http://gb1:8086/v1
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_OWNER is not valid: GitHub does not say dave can be assigned issues in acme/widget"
t_lacks "$(t_calls steps)" "install.sh"
mv "$T/instance.env.kept" "$cfg/instance.env"

t_case "a different setting is refused with 78 and nothing changes, until --force"
stub_all
run_setup --repo acme/widget --labellers "carol" --endpoint http://gb1:8086/v1
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_ALLOWED_LABELERS"
t_contains "$t_out" "--force"
t_eq "$(cd "$cfg" && sha256sum instance.env models.json github-app.pem; git config -f "$T/home/.gitconfig" -l)" "$before"
t_eq "$(t_calls gh)$(t_calls curl)" ""
run_setup --repo acme/widget --labellers "carol" --endpoint http://gb1:8086/v1 --force --no-build
t_eq "$t_rc" 0
t_eq "$(grep '^TEHAS_ALLOWED_LABELERS=' "$cfg/instance.env")" 'TEHAS_ALLOWED_LABELERS="carol"'
t_eq "$(grep -c '^TEHAS_ALLOWED_LABELERS=' "$cfg/instance.env")" 1

t_case "--token-file is refused with 78 in the settings' words, before any call"
stub_all
run_setup "${args[@]}" --token-file "$T/token"
t_eq "$t_rc" 78
t_contains "$t_out" "--token-file is no longer used: the factory acts as a GitHub App. Run tehas setup --new-app or give --app-id and --app-key-file."
t_eq "$(t_calls gh)$(t_calls curl)" ""
t_lacks "$(cat "$cfg/instance.env" "$cfg"/env 2>/dev/null)" "ghp_secret_token"

t_case "--app-installation is warned of and ignored"
fresh
run_setup "${args[@]}" --app-installation 88 --no-build
t_eq "$t_rc" 0
t_contains "$t_out" "--app-installation is no longer used"
t_lacks "$(cat "$cfg/instance.env")" "TEHAS_GITHUB_APP_INSTALLATION"

t_case "a missing setting with nothing to ask on is 78, before any call"
fresh
run_setup --repo acme/widget --app-id 77 --app-key-file "$T/app.pem" --endpoint http://gb1:8086/v1
t_eq "$t_rc" 78
t_contains "$t_out" "--labellers"
t_eq "$([ -e "$cfg/instance.env" ] && echo written)" ""
t_eq "$(t_calls gh)$(t_calls curl)" ""

t_case "a malformed repository or endpoint is 78, before any call"
fresh
run_setup --repo "acme widget" --labellers alice --app-id 77 --app-key-file "$T/app.pem" --endpoint http://gb1:8086/v1
t_eq "$t_rc" 78
t_contains "$t_out" "owner/name"
run_setup --repo acme/widget --labellers alice --app-id 77 --app-key-file "$T/app.pem" --endpoint gb1:8086
t_eq "$t_rc" 78
t_contains "$t_out" "http"
t_eq "$(t_calls gh)$(t_calls curl)" ""

t_case "no app at all is 78 naming both ways, before any call"
fresh
run_setup --repo acme/widget --labellers alice --endpoint http://gb1:8086/v1
t_eq "$t_rc" 78
t_contains "$t_out" "give --new-app to make one, or --app-id <id> --app-key-file <key>"
t_eq "$(t_calls gh)$(t_calls curl)" ""
t_eq "$([ -e "$cfg" ] && echo written)" ""

t_case "several models and no --model is 78 naming them; --model picks one; one not offered is 1"
fresh
T_ENV=(MODELS="m1 m2")
run_setup "${args[@]}"
t_eq "$t_rc" 78
t_contains "$t_out" "m1 m2"
t_eq "$([ -e "$cfg/instance.env" ] && echo written)" ""
run_setup "${args[@]}" --model m2 --no-build
t_eq "$t_rc" 0
t_eq "$(grep '^PI_ENSEMBLE_SUBAGENT_MODEL=' "$cfg/instance.env")" "PI_ENSEMBLE_SUBAGENT_MODEL=m2"
fresh
run_setup "${args[@]}" --model m3
t_eq "$t_rc" 1
t_contains "$t_out" "does not offer m3"
T_ENV=()

t_case "a model key goes in env, models.json names it, and curl gets it on stdin"
fresh
run_setup "${args[@]}" --model-key-file "$T/model-key" --no-build
t_eq "$t_rc" 0
t_eq "$(grep '^TEHAS_MODEL_KEY_MAIN=' "$cfg/env")" "TEHAS_MODEL_KEY_MAIN=sk_model_key"
t_eq "$(jq -r .providers.main.apiKey "$cfg/models.json")" '$TEHAS_MODEL_KEY_MAIN'
t_contains "$(cat "$T/curl-stdin")" "Authorization: Bearer sk_model_key"
t_lacks "$(cat "$T"/calls/*)" "sk_model_key"
t_lacks "$(cat "$cfg/models.json" "$cfg/instance.env")" "sk_model_key"

t_case "a git identity that is there is kept"
fresh
git config -f "$T/home/.gitconfig" user.name "Someone"
git config -f "$T/home/.gitconfig" user.email "someone@example.org"
run_setup "${args[@]}" --no-build
t_eq "$t_rc" 0
t_eq "$(git config -f "$T/home/.gitconfig" user.name)" "Someone"
t_contains "$t_out" "kept"

# The app's helper is environment-level config, which outranks git's files:
# nothing is written to them and gh is not asked to write one.
t_case "git asks the app's gh for credentials, and nothing is written to git's files"
fresh
run_setup "${args[@]}" --no-build
t_eq "$t_rc" 0
t_contains "$t_out" "git asks the app's gh for github.com credentials ($T/home/tehas/shim/gh auth git-credential)"
t_lacks "$(t_calls gh)" "auth setup-git"
t_eq "$(git config -f "$T/home/.gitconfig" --get-all "$HK")" ""

# A helper an operator wrote can hold a token in its arguments: it is not
# printed, and git's words about the file it read are not either.
t_case "a helper in git's files is not printed"
fresh
git config -f "$T/home/.gitconfig" "$HK" '!store --file ~/.token-cache'
run_setup "${args[@]}" --no-build
t_eq "$t_rc" 0
t_lacks "$t_out" "token-cache"
t_lacks "$t_out" "store"
t_lacks "$(t_calls gh)" "auth setup-git"

# A config git will not open is a settings problem, not a step failure: the
# run stops with 78, says which key it could not read, and neither asks gh for
# a helper nor clones, so nothing is written over a file git refused to read.
t_case "a git config git cannot open is exit 78 before the helper is set"
fresh
mkdir -p "$T/home/.gitconfig"
run_setup "${args[@]}" --no-build
t_eq "$t_rc" 78
t_contains "$t_out" "could not read git's credential helper for github.com"
t_lacks "$t_out" "Is a directory"
t_lacks "$(t_calls gh)" "auth setup-git"
t_lacks "$(t_calls gh)" "repo clone"

t_case "the product's gates run in the sandbox when check-product is there; --no-build skips both"
fresh
mkdir -p "$plat/bin"
printf '#!/usr/bin/env bash\necho $* >>"$T/calls/steps"\n' >"$plat/bin/tehas"
chmod +x "$plat/bin/tehas"
run_setup "${args[@]}"
t_eq "$t_rc" 0
t_contains "$(t_calls steps)" "check-product --sandbox"
fresh
run_setup "${args[@]}" --no-build
t_lacks "$(t_calls steps)" "build"
t_lacks "$(t_calls steps)" "check-product"
rm -f "$plat/bin/tehas"

# A checkout setup finds, as tehas state unpack leaves it: a real clone of a
# bare origin whose URL ends in the repository, one commit behind. The gates
# record the commit they were given.
g() { git -c user.name=t -c user.email=t@example.com "$@"; }
behind_origin() {
  rm -rf "$T/src" "$T/origin" && mkdir -p "$T/src" "$T/home/tehas/repos"
  g -C "$T/src" init -q --initial-branch=main && echo a >"$T/src/a"
  g -C "$T/src" add -A && g -C "$T/src" commit -qm one
  g clone -q --bare "$T/src" "$T/origin/acme/widget.git"
  g clone -q "$T/origin/acme/widget.git" "$T/home/tehas/repos/widget"
  echo b >"$T/src/a" && g -C "$T/src" commit -qam two
  g -C "$T/src" push -q "$T/origin/acme/widget.git" main
}
work=$T/home/tehas/repos/widget
mkdir -p "$plat/bin"
printf '#!/usr/bin/env bash\necho $* $(git -C "$T/home/tehas/repos/widget" rev-parse HEAD origin/HEAD) >>"$T/calls/steps"\n' >"$plat/bin/tehas"
chmod +x "$plat/bin/tehas"

t_case "a checkout behind origin is fast-forwarded before the gates, which run on the new head"
fresh
behind_origin
from=$(git -C "$work" rev-parse HEAD)
to=$(git -C "$T/src" rev-parse HEAD)
run_setup "${args[@]}"
t_eq "$t_rc" 0
t_eq "$(git -C "$work" rev-parse HEAD)" "$to"
t_contains "$t_out" "tehas setup: fast-forwarded the product checkout $work ${from:0:12} -> ${to:0:12}"
t_eq "$(grep check-product "$T/calls/steps")" "check-product --sandbox $to $to"
t_lacks "$(t_calls gh)" "repo clone"

t_case "a second setup on a current checkout changes nothing and makes no commit"
stub_all
: >"$T/image-there"
refs=$(git -C "$work" for-each-ref; git -C "$work" rev-list --count --all; git -C "$work" status --porcelain)
run_setup "${args[@]}"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas setup: the product checkout $work is up to date at ${to:0:12}"
t_eq "$(git -C "$work" for-each-ref; git -C "$work" rev-list --count --all; git -C "$work" status --porcelain)" "$refs"

t_case "a checkout with commits of its own is left as it is and named, and setup goes on with status 0"
fresh
behind_origin
g -C "$work" commit -q --allow-empty -m local
to=$(git -C "$T/src" rev-parse HEAD)
mine=$(git -C "$work" rev-parse HEAD)
run_setup "${args[@]}"
t_eq "$t_rc" 0
t_eq "$(git -C "$work" rev-parse HEAD)" "$mine"
t_contains "$t_out" "tehas setup: no fast-forward of the product checkout $work (it cannot fast-forward to origin/main at ${to:0:12}"
t_contains "$t_out" "the checkout was left as it is"
t_contains "$(t_calls steps)" "check-product --sandbox"

t_case "--no-build fast-forwards the checkout too, and runs no gates"
fresh
behind_origin
to=$(git -C "$T/src" rev-parse HEAD)
run_setup "${args[@]}" --no-build
t_eq "$t_rc" 0
t_eq "$(git -C "$work" rev-parse HEAD)" "$to"
t_lacks "$(t_calls steps)" "check-product"
rm -rf "$T/src" "$T/origin"
rm -f "$plat/bin/tehas"

t_case "a given control issue is used and none is made"
fresh
run_setup "${args[@]}" --control-issue 3 --no-build
t_eq "$t_rc" 0
t_eq "$(grep '^TEHAS_CONTROL_ISSUE=' "$cfg/instance.env")" "TEHAS_CONTROL_ISSUE=3"
t_lacks "$(t_calls gh)" "issue create"

app_args=("${args[@]}")

t_case "an answer for GET /app with no slug writes nothing, exit 1"
fresh
T_ENV=("APP_ANSWER={}")
run_setup "${app_args[@]}" --no-build
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "GitHub's answer for app 77 held no slug"
t_contains "$t_out" "nothing was written"
t_eq "$([ -e "$cfg/instance.env" ] && echo written)" ""

t_case "a first app run installs before it clones, because the app's gh is the install's"
fresh
mv "$T/home/tehas/shim/gh" "$T/shim-gh"
printf '#!/usr/bin/env bash\necho install.sh $* >>"$T/calls/steps"\nmkdir -p "$T/home/tehas/shim"\ncp "$T/shim-gh" "$T/home/tehas/shim/gh"\n' >"$plat/install.sh"
run_setup "${app_args[@]}" --no-build
t_eq "$t_rc" 0
t_eq "$(grep -c '^install.sh' "$T/calls/steps")" 2
t_contains "$(t_calls shim)" "repo clone acme/widget"
printf '#!/usr/bin/env bash\necho install.sh $* >>"$T/calls/steps"\n' >"$plat/install.sh"
chmod +x "$plat/install.sh"

t_case "another app key is refused with 78 until --force, which replaces the key and drops the old token"
printf 'another key\n' >"$T/app2.pem"
stub_all
mkdir -p "$T/home/tehas/state/github"
echo ghs_old >"$T/home/tehas/state/github/token"
run_setup "${app_args[@]}" --app-key-file "$T/app2.pem" --no-build
t_eq "$t_rc" 78
t_contains "$t_out" "github-app.pem"
t_eq "$(cat "$cfg/github-app.pem")" "an app key"
t_eq "$(t_calls gh)$(t_calls curl)" ""
run_setup "${app_args[@]}" --app-key-file "$T/app2.pem" --no-build --force
t_eq "$t_rc" 0
t_eq "$(cat "$cfg/github-app.pem")" "another key"
t_eq "$(stat -c %a "$cfg/github-app.pem")" 600
t_eq "$([ -e "$T/home/tehas/state/github/token" ] && echo kept)" ""

t_case "another app ID is refused with 78 until --force, which forgets what was kept for the old app"
stub_all
printf 'acme 11\n' >"$T/home/tehas/state/github/installation"
printf 'old-app[bot]\n' >"$T/home/tehas/state/github/login"
run_setup "${app_args[@]}" --app-id 78 --app-key-file "$T/app2.pem" --no-build
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_GITHUB_APP_ID"
run_setup "${app_args[@]}" --app-id 78 --app-key-file "$T/app2.pem" --no-build --force
t_eq "$t_rc" 0
t_eq "$(grep '^TEHAS_GITHUB_APP_ID=' "$cfg/instance.env")" "TEHAS_GITHUB_APP_ID=78"
t_eq "$(cat "$T/home/tehas/state/github/login")" "tehas-app[bot]"
t_eq "$([ -e "$T/home/tehas/state/github/installation" ] && echo kept)" ""

t_case "an app not installed on the repository stops with 1, names it and the link, and writes nothing"
fresh
T_ENV=(APP_REPOS=acme/other)
run_setup "${app_args[@]}" --no-build
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "the GitHub App is not installed on acme/widget; install it at https://github.com/apps/tehas-app/installations/new"
t_contains "$t_out" "nothing was written"
t_eq "$([ -e "$cfg" ] && echo written)" ""
t_eq "$([ -e "$T/home/tehas/state" ] && echo written)" ""
t_lacks "$(t_calls gh)" "issue create"

t_case "another repository the instance names that the app does not reach is named with the link"
fresh
mkdir -p "$cfg"
printf 'TEHAS_REVIEW_REPO=acme/reviews\nTEHAS_IMPROVE_REPO=acme/other\n' >"$cfg/instance.env"
run_setup "${app_args[@]}" --no-build
t_eq "$t_rc" 0
t_contains "$t_out" "the app does not reach acme/reviews (TEHAS_REVIEW_REPO: HTTP 404); install it there at https://github.com/apps/tehas-app/installations/new"
t_contains "$t_out" "the app reaches acme/other (TEHAS_IMPROVE_REPO)"

t_case "an installation without write on contents, issues or pull requests stops with 1 and writes nothing"
for p in contents issues pull_requests; do
  fresh
  T_ENV=("PERMS=$(jq -cn --arg p "$p" '{contents: "write", issues: "write", pull_requests: "write"} | .[$p] = "read"')")
  run_setup "${app_args[@]}" --no-build
  T_ENV=()
  t_eq "$t_rc" 1
  t_contains "$t_out" "may not write $p"
  t_eq "$([ -e "$cfg" ] && echo written)" ""
done

t_case "half of the app options, or a made app and a given one, is 64 before any call"
fresh
for part in "--app-id 77" "--app-key-file $T/app.pem" "--new-app --app-id 77 --app-key-file $T/app.pem" "--app-name x" "--app-owner acme"; do
  # shellcheck disable=SC2086  # the options are split on purpose
  run_setup --repo acme/widget --labellers alice --endpoint http://gb1:8086/v1 $part
  t_eq "$t_rc" 64
done
t_eq "$(t_calls gh)$(t_calls curl)" ""
t_eq "$([ -e "$cfg" ] && echo written)" ""

new_args=(--repo acme/widget --labellers "alice bob" --new-app --endpoint http://gb1:8086/v1 --no-build)
manifest_app='{"slug":"widget-factory"}'

t_case "--new-app makes the app from the pasted code and writes its key and ID together"
fresh
T_ENV=("APP_ANSWER=$manifest_app")
run_setup_given abc123 "${new_args[@]}"
T_ENV=()
t_eq "$t_rc" 0
t_eq "$(head -n 1 "$cfg/github-app.pem")" "-----BEGIN RSA PRIVATE KEY-----"
t_eq "$(stat -c %a "$cfg/github-app.pem")" 600
t_eq "$(grep '^TEHAS_GITHUB_APP_ID=' "$cfg/instance.env")" "TEHAS_GITHUB_APP_ID=9"
t_eq "$(grep -c '^TEHAS_GITHUB_APP_ID=' "$cfg/instance.env")" 1
t_eq "$(cat "$T/home/tehas/state/github/login")" "widget-factory[bot]"
t_eq "$(git config -f "$T/home/.gitconfig" user.email)" "9191+widget-factory[bot]@users.noreply.github.com"
t_contains "$t_out" "made the GitHub App widget-factory: TEHAS_GITHUB_APP_ID=9 written to"
t_contains "$t_out" "Install the app on acme: https://github.com/apps/widget-factory/installations/new"
t_contains "$t_out" "new-app.html on this host is the same page. Copy it to the machine your browser runs on"
t_eq "$([ -e "$cfg/new-app.html" ] && echo kept)" ""
t_eq "$(t_calls tehasbot)" "github-token --check"

t_case "the link posts the manifest to the organisation's new-app form, and names the app after the repository"
page=$(linked_page)
t_contains "$page" 'method="post"'
t_contains "$page" 'action="https://github.com/organizations/acme/settings/apps/new?state='
t_contains "$page" 'submit()'
manifest=$(sed -n 's/.*name="manifest" value="\([^"]*\)".*/\1/p' <<<"$page" | sed -e 's/&quot;/"/g' -e 's/&amp;/\&/g')
t_eq "$(jq -c '[.name, .url, .redirect_url, .hook_attributes.active, .default_events, (.default_permissions | keys)]' <<<"$manifest")" '["widget-factory","https://github.com/acme","https://github.com/acme",false,[],["actions","checks","contents","issues","metadata","pull_requests","statuses"]]'
state=$(sed -n 's/.*?state=\([0-9a-f]*\)".*/\1/p' <<<"$page")
t_eq "$([ "${#state}" -ge 32 ] && echo long)" long

t_case "the code goes to curl on standard input only, and no key or secret is printed"
t_contains "$(t_calls conversion)" "https://api.github.com/app-manifests/abc123/conversions"
t_lacks "$(t_calls curl)" "abc123"
t_lacks "$t_out" "BEGIN"
t_lacks "$t_out" "cs_secret"
t_lacks "$t_out" "wh_secret"
t_lacks "$(cat "$cfg/instance.env" "$cfg/models.json")" "secret"

t_case "a second --new-app finds the app the instance names and only checks it"
stub_all
T_ENV=("APP_ANSWER=$manifest_app")
run_setup "${new_args[@]}"
T_ENV=()
t_eq "$t_rc" 0
t_contains "$t_out" "instance.env names the app 9 and its key is at $cfg/github-app.pem: it is checked, no new app is made"
t_eq "$(t_calls conversion)" ""
t_lacks "$t_out" "data:text/html"

t_case "a key with no app ID in instance.env refuses --new-app and points to --app-id"
fresh
mkdir -p "$cfg"
printf 'a key\n' >"$cfg/github-app.pem"
run_setup "${new_args[@]}"
t_eq "$t_rc" 78
t_contains "$t_out" "give --app-id <id> --app-key-file $cfg/github-app.pem; --new-app does not make a second app"
t_eq "$(t_calls curl)" ""

t_case "an app ID whose key is gone refuses --new-app"
fresh
mkdir -p "$cfg"
printf 'TEHAS_GITHUB_APP_ID=9\n' >"$cfg/instance.env"
run_setup "${new_args[@]}"
t_eq "$t_rc" 78
t_contains "$t_out" "instance.env names the app 9, and its key is not at $cfg/github-app.pem"
t_eq "$(t_calls curl)" ""

t_case "an address with another state, or none, is refused and no app is made"
for pasted in "https://github.com/acme?code=abc123&state=0123456789abcdef0123456789abcdef" "https://github.com/acme?code=abc123" "abc 123"; do
  fresh
  run_setup_given "$pasted" "${new_args[@]}"
  t_eq "$t_rc" 78
  t_contains "$t_out" "no app was made, run tehas setup --new-app again"
  t_eq "$(t_calls conversion)" ""
  t_eq "$([ -e "$cfg/github-app.pem" ] && echo written)" ""
done

t_case "a code GitHub does not take is exit 1, says a code works once, and writes no key or ID"
fresh
T_ENV=(CONVERT_FAIL=1)
run_setup_given abc123 "${new_args[@]}"
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "a code lasts one hour and works once"
t_eq "$(t_calls conversion | grep -c conversions)" 1
t_eq "$([ -e "$cfg/github-app.pem" ] && echo written)" ""
t_lacks "$(cat "$cfg/instance.env" 2>/dev/null)" "TEHAS_GITHUB_APP_ID"

t_case "a user's app posts to the user's form, with the name given"
fresh
T_ENV=("APP_ANSWER=$manifest_app" OWNER_TYPE=User)
run_setup_given abc123 "${new_args[@]}" --app-owner alice --app-name "Widget bot"
T_ENV=()
t_eq "$t_rc" 0
page=$(linked_page)
t_contains "$page" 'action="https://github.com/settings/apps/new?state='
t_contains "$page" '&quot;name&quot;:&quot;Widget bot&quot;'
t_contains "$page" '&quot;url&quot;:&quot;https://github.com/alice&quot;'

t_case "the page is never written through a link, and the run goes on with the data link"
fresh
mkdir -p "$cfg"
echo kept >"$T/elsewhere"
ln -s "$T/elsewhere" "$cfg/new-app.html"
T_ENV=("APP_ANSWER=$manifest_app")
run_setup_given abc123 "${new_args[@]}"
T_ENV=()
t_eq "$t_rc" 0
t_eq "$(cat "$T/elsewhere")" kept
t_contains "$t_out" "could not write $cfg/new-app.html"
t_lacks "$t_out" "on this host is the same page"

t_case "a link where the key would go stops --new-app before a code is asked for"
fresh
mkdir -p "$cfg"
ln -s "$T/nowhere.pem" "$cfg/github-app.pem"
run_setup_given abc123 "${new_args[@]}"
t_eq "$t_rc" 78
t_contains "$t_out" "the new app's key cannot go to $cfg/github-app.pem"
t_lacks "$t_out" "data:text/html"
t_eq "$(t_calls conversion)" ""
t_eq "$([ -e "$T/nowhere.pem" ] && echo written)" ""

t_case "--new-app with the model key on standard input is 78: the code comes there"
fresh
run_setup "${new_args[@]}" --model-key-file -
t_eq "$t_rc" 78
t_contains "$t_out" "--new-app reads GitHub's code on standard input"

t_case "a step that fails stops the run with its status"
fresh
printf '#!/usr/bin/env bash\nexit 78\n' >"$plat/install.sh"
run_setup "${args[@]}" --no-build
t_eq "$t_rc" 78
t_contains "$t_out" "install.sh"
t_eq "$(t_calls systemctl)" ""

t_case "an unknown option is 64"
run_setup --bogus
t_eq "$t_rc" 64

t_case "no console beside the command and no checkout to build it is 78, before any call"
fresh
mv "$plat/libexec/console" "$T/console-aside"
run_setup "${args[@]}"
mv "$T/console-aside" "$plat/libexec/console"
t_eq "$t_rc" 78
t_contains "$t_out" "run install.sh"
t_eq "$(t_calls gh)$(t_calls curl)" ""
t_eq "$([ -e "$cfg" ] && echo written)" ""

t_case "a checkout's first setup loads with the console it built, and install.sh is not given that console"
fresh
tree=$T/checkout
mkdir -p "$tree/libexec" "$tree/console"
cp "$T_ROOT/libexec/setup" "$tree/libexec/setup"
cp -R "$T_ROOT/lib" "$tree/lib"
: >"$tree/console/Cargo.toml"
printf '#!/usr/bin/env bash\necho "install.sh hook=${TEHAS_SETTINGS_CMD:-none}" >>"$T/calls/steps"\n' >"$tree/install.sh"
chmod +x "$tree/install.sh"
t_stub cargo 'mkdir -p "$CARGO_TARGET_DIR/release" && cp "$T/plat/libexec/console" "$CARGO_TARGET_DIR/release/console"'
t_run "unset TEHAS_SETTINGS_CMD; $tree/libexec/setup $(printf '%q ' "${args[@]}") --no-build </dev/null"
t_eq "$t_rc" 0
t_lacks "$t_out" "no console at"
t_eq "$(t_calls steps)" "install.sh hook=none
labels"

t_case "a checkout's first setup with an app loads with the console it built too"
fresh
rm -f "$T/target/release/console"
t_run "unset TEHAS_SETTINGS_CMD; $tree/libexec/setup $(printf '%q ' "${app_args[@]}") --no-build </dev/null"
t_eq "$t_rc" 0
t_lacks "$t_out" "no console at"
t_eq "$(t_calls steps)" "install.sh hook=none
labels"

t_done
