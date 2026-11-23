#!/usr/bin/env bash
# Tests for install.sh, split from install_steps_test.sh: a record no console
# reads, the machine's report, the store it runs, and a checkout whose commit
# is not known is refused.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

# The console reads the record: on a host whose console cannot, --check still
# says the record is owed, and with no record says nothing.
t_case "--check with no console that reads the record says it is not done"
fresh
instance
t_run "$install"
mkdir -p "$th/state"
printf 'sandbox\n' >"$th/state/update-pending"
t_stub no-console 'exit 64'
T_ENV_ALWAYS=(TEHAS_CONSOLE="$T/bin/no-console" GH_TOKEN=ghs_test)
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Follow-ups:   NOT DONE (the record $th/state/update-pending cannot be read)"
rm -f "$th/state/update-pending"
t_run "$install --check"
t_lacks "$t_out" "Follow-ups:"
T_ENV_ALWAYS=(TEHAS_CONSOLE="$T_CONSOLE" GH_TOKEN=ghs_test)

t_case "an install will not record into a record it cannot read"
fresh
instance
mkdir -p "$th/state"
printf 'sandbox\n' >"$th/state/update-pending"
chmod 000 "$th/state/update-pending"
t_run "$install"
t_eq "$t_rc" 1
t_contains "$t_out" "cannot read the follow-up record"
# It stopped rather than replace what was owed with the steps it was copying.
chmod 644 "$th/state/update-pending"
t_eq "$(cat "$th/state/update-pending")" "sandbox"

t_case "a fresh install takes no answer"
fresh
instance
# No terminal and nothing on standard input: the run must end by itself, not
# stop for a question and hang until the timeout.
t_run "$install </dev/null"
t_eq "$t_rc" 0
# The report of a real run: one line per thing it did, and the follow-ups it
# leaves for tehas update. None of it asks for anything.
t_contains "$t_out" "installed: $th/libexec/setup"
t_contains "$t_out" "next: units"

t_case "the machine reads back as one JSON object, with no secret in it"
fresh
instance
mkdir -p "$th/state/github" && printf 'bob[bot]\n' >"$th/state/github/login"
t_run "$install </dev/null"
t_eq "$t_rc" 0
# The token file is what holds them, and a value written into the report would
# be a secret in a file a person may put anywhere.
mkdir -p "$home/.config/tehas"
printf 'TEHAS_MODEL_KEY_MAIN=sekret-main\n' >"$home/.config/tehas/env"
chmod 600 "$home/.config/tehas/env"
json=$(api_json)
t_eq "$t_rc" 0
# One object, one line, and valid JSON: what a program keeps is what it read.
t_eq "$(printf '%s\n' "$json" | wc -l)" 1
t_eq "$(printf '%s' "$json" | jq -e '.api == 1 and .repo == "acme/widget" and
  .millwright == "bob[bot]" and (.features | index("limit")) != null' >/dev/null && echo ok)" "ok"
# A model token is named, never written: a person may put this file anywhere.
t_lacks "$json" "TEHAS_MODEL_KEY_"

t_case "a machine that still owes a follow-up is not reported as installed"
fresh
instance
t_run "$install </dev/null"
t_eq "$t_rc" 0
# The copies of this run have not had their follow-ups run: `tehas update` does
# that. Until they have, --check says the machine is not current.
t_contains "$t_out" "next: units"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Follow-ups:   NOT DONE"
t_run "$install --check"
t_eq "$t_rc" 1
# The record read, the steps run: only then does the check go green, and twice
# in a row, which is what "installed as the instance says" has to mean.
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 0
t_run "$install --check"
t_eq "$t_rc" 0

t_case "a machine missing one model token says the name and installs nothing"
fresh
instance
cat >"$TEHAS_INSTANCE_DIR/models.json" <<'EOF'
{"providers": {"test-provider": {"apiKey": "$TEHAS_MODEL_KEY_MAIN",
  "models": [{"id": "test-model"}, {"id": "session-model"}]}}}
EOF
rm -f "$home/.config/tehas/env"
t_run "$install </dev/null"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_MODEL_KEY_MAIN"
# The finding names the file for the token, with the tilde the command prints.
# shellcheck disable=SC2088
t_contains "$t_out" "~/.config/tehas/env"
# A run that cannot go on leaves the machine as it found it. The token file goes
# again: a later case reads it.
t_eq "$(core_files)" ""
t_eq "$(t_calls cargo)" ""
rm -f "$home/.config/tehas/env"


t_case "an install runs the store with what this tree and the instance say"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(t_calls store)" "init --version $(git -C "$T_ROOT" describe --tags --abbrev=0 2>/dev/null | sed 's/$/+/')$(git -C "$T_ROOT" rev-parse --short=7 HEAD) --commit $(git -C "$T_ROOT" rev-parse HEAD) --instance instance --repo acme/widget --say"
t_contains "$t_out" "store: made $th/state/tehas.db"
t_eq "$(stat -c %a "$th/state")" 700

t_case "a store that fails stops the install with its own line"
fresh
instance
t_env STORE_RC=78
t_run "$install"
t_env
t_eq "$t_rc" 1
t_contains "$t_out" "store: corrupt the database: a stand-in"
t_contains "$t_out" "tehas: the store could not init $th/state/tehas.db; the files are installed, the database was left as it was"
t_lacks "$t_out" "tehas is up to date"
# The record of what is owed is kept, so the next run is not told it is done.
t_contains "$(cat "$th/state/update-pending")" "units"

t_case "a tree with no commit to give the store installs nothing"
fresh
instance
nocommit=$T/nocommit
rm -rf "$nocommit"
mkdir -p "$nocommit"
cp -R "$fixture/install.sh" "$fixture/lib" "$fixture/host" "$fixture/bin" \
  "$fixture/libexec" "$nocommit/"
t_console_source "$nocommit" "$fixture"
rm -f "$nocommit/COMMIT"
t_run "$nocommit/install.sh"
t_eq "$t_rc" 1
t_contains "$t_out" "the commit of $nocommit is not known"
t_eq "$(host_files | wc -l)" 0
t_eq "$(t_calls cargo)" ""

t_done
