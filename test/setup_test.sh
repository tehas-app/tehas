#!/usr/bin/env bash
# Tests for libexec/setup, the first setup of an instance: what it asks
# GitHub and the model endpoint, the files it writes and their modes, that a
# second run changes nothing, that it never overwrites a setting without
# --force, and that no token is ever on a command line. A scratch platform
# tree stands in for the install: its install.sh, labels and check-product
# record that they ran.
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
printf '#!/usr/bin/env bash\necho labels $* >>"$T/calls/steps"\n' >"$plat/libexec/labels"
mkdir -p "$(dirname "$build")"
printf '#!/usr/bin/env bash\necho build $* >>"$T/calls/steps"\n' >"$build"
chmod +x "$plat/install.sh" "$plat/libexec/labels" "$build"

printf 'ghp_secret_token\n' >"$T/token"
printf 'sk_model_key\n' >"$T/model-key"

# The github.com host, the key of its credential helper, and the helper command
# gh writes, all read from lib/sandbox.sh. The container's git, the probe of
# tehas setup and the gh stub below read one contract, so a change to its
# spelling moves the three together and a drift fails here, not on a host.
# shellcheck source=lib/sandbox.sh
. "$T_ROOT/lib/sandbox.sh"
HK=$(tehas_git_credential_helper_key)
HG=$(tehas_git_github_host)
HB=$(tehas_sandbox_helper_command)

# gh: the token can push (PUSH=false: it cannot), the bot is tehas-bot, the
# app's installation lists APP_REPOS and its bot has the ID 9191, a new
# issue is number 7, a clone makes the checkout. setup-git writes what gh writes
# on a host it is logged into: the helper key twice, an empty entry that ends
# any list before it and gh's command after it. GH_TOKEN is the knob of a host
# gh is not logged into, SETUP_GIT_RC of a gh that cannot write the config. The
# key and the command come from lib/sandbox.sh, so the stub writes what the
# container is given and cannot drift from it.
stub_gh() {
  # The body is written with @HK@, @HG@ and @HB@ in place of the key, the host
  # and the command, and the stubs of the other commands are written with @T@
  # in place of the scratch directory: t_stub echoes its body unquoted, so the
  # three names and the path are substituted here, before it does.
  t_stub gh "$(printf '%s' 'case "$*" in
  "api repos/acme/widget --jq .permissions.push") echo "${PUSH:-true}" ;;
  "api user --jq [.login,.id]|@tsv") printf "tehas-bot\t4242\n" ;;
  "api --paginate installation/repositories --jq .repositories[].full_name") printf "%s\n" ${APP_REPOS:-acme/other acme/Widget} ;;
  "api users/tehas-app[bot] --jq .id") echo 9191 ;;
  "issue create --repo acme/widget"*) echo "https://github.com/acme/widget/issues/7" ;;
  "repo clone acme/widget "*) mkdir -p "${4}/.git" ;;
  "auth setup-git --hostname @HG@")
    [ -n "${GH_TOKEN:-}" ] || { echo "not logged into @HG@" >&2; exit 1; }
    [ "${SETUP_GIT_RC:-0}" = 0 ] || exit "${SETUP_GIT_RC}"
    git config --global --replace-all @HK@ "" || exit 1
    git config --global --add @HK@ @HB@ || exit 1 ;;
  *) exit 1 ;;
esac
[ -z "${GH_TOKEN:-}" ] || printf "%s\n" "$GH_TOKEN" >"@T@/gh-token"' | sed -e "s|@HK@|$HK|g" -e "s|@HG@|$HG|g" -e "s|@HB@|\"$HB\"|g" -e "s|@T@|$T|g")"
}

# curl: the endpoint's model list (MODELS, space separated). The config it was
# given on stdin is kept, to see the key went there and not on the line. For
# the app: GET /app answers the slug tehas-app, and a minted token has the
# permissions PERMS, by default write on all three that setup needs.
stub_curl() {
  t_stub curl 'case "$*" in
*access_tokens) printf "{\"token\":\"ghs_minted\",\"expires_at\":\"2099-01-01T00:00:00Z\",\"permissions\":%s}\n" "${PERMS:-{\"contents\":\"write\",\"issues\":\"write\",\"pull_requests\":\"write\"\}}"; exit 0 ;;
*/app) printf "{\"slug\":\"tehas-app\"}\n"; exit 0 ;;
esac
cat >"$T/curl-stdin"
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
  t_stub openssl 'case "$1" in base64) base64 -w0 ;; dgst) cat >/dev/null; printf signature ;; esac'
  t_stub docker 'case "$1 $2" in "image inspect") [ -e "$T/image-there" ] ;; esac'
}
fresh() {
  rm -rf "$cfg" "$T/home/.gitconfig" "$T/home/tehas/repos"
  stub_all
}

