#!/usr/bin/env bash
# Tests for install.sh against the real store: the database a first install
# makes, how an install migrates it, and --check leaving it alone.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

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
  t_eq "$(jq -r '"\(.schema) \(.integrity)"' <<<"$t_out")" "12 ok"
  t_eq "$([ -e "$th/state/backup" ] && echo there)" ""
fi

if real_store "a second install migrates and leaves a backup under state/backup/"; then
  chmod 755 "$th/state"
  t_run "umask 022; $install"
  t_eq "$t_rc" 0
  t_contains "$t_out" "store: $th/state/tehas.db at schema 12, backup $th/state/backup/tehas-12-"
  t_eq "$(stat -c %a "$th/state")" 700
  t_eq "$(stat -c %a "$th/state/backup")" 700
  copies=$(find "$th/state/backup" -type f -name 'tehas-12-*.db')
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
  t_contains "$t_out" "backup $th/state/backup/tehas-12-"
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
  t_contains "$t_out" "backup $T/disk/state/backup/tehas-12-"
  t_eq "$(find "$T/disk/state/backup" -type f -name 'tehas-12-*.db' | wc -l)" 1
  rm -rf "$T/disk"
fi
t_env

# The console that names the steps, where nothing else names one: a real run needs
# it before its first copy, so these run with no TEHAS_CONSOLE and the console a
# build leaves is the real one.
t_case "with the console due, the one this run built records the steps before it is put"
fresh
instance
real_console
T_ENV_ALWAYS=(GH_TOKEN=ghs_test)
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "next: sandbox"
t_contains "$t_out" "next: labels"
t_eq "$(cat "$th/state/update-pending")" "$(next_steps)"

t_case "with nothing due, the one installed records them and nothing is built"
: >"$T/calls/cargo"
rm -f "$th/state/update-pending"
echo "# a change" >>"$th/env.sh"
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(t_calls cargo)" ""
t_contains "$t_out" "next: units tehas-dispatch.timer"
t_contains "$(cat "$th/state/update-pending")" "units tehas-dispatch.timer"

t_case "a record of steps that cannot be written stops the run at the first copy"
rm -f "$th/state/update-pending"
echo "# another change" >>"$th/env.sh"
# A directory where the record goes: it can be neither read nor replaced.
mkdir -p "$th/state/update-pending/in"
t_run "$install"
rm -rf "$th/state/update-pending"
t_eq "$t_rc" 1
t_contains "$t_out" "tehas: could not record the follow-ups of env.sh; the copies so far are installed"
t_contains "$(cat "$th/env.sh")" "# another change"

t_done
