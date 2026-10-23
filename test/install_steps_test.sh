#!/usr/bin/env bash
# Tests for install.sh: the shipped instances are valid, the follow-ups a
# run asks for are recorded and reported, and the store's database is made,
# migrated and left alone by --check.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

t_stub nproc 'echo 12'
for name in example tehas-tehas; do
  t_case "the shipped instance '$name' is valid"
  fresh
  export TEHAS_INSTANCE_DIR=$T_ROOT/instances/$name
  # The tokens are the host's, not the instance's: stand-ins for each it names.
  keep=("${T_ENV[@]}")
  while read -r k; do
    [ -z "$k" ] || T_ENV+=("$k=stand-in")
  done < <(bash -c ". $T_ROOT/lib/models.sh; tehas_model_key_names $TEHAS_INSTANCE_DIR/models.json")
  t_run "$install --check"
  t_lacks "$t_out" "INVALID"
  t_contains "$t_out" "Instance:     $T_ROOT/instances/$name OK"
  T_ENV=("${keep[@]}")
done
rm -f "$T/bin/nproc"

t_case "a changed unit file asks for the units step, whatever systemd answers"
fresh
instance
# Past a first install's enable, install.sh asks systemd for nothing, so nothing
# here can report a changed unit as applied while systemd still holds the old
# definition. The reload belongs to the step that restarts the units, which is
# the only place that can tell whether it happened. This is a first install
# whose enable fails: the units stay owed, and so does the first install.
t_stub systemctl 'echo "Failed to connect to bus: No such file or directory" >&2; exit 1'
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "next: units tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-mcp.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer tehas-refresh.timer tehas-clean.timer"
t_eq "$(t_calls systemctl)" "--user daemon-reload"
t_contains "$t_out" "the first install's labels or units are not done: systemctl --user enable failed"
t_eq "$([ -e "$th/state/first-install" ] && echo owed)" owed
t_contains "$(cat "$th/state/update-pending")" "units tehas-dispatch.timer"

t_case "a run that dies partway leaves the copies it made owed"
fresh
instance
# A checkout missing one source: put() stops the run with no follow-up report at
# all. The files it did copy are on the host, and what they call for was recorded
# as each of them went - so the next tehas update still runs those steps, and
# install.sh --check still says the host is not current.
part=$T/partial
rm -rf "$part"
mkdir -p "$part"
cp -R "$T_ROOT/install.sh" "$T_ROOT/lib" "$T_ROOT/host" "$T_ROOT/bin" "$part/"
t_console_source "$part"
chmod +x "$part/install.sh"
rm "$part/host/pi/mcp.json"
t_run "$part/install.sh"
t_eq "$t_rc" 1
t_contains "$t_out" "is missing from this checkout"
t_lacks "$t_out" "next: "
t_eq "$(cat "$th/state/update-pending")" "units tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-mcp.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer tehas-refresh.timer tehas-clean.timer
sandbox
docs --force-recreate
edge --force-recreate"

t_case "the follow-ups it prints are the ids lib/update-steps.sh defines"
fresh
instance
t_stub systemctl
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(printf '%s\n' "$t_out" | sed -n 's/^next: //p')" \
  "$(tehas_steps_for systemd/tehas-dispatch.timer sandbox/Dockerfile versions.env edge/docs.compose.yaml edge/router.compose.yaml libexec/labels)"
# Steps, not shell text for a person to paste.
t_lacks "$t_out" "docker compose -f"
t_lacks "$t_out" "systemctl --user enable"

