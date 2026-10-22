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

# gh: the token can push (PUSH=false: it cannot), the bot is tehas-bot, a new
# issue is number 7, a clone makes the checkout.
stub_gh() {
  t_stub gh 'case "$*" in
  "api repos/acme/widget --jq .permissions.push") echo "${PUSH:-true}" ;;
  "api user --jq [.login,.id]|@tsv") printf "tehas-bot\t4242\n" ;;
  "issue create --repo acme/widget"*) echo "https://github.com/acme/widget/issues/7" ;;
  "repo clone acme/widget "*) mkdir -p "${4}/.git" ;;
  "auth setup-git --hostname github.com") [ "${SETUP_GIT_RC:-0}" = 0 ] || exit 1 ;;
  *) exit 1 ;;
esac
[ -z "${GH_TOKEN:-}" ] || printf "%s\n" "$GH_TOKEN" >"$T/gh-token"'
}
# curl: the endpoint's model list (MODELS, space separated). The config it was
# given on stdin is kept, to see the key went there and not on the line.
stub_curl() {
  t_stub curl 'cat >"$T/curl-stdin"
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
  t_stub docker 'case "$1 $2" in "image inspect") [ -e "$T/image-there" ] ;; esac'
}
fresh() {
  rm -rf "$cfg" "$T/home/.gitconfig" "$T/home/tehas/repos"
  stub_all
}

args=(--repo acme/widget --labellers "alice bob" --token-file "$T/token" --endpoint http://gb1:8086/v1)
run_setup() { t_run "$setup $(printf '%q ' "$@") </dev/null"; }
# One where the steps before it are laid by a snippet: a second $setup inside
# the quotes survives t_run's word splitting.
run_setup_after() { # <snippet> <args...>
  local snip=$1
  shift
  t_run "$snip; \"$setup\" $(printf '%q ' "$@") </dev/null"
}

t_case "a first run writes the required settings, the model list and the token, and starts the factory"
fresh
run_setup "${args[@]}"
t_eq "$t_rc" 0
t_eq "$(grep -v -e '^#' -e '^$' "$cfg/instance.env")" 'TEHAS_REPO=acme/widget
TEHAS_CONTROL_ISSUE=7
TEHAS_ALLOWED_LABELERS="alice bob"
PI_ENSEMBLE_SUBAGENT_PROVIDER=main
PI_ENSEMBLE_SUBAGENT_MODEL=m1'
t_eq "$(jq -c . "$cfg/models.json")" '{"providers":{"main":{"api":"openai-completions","baseUrl":"http://gb1:8086/v1","apiKey":"unused","models":[{"id":"m1"}]}}}'
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
t_contains "$t_out" "gh's credential helper for github.com"
t_eq "$(t_calls gh | grep -n 'auth setup-git --hostname github.com' | sed 's/:.*//')" 3
t_contains "$(t_calls gh | tail -n 2)" "repo clone acme/widget"

t_case "a helper for github.com that is there is kept and named"
fresh
# The state tehas-tehas is in: a helper set by hand at some point with
# gh auth setup-git. The checkout is put in place by hand too, so the run
# clones nothing and the case is about the helper alone.
printf '[credential "https://github.com"]\n\thelper = !/usr/bin/gh auth git-credential\n' >>"$T/home/.gitconfig"
mkdir -p "$T/home/tehas/repos/widget/.git"
run_setup_after 'git config --global user.name Someone; git config --global user.email someone@example.org' "${args[@]}" --no-build
t_eq "$t_rc" 0
t_contains "$t_out" "kept: credential.https://github.com.helper !/usr/bin/gh auth git-credential"
t_lacks "$(t_calls gh)" "auth setup-git"
t_lacks "$(t_calls gh)" "repo clone"

t_case "a host with no git identity is not stopped by the helper failing"
fresh
# The state a factory made by bootstrap.sh and install.sh alone is in: no
# identity and no helper. The helper is asked for first, so this reports the
# helper and never reaches the identity.
T_ENV=(SETUP_GIT_RC=1)
run_setup "${args[@]}" --no-build
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "gh auth setup-git failed"
t_lacks "$t_out" "git's identity"

t_case "gh auth setup-git failing is exit 1 and clones nothing"
fresh
T_ENV=(SETUP_GIT_RC=1)
run_setup "${args[@]}" --no-build
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "gh auth setup-git failed"
t_lacks "$(t_calls gh)" "repo clone"
t_eq "$([ -e "$T/home/tehas/repos/widget" ] && echo cloned)" ""

# The next cases reuse the checkout and the settings the case above wrote, so
# the earlier ones run again first.
stub_all
: >"$T/image-there"
run_setup "${args[@]}" --no-build
t_eq "$t_rc" 0

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

t_done