args=(--repo acme/widget --labellers "alice bob" --token-file "$T/token" --endpoint http://gb1:8086/v1)
# Every run of setup reads the host's git config as the host has it: t_run
# starts from an empty environment and carries over only what T_ENV names, so
# git's environment-level config - the scope that outranks every file - is not
# there unless a case puts those entries in T_ENV itself. One entry per value,
# quoted, because t_run hands T_ENV to env as separate words.
run_setup() { t_run "$setup $(printf '%q ' "$@") </dev/null"; }

t_case "a first run writes the required settings, the model list and the token, and starts the factory"
fresh
run_setup "${args[@]}"
t_eq "$t_rc" 0
t_eq "$(grep -v -e '^#' -e '^$' "$cfg/instance.env")" 'TEHAS_REPO=acme/widget
TEHAS_CONTROL_ISSUE=7
TEHAS_ALLOWED_LABELERS="alice bob"
PI_ENSEMBLE_SUBAGENT_PROVIDER=main
PI_ENSEMBLE_SUBAGENT_MODEL=m1'
t_eq "$(jq -cS . "$cfg/models.json")" '{"providers":{"main":{"api":"openai-completions","apiKey":"unused","baseUrl":"http://gb1:8086/v1","models":[{"id":"m1"}]}}}'
t_eq "$(cat "$cfg/env")" "GH_TOKEN=ghp_secret_token"
t_eq "$(stat -c %a "$cfg/env")" 600
t_eq "$(stat -c %a "$cfg")" 700
t_eq "$(git config -f "$T/home/.gitconfig" user.name)" "tehas-bot"
t_eq "$(git config -f "$T/home/.gitconfig" user.email)" "4242+tehas-bot@users.noreply.github.com"
t_eq "$([ -d "$T/home/tehas/repos/widget/.git" ] && echo cloned)" cloned
t_eq "$(t_calls steps)" "install.sh
labels
build"
t_contains "$(t_calls systemctl)" "--user enable --now tehas-dispatch.timer tehas-dispatch.path"
t_contains "$t_out" "check-product is not installed"
t_contains "$t_out" "http://127.0.0.1:8080/"
t_eq "$(cat "$T/gh-token")" "ghp_secret_token"

t_case "no token is on any command line"
t_lacks "$(cat "$T"/calls/*)" "ghp_secret_token"

t_case "a second run changes nothing and makes no issue"
before=$(cd "$cfg" && sha256sum instance.env models.json env; git config -f "$T/home/.gitconfig" -l)
stub_all
: >"$T/image-there"
run_setup "${args[@]}"
t_eq "$t_rc" 0
t_eq "$(cd "$cfg" && sha256sum instance.env models.json env; git config -f "$T/home/.gitconfig" -l)" "$before"
t_lacks "$(t_calls gh)" "issue create"
t_lacks "$(t_calls steps)" "build"
t_contains "$t_out" "the sandbox image is there"

t_case "the settings already in instance.env fill what the command line leaves out"
stub_all
: >"$T/image-there"
run_setup --token-file "$T/token" --endpoint http://gb1:8086/v1
t_eq "$t_rc" 0
t_eq "$(cd "$cfg" && sha256sum instance.env models.json env; git config -f "$T/home/.gitconfig" -l)" "$before"

t_case "a different setting is refused with 78 and nothing changes, until --force"
stub_all
run_setup --repo acme/widget --labellers "carol" --token-file "$T/token" --endpoint http://gb1:8086/v1
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_ALLOWED_LABELERS"
t_contains "$t_out" "--force"
t_eq "$(cd "$cfg" && sha256sum instance.env models.json env; git config -f "$T/home/.gitconfig" -l)" "$before"
t_eq "$(t_calls gh)$(t_calls curl)" ""
run_setup --repo acme/widget --labellers "carol" --token-file "$T/token" --endpoint http://gb1:8086/v1 --force --no-build
t_eq "$t_rc" 0
t_eq "$(grep '^TEHAS_ALLOWED_LABELERS=' "$cfg/instance.env")" 'TEHAS_ALLOWED_LABELERS="carol"'
t_eq "$(grep -c '^TEHAS_ALLOWED_LABELERS=' "$cfg/instance.env")" 1

t_case "another token is refused with 78 until --force"
printf 'ghp_other\n' >"$T/token2"
stub_all
run_setup "${args[@]}" --token-file "$T/token2"
t_eq "$t_rc" 78
t_contains "$t_out" "GH_TOKEN"
t_eq "$(cat "$cfg/env")" "GH_TOKEN=ghp_secret_token"

t_case "a missing setting with nothing to ask on is 78, before any call"
fresh
run_setup --repo acme/widget --token-file "$T/token" --endpoint http://gb1:8086/v1
t_eq "$t_rc" 78
t_contains "$t_out" "--labellers"
t_eq "$([ -e "$cfg/instance.env" ] && echo written)" ""
t_eq "$(t_calls gh)$(t_calls curl)" ""

t_case "a malformed repository or endpoint is 78, before any call"
fresh
run_setup --repo "acme widget" --labellers alice --token-file "$T/token" --endpoint http://gb1:8086/v1
t_eq "$t_rc" 78
t_contains "$t_out" "owner/name"
run_setup --repo acme/widget --labellers alice --token-file "$T/token" --endpoint gb1:8086
t_eq "$t_rc" 78
t_contains "$t_out" "http"
t_eq "$(t_calls gh)$(t_calls curl)" ""

t_case "a token that cannot push stops before anything is written"
fresh
T_ENV=(PUSH=false)
run_setup "${args[@]}"
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "cannot push to acme/widget"
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

t_case "with no helper for github.com, gh sets one before the clone"
fresh
run_setup "${args[@]}" --no-build
t_eq "$t_rc" 0
t_contains "$t_out" "git uses gh's credential helper for github.com"
t_eq "$(t_calls gh | grep -n 'auth setup-git --hostname github.com' | sed 's/:.*//')" 3
t_contains "$(t_calls gh | tail -n 2)" "repo clone acme/widget"
# What gh wrote is the entry the container's git is given, read back from the
# file with git's own read: the value is there and the step printed no value.
t_eq "$(git config -f "$T/home/.gitconfig" --get-all "$HK" | tr '\n' ' ')" " $HB "

# The run above left gh's own two entries, the empty one and the command, so
# the next run reads a helper out of git's config and asks gh for none.
t_case "the helper gh wrote is kept on the next run, which asks for none"
stub_all
run_setup "${args[@]}" --no-build
t_eq "$t_rc" 0
t_contains "$t_out" "git's helper for github.com is kept (2 in git's config)"
t_lacks "$t_out" "git uses gh's credential helper"
t_lacks "$(t_calls gh)" "auth setup-git"

# The value of the key is a command an operator wrote and can hold a token in
# its arguments, so the step names the key and counts its entries, and prints
# neither the value nor git's words about the file it read.
t_case "a helper whose value holds a secret is named, not printed"
fresh
git config -f "$T/home/.gitconfig" "$HK" '!store --file ~/.token-cache'
mkdir -p "$T/home/tehas/repos/widget/.git"
run_setup "${args[@]}" --no-build
t_eq "$t_rc" 0
t_contains "$t_out" "git's helper for github.com is kept (1 in git's config)"
t_lacks "$t_out" "token-cache"
t_lacks "$t_out" "store"
t_lacks "$(t_calls gh)" "auth setup-git"
t_lacks "$(t_calls gh)" "repo clone"

# gitcredentials ends the helper list at an empty value, so a key whose only
# entry is empty is no helper at all, whatever the file says: the run asks gh
# for its own. The same holds for the operator's deliberate off switch.
t_case "a helper with only an empty value is no helper, so gh is asked for one"
fresh
printf '[credential "https://github.com"]\n\thelper =\n' >>"$T/home/.gitconfig"
run_setup "${args[@]}" --no-build
t_eq "$t_rc" 0
t_contains "$t_out" "git uses gh's credential helper for github.com"
t_contains "$(t_calls gh)" "auth setup-git --hostname github.com"

# The state a factory made by bootstrap.sh and install.sh alone is in: no
# identity and no helper. The helper is asked for first, so this names one of
# the two problems, not both. gh's own words are not inherited into the line.
t_case "gh auth setup-git failing is exit 1, names the helper and clones nothing"
fresh
T_ENV=(SETUP_GIT_RC=3)
run_setup "${args[@]}" --no-build
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "gh auth setup-git failed for github.com (exit 3)"
t_lacks "$t_out" "git's identity"
t_lacks "$(t_calls gh)" "repo clone"
t_eq "$([ -e "$T/home/tehas/repos/widget" ] && echo cloned)" ""

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
printf '#!/usr/bin/env bash\necho check-product $* >>"$T/calls/steps"\n' >"$plat/libexec/check-product"
chmod +x "$plat/libexec/check-product"
run_setup "${args[@]}"
t_eq "$t_rc" 0
t_contains "$(t_calls steps)" "check-product --sandbox"
fresh
run_setup "${args[@]}" --no-build
t_lacks "$(t_calls steps)" "build"
t_lacks "$(t_calls steps)" "check-product"
rm -f "$plat/libexec/check-product"

t_case "a given control issue is used and none is made"
fresh
run_setup "${args[@]}" --control-issue 3 --no-build
t_eq "$t_rc" 0
t_eq "$(grep '^TEHAS_CONTROL_ISSUE=' "$cfg/instance.env")" "TEHAS_CONTROL_ISSUE=3"
t_lacks "$(t_calls gh)" "issue create"

# The shim the install puts first on PATH for an app: it records the call and
# hands gh a token of its own, as tehasbot github-token would.
mkdir -p "$T/home/tehas/shim"
printf '#!/usr/bin/env bash\necho "$*" >>"$T/calls/shim"\nGH_TOKEN=ghs_shim exec "$T/bin/gh" "$@"\n' >"$T/home/tehas/shim/gh"
chmod +x "$T/home/tehas/shim/gh"
printf 'an app key\n' >"$T/app.pem"
app_args=(--repo acme/widget --labellers "alice bob" --app-id 77 --app-installation 88 --app-key-file "$T/app.pem" --endpoint http://gb1:8086/v1)

t_case "an app run writes the app's settings and key, no token, and commits as the app's bot"
fresh
run_setup "${app_args[@]}" --no-build
t_eq "$t_rc" 0
t_eq "$(grep -v -e '^#' -e '^$' "$cfg/instance.env")" 'TEHAS_REPO=acme/widget
TEHAS_CONTROL_ISSUE=7
TEHAS_ALLOWED_LABELERS="alice bob"
PI_ENSEMBLE_SUBAGENT_PROVIDER=main
PI_ENSEMBLE_SUBAGENT_MODEL=m1
TEHAS_GITHUB_APP_ID=77
TEHAS_GITHUB_APP_INSTALLATION=88
TEHAS_BOT_LOGIN=tehas-app[bot]'
t_eq "$(cat "$cfg/github-app.pem")" "an app key"
t_eq "$(stat -c %a "$cfg/github-app.pem")" 600
t_eq "$([ -e "$cfg/env" ] && echo written)" ""
t_eq "$(git config -f "$T/home/.gitconfig" user.name)" "tehas-app[bot]"
t_eq "$(git config -f "$T/home/.gitconfig" user.email)" "9191+tehas-app[bot]@users.noreply.github.com"
t_lacks "$(t_calls gh)" "permissions.push"
t_lacks "$(t_calls gh)" "api user --jq"
t_contains "$(t_calls gh | head -n 2)" "installation/repositories"
t_contains "$(t_calls shim)" "repo clone acme/widget"
t_lacks "$(t_calls shim)" "installation/repositories"
t_eq "$(cat "$T/gh-token")" "ghs_shim"
t_lacks "$(cat "$T"/calls/*)" "ghs_minted"

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

t_case "a second app run changes nothing and makes no issue"
before=$(cd "$cfg" && sha256sum instance.env models.json github-app.pem; git config -f "$T/home/.gitconfig" -l)
stub_all
run_setup "${app_args[@]}" --no-build
t_eq "$t_rc" 0
t_eq "$(cd "$cfg" && sha256sum instance.env models.json github-app.pem; git config -f "$T/home/.gitconfig" -l)" "$before"
t_lacks "$(t_calls gh)" "issue create"

t_case "another app key is refused with 78 until --force"
printf 'another key\n' >"$T/app2.pem"
stub_all
run_setup "${app_args[@]}" --app-key-file "$T/app2.pem" --no-build
t_eq "$t_rc" 78
t_contains "$t_out" "github-app.pem"
t_eq "$(cat "$cfg/github-app.pem")" "an app key"
t_eq "$(t_calls gh)$(t_calls curl)" ""

t_case "an installation that does not list the repository stops with 1 and writes nothing"
fresh
T_ENV=(APP_REPOS=acme/other)
run_setup "${app_args[@]}" --no-build
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "does not list acme/widget"
t_eq "$([ -e "$cfg" ] && echo written)" ""
t_lacks "$(t_calls gh)" "issue create"

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

t_case "one or two of the app options is 64, and so is an app with --token-file"
fresh
for part in "--app-id 77" "--app-installation 88" "--app-key-file $T/app.pem" "--app-id 77 --app-installation 88" "--app-id 77 --app-key-file $T/app.pem"; do
  # shellcheck disable=SC2086  # the options are split on purpose
  run_setup --repo acme/widget --labellers alice --endpoint http://gb1:8086/v1 $part
  t_eq "$t_rc" 64
done
run_setup "${app_args[@]}" --token-file "$T/token"
t_eq "$t_rc" 64
t_eq "$(t_calls gh)$(t_calls curl)" ""
t_eq "$([ -e "$cfg" ] && echo written)" ""

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

t_done