t_case "--check reports follow-ups an earlier run left undone"
# One of the two lines is a step no version of tehas here knows: install.sh --check
# reports whatever is in the record, because it cannot perform any of them either.
fresh
instance
t_run "$install"
mkdir -p "$th/state"
printf 'sandbox\ndocs\n' >"$(TEHAS_HOME=$th tehas_pending_file)"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Follow-ups:   NOT DONE (2 from an earlier run"
t_contains "$t_out" "Installed:    OK"
fresh
instance
t_run "$install --check"
t_lacks "$t_out" "Follow-ups:"

t_case "--check says a record it cannot read is not done, and does not count it"
fresh
instance
t_run "$install"
rm -f "$th/state/update-pending"
mkdir -p "$th/state"
printf 'sandbox\ndocs --force-recreate\n' >"$th/state/update-pending"
chmod 000 "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Follow-ups:   NOT DONE (the record $th/state/update-pending cannot be read)"
chmod 644 "$th/state/update-pending"
t_eq "$(cat "$th/state/update-pending")" "sandbox
docs --force-recreate"

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
# No terminal and nothing on standard input: a program runs it from a pipe, or
# with </dev/null, and the run must end by itself rather than stop for a
# question. A prompt would leave this hanging and fail on the timeout.
t_run "$install </dev/null"
t_eq "$t_rc" 0
# The report of a real run: one line per thing it did, and the follow-ups it
# leaves for tehas update. None of it asks for anything.
t_contains "$t_out" "installed: $th/libexec/dispatch"
t_contains "$t_out" "next: units"

t_case "the machine reads back as one JSON object, with no secret in it"
fresh
instance TEHAS_FIXER_LOGIN=bob
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
  .fixer == "bob" and (.features | index("limit")) != null' >/dev/null && echo ok)" "ok"
# A model token is named, never written: the file that carries these facts is
# the one a person may put anywhere, so no value of a TEHAS_MODEL_KEY_ name
# appears in it.
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
# The finding names the file to put the token in, so the person is not left
# guessing where that file is. Spelled with the tilde the command prints, not
# expanded by the test: SC2088 would only ask for that.
# shellcheck disable=SC2088
t_contains "$t_out" "~/.config/tehas/env"
# Nothing changed, and no host to inspect afterwards: an unattended run that
# cannot go on leaves the machine as it found it. The token file is removed
# again here: a case after this one reads it, and a leftover would hide what
# that case is checking.
t_eq "$(core_files)" ""
t_eq "$(t_calls cargo)" ""
rm -f "$home/.config/tehas/env"


t_case "an install runs the store with what this tree and the instance say"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(t_calls store)" "init --version $(git -C "$T_ROOT" describe --tags --abbrev=0 2>/dev/null | sed 's/$/+/')$(git -C "$T_ROOT" rev-parse --short=7 HEAD) --commit $(git -C "$T_ROOT" rev-parse HEAD) --instance instance --repo acme/widget"
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
cp -R "$T_ROOT/install.sh" "$T_ROOT/lib" "$T_ROOT/host" "$T_ROOT/bin" "$T_ROOT/libexec" "$nocommit/"
t_console_source "$nocommit"
rm -f "$nocommit/COMMIT"
t_run "$nocommit/install.sh"
t_eq "$t_rc" 1
t_contains "$t_out" "the commit of $nocommit is not known"
t_eq "$(host_files | wc -l)" 0
t_eq "$(t_calls cargo)" ""

# The real store from here on, built once for the run: the database it makes is
# the one a host gets.
real_store() { # <case name>: 0 when the store is there to run
  t_case "$1"
  t_store && return 0
  t_skip "$t_store_why"
  return 1
}
if real_store "a first install makes state/tehas.db 0600 in a 0700 state/"; then
  fresh
  instance
  t_env STORE_BIN="$T_STORE"
  # The umask a shell gives would leave state/ 0755 and the file 0644.
  t_run "umask 022; $install"
  t_eq "$t_rc" 0
  t_contains "$t_out" "store: made $th/state/tehas.db"
  t_eq "$(stat -c %a "$th/state")" 700
  t_eq "$(stat -c %a "$th/state/tehas.db")" 600
  t_run "TEHAS_HOME='$th' '$th/libexec/store' check"
  t_eq "$t_rc" 0
  t_eq "$(jq -r '"\(.schema) \(.integrity)"' <<<"$t_out")" "1 ok"
  t_eq "$([ -e "$th/state/backup" ] && echo there)" ""
fi

if real_store "a second install migrates and leaves a backup under state/backup/"; then
  chmod 755 "$th/state"
  t_run "umask 022; $install"
  t_eq "$t_rc" 0
  t_contains "$t_out" "store: $th/state/tehas.db at schema 1, backup $th/state/backup/tehas-1-"
  t_eq "$(stat -c %a "$th/state")" 700
  t_eq "$(stat -c %a "$th/state/backup")" 700
  copies=$(find "$th/state/backup" -type f -name 'tehas-1-*.db')
  t_eq "$(printf '%s\n' "$copies" | grep -c .)" 1
  t_eq "$(stat -c %a "$copies")" 600
  t_eq "$(stat -c %a "$th/state/tehas.db")" 600
fi

if real_store "a --check run writes nothing, to the database or beside it"; then
  # state/ and libexec/: the database, its copies and the store. A check's own
  # scratch files come and go under tmp/, as they did before the store.
  snap() { find "$th/state" "$th/libexec" -printf '%p %m %s %T@\n' | LC_ALL=C sort; }
  before=$(snap)
  t_run "$install --check"
  t_eq "$(snap)" "$before"
  fresh
  instance
  t_run "$install --check"
  t_eq "$(find "$home" -mindepth 1 | wc -l)" 0
fi

if real_store "a database the install did not make is migrated and not replaced"; then
  fresh
  instance
  t_store_db "$th"
  made=$(sha256sum <"$th/state/tehas.db")
  t_run "$install"
  t_eq "$t_rc" 0
  t_contains "$t_out" "backup $th/state/backup/tehas-1-"
  t_eq "$(sha256sum <"$th/state/tehas.db")" "$made"
  t_lacks "$t_out" "store: made"
fi

if real_store "a database that is not one stops the install and is left as it was"; then
  fresh
  instance
  mkdir -p "$th/state"
  printf 'not a database\n' >"$th/state/tehas.db"
  t_run "$install"
  t_eq "$t_rc" 1
  t_contains "$t_out" "corrupt"
  t_contains "$t_out" "tehas: the store could not migrate $th/state/tehas.db"
  t_eq "$(cat "$th/state/tehas.db")" "not a database"
fi

if real_store "a home reached through a link installs a second time"; then
  fresh
  instance
  mkdir -p "$T/disk"
  ln -s "$T/disk" "$th"
  t_run "$install"
  t_eq "$t_rc" 0
  t_run "$install"
  t_eq "$t_rc" 0
  t_contains "$t_out" "backup $T/disk/state/backup/tehas-1-"
  t_eq "$(find "$T/disk/state/backup" -type f -name 'tehas-1-*.db' | wc -l)" 1
  rm -rf "$T/disk"
fi
t_env

t_done
