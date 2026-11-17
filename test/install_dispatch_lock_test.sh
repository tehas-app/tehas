#!/usr/bin/env bash
# Tests for install.sh and the dispatcher's lock: a real run holds it from the
# first copy until the old files are removed, so no poll starts anything in
# between. And a store older than v0.30.0 wrote stops the run with the store's
# own line before anything is copied: there is no import of the files an older
# tehas kept any more.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

real_store() { # <case name>: 0 when the store is there to run
  t_case "$1"
  t_store && return 0
  t_skip "$t_store_why"
  return 1
}

# An installed host, with its marker and its store.
installed() {
  fresh
  instance
  t_run "$install"
  t_eq "$t_rc" 0
}
# hold <lock file>: the lock held by a process of its own until unhold.
hold() {
  mkdir -p "${1%/*}"
  bash -c 'exec 8>"$1"; flock 8; exec sleep 60' _ "$1" &
  holder=$!
  for _ in $(seq 50); do flock -n "$1" true || break; sleep 0.1; done
}
unhold() {
  kill "$holder" 2>/dev/null
  wait "$holder" 2>/dev/null
}

t_case "a fresh host installs, gets the marker and gives the lock back"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(t_calls store | cut -d' ' -f1 | tr '\n' ' ')" "init "
t_eq "$(cat "$th/.tehas-home")" "tehas"
t_eq "$(flock -n "$th/state/dispatch.lock" true && echo free)" "free"

t_case "a second run asks the store for nothing but its database"
t_run "$install"
t_eq "$t_rc" 0
# The stand-in store makes no database, so the second run makes it again.
t_eq "$(t_calls store | cut -d' ' -f1-2 | tr '\n' ' ')" "init --version init --version "

if real_store "a store older than v0.30.0 wrote stops the run before anything is copied"; then
  if ! command -v sqlite3 >/dev/null 2>&1; then
    # The gate needs no sqlite3; the store's own tests prove the refusal.
    t_skip "no sqlite3 to age the store with"
  else
    t_env STORE_BIN="$T_STORE"
    installed
    sqlite3 "$th/state/tehas.db" "PRAGMA user_version = 11; UPDATE meta SET value = '11' WHERE key = 'schema';"
    # A file each copy would put back, gone, and the store as it is now.
    rm -f "$th/bin/tehas"
    before=$(sha256sum "$th/state/tehas.db" "$th/libexec/store")
    t_run "$install"
    t_eq "$t_rc" 78
    t_contains "$t_out" "older than v0.30.0 wrote: update to a v0.30 release first, then to this one"
    t_contains "$t_out" "tehas: this release cannot migrate $th/state/tehas.db; nothing was installed"
    t_lacks "$t_out" "the store could not migrate"
    t_eq "$([ -e "$th/bin/tehas" ] || echo gone)" "gone"
    t_eq "$(sha256sum "$th/state/tehas.db" "$th/libexec/store")" "$before"
    t_eq "$(find "$th/state/backup" -type f 2>/dev/null | wc -l)" 0
    t_eq "$(flock -n "$th/state/dispatch.lock" true && echo free)" "free"
  fi
fi
t_env

t_case "a poll that holds the dispatcher's lock is waited for past the update lock's wait"
installed
bash -c 'exec 8>>"$1"; flock 8; exec sleep 2' _ "$th/state/dispatch.lock" &
poll=$!
for _ in $(seq 50); do flock -n "$th/state/dispatch.lock" true || break; sleep 0.1; done
t_run "${no_wait}'$install'"
wait "$poll" 2>/dev/null
t_eq "$t_rc" 0

t_case "TEHAS_DISPATCH_LOCK_WAIT bounds the wait for a poll"
hold "$th/state/dispatch.lock"
rm -f "$th/bin/tehas"
t_run "export TEHAS_DISPATCH_LOCK_WAIT=0; '$install'"
unhold
t_eq "$t_rc" 1
t_contains "$t_out" "tehas: a poll is running ($th/state/dispatch.lock still held after 0s); nothing was installed"
t_eq "$([ -e "$th/bin/tehas" ] || echo gone)" "gone"

if real_store "a held cycle lock with no old record refuses nothing"; then
  installed
  hold "$th/state/cycles/9.lock"
  t_run "$install"
  unhold
  t_eq "$t_rc" 0
fi

if real_store "a poll during the install finds the dispatcher's lock held"; then
  # The store this run installs reports, each time it is asked to migrate,
  # whether a poll could take the lock then.
  cat >"$T/store-watch" <<EOF
#!/usr/bin/env bash
if [ "\$1" = migrate ]; then
  if flock -n "\$TEHAS_HOME/state/dispatch.lock" true; then echo free; else echo held; fi >>"\$T/calls/poll"
fi
exec "$T_STORE" "\$@"
EOF
  chmod +x "$T/store-watch"
  t_env STORE_BIN="$T/store-watch"
  installed
  rm -f "$T/calls/poll"
  t_run "$install"
  t_eq "$t_rc" 0
  t_eq "$(t_calls poll | sort -u)" "held"
  t_eq "$(flock -n "$th/state/dispatch.lock" true && echo free)" "free"
fi
t_env

t_done
