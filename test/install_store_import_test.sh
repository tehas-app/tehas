#!/usr/bin/env bash
# Tests for install.sh and the records an older tehas kept as files: the store
# imports them under the dispatcher's lock and moves each to
# state/imported-<day>/, and a cycle or planning turn that runs, or a file the
# import would hold, stops the run before the first copy.
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
today=$(date -u +%Y%m%d)
start=$(date -u -d '2026-10-14T10:00:00Z' +%s)

# An installed host, with its marker and its store, and nothing of an older
# tehas yet.
installed() {
  fresh
  instance
  t_run "$install"
  t_eq "$t_rc" 0
}
# The records an older tehas left for issue 7: a cycle, its approval, its line
# of history, and a planning session of issue 8 with its kept directory.
old_records() {
  local c=$th/state/cycles
  mkdir -p "$c"
  : >"$c/7.taken"
  touch -d @"$start" "$c/7.taken"
  echo alice >"$c/7.by"
  printf '{"at":"2026-10-14T09:59:00Z","by":"alice","how":"label","body_sha256":"%s","links":[]}\n' \
    "$(printf 'a%.0s' {1..64})" >"$c/7.approved.json"
  printf '{"at":"2026-10-14T11:00:00Z","started":%s,"rc":0,"status":"handoff","provider":false,"parked":false}\n' \
    "$((start + 1))" >"$c/7.history.jsonl"
  printf '{"session":"s-1","turns":1,"options":[],"pending":false,"failed":false,"running":false,"by":"alice","label":"tehas:plan"}\n' \
    >"$th/state/plan-8.json"
  mkdir -p "$th/state/plan-8.json.keep/plan-sessions"
  echo kept >"$th/state/plan-8.json.keep/plan-sessions/one"
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

t_case "a fresh host installs with nothing to import"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$t_out" "store: imported"
# The marker is written before the copies, so even a fresh home is judged.
t_eq "$(t_calls store | cut -d' ' -f1 | tr '\n' ' ')" "import init import "
t_eq "$(cat "$th/.tehas-home")" "tehas"
t_eq "$(flock -n "$th/state/dispatch.lock" true && echo free)" "free"

t_case "a host with a marker has its old records judged before the copies"
t_run "$install"
t_eq "$t_rc" 0
# The stand-in store makes no database, so the second run makes it again.
t_eq "$(t_calls store | cut -d' ' -f1-2 | tr '\n' ' ')" "import --dry-run init --version import import --dry-run init --version import "

if real_store "old records are imported, moved to state/imported-<day>/ and counted"; then
  t_env STORE_BIN="$T_STORE"
  installed
  old_records
  t_run "$install"
  t_eq "$t_rc" 0
  t_contains "$t_out" "store: imported cycles 1, approvals 1, cycle_history 1, planning 1; moved 5 files to $th/state/imported-$today/"
  for rel in cycles/7.taken cycles/7.by cycles/7.approved.json cycles/7.history.jsonl plan-8.json; do
    t_eq "$([ -f "$th/state/imported-$today/$rel" ] && [ ! -e "$th/state/$rel" ] && echo moved)" moved
  done
  t_run "TEHAS_HOME='$th' '$th/libexec/store' cycle-show 7"
  t_eq "$(jq -r '"\(.cycle.attempt) \(.cycle.started_by) \(.cycle.end_reason)"' <<<"$t_out")" "1 alice handoff"
  t_run "TEHAS_HOME='$th' '$th/libexec/store' plan-show 8"
  t_eq "$(jq -r .planning.session <<<"$t_out")" "s-1"
  # The kept directory leaves state/ for the place beside the workspaces.
  t_eq "$(cat "$th/work/widget-plan-8.keep/plan-sessions/one" 2>/dev/null)" "kept"
  t_eq "$([ -e "$th/state/plan-8.json.keep" ] || echo gone)" "gone"
  # Again: nothing is left to import, and nothing is said about it.
  t_run "$install"
  t_eq "$t_rc" 0
  t_lacks "$t_out" "store: imported"
fi

if real_store "a cycle that runs while old records are there stops the run before the first copy"; then
  installed
  old_records
  hold "$th/state/cycles/9.lock"
  rm -f "$th/bin/tehas"
  t_run "$install"
  unhold
  t_eq "$t_rc" 1
  t_contains "$t_out" "tehas: a cycle or planning turn runs (#9); run tehas drain --wait and let it end, then install again; nothing was installed"
  t_eq "$([ -e "$th/bin/tehas" ] || echo gone)" "gone"
  t_eq "$([ -f "$th/state/cycles/7.taken" ] && echo there)" "there"
  t_eq "$([ -e "$th/state/imported-$today" ] || echo none)" "none"
  # A planning turn's lock is a run too.
  hold "$th/state/plan-8.json.lock"
  t_run "$install"
  unhold
  t_eq "$t_rc" 1
  t_contains "$t_out" "runs (#8)"
  t_eq "$([ -e "$th/bin/tehas" ] || echo gone)" "gone"
  # Once it has ended, the run goes on.
  t_run "$install"
  t_eq "$t_rc" 0
  t_eq "$([ -e "$th/bin/tehas" ] && echo there)" "there"
fi

if real_store "a home from before the marker has its old records judged before the copies"; then
  installed
  old_records
  # A host installed before the marker came in: env.sh and libexec/dispatch prove it.
  rm -f "$th/.tehas-home" "$th/bin/tehas"
  hold "$th/state/plan-8.json.lock"
  t_run "$install"
  unhold
  t_eq "$t_rc" 1
  t_contains "$t_out" "runs (#8)"
  t_contains "$t_out" "nothing was installed"
  t_eq "$([ -e "$th/bin/tehas" ] || echo gone)" "gone"
  t_eq "$([ -f "$th/state/plan-8.json" ] && echo there)" "there"
fi

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

if real_store "a file the import would hold stops the run before the first copy"; then
  installed
  old_records
  echo alice >"$T/elsewhere"
  rm -f "$th/state/cycles/7.by"
  ln -s "$T/elsewhere" "$th/state/cycles/7.by"
  rm -f "$th/bin/tehas"
  t_run "$install"
  t_eq "$t_rc" 1
  t_contains "$t_out" "store: held"
  t_contains "$t_out" "state/cycles/7.by: is not a regular file"
  t_contains "$t_out" "the old records listed above cannot be imported"
  t_contains "$t_out" "nothing was installed"
  t_eq "$([ -e "$th/bin/tehas" ] || echo gone)" "gone"
  t_eq "$([ -L "$th/state/cycles/7.by" ] && echo there)" "there"
  t_eq "$(cat "$T/elsewhere")" "alice"
fi

if real_store "a poll during the install finds the dispatcher's lock held"; then
  # The store this run installs reports, each time it is asked to import,
  # whether a poll could take the lock then.
  cat >"$T/store-watch" <<EOF
#!/usr/bin/env bash
if [ "\$1" = import ]; then
  if flock -n "\$TEHAS_HOME/state/dispatch.lock" true; then echo free; else echo held; fi >>"\$T/calls/poll"
fi
exec "$T_STORE" "\$@"
EOF
  chmod +x "$T/store-watch"
  t_env STORE_BIN="$T/store-watch"
  installed
  rm -f "$T/calls/poll"
  old_records
  t_run "$install"
  t_eq "$t_rc" 0
  t_eq "$(t_calls poll | sort -u)" "held"
  t_eq "$(flock -n "$th/state/dispatch.lock" true && echo free)" "free"
fi
t_env

if real_store "an import that fails after the copies stops the run and a second run finishes it"; then
  cat >"$T/store-fails" <<EOF
#!/usr/bin/env bash
if [ "\$*" = import ] && [ -e "\$T/import-fails" ]; then
  echo "store: busy the database: a stand-in" >&2
  exit 1
fi
exec "$T_STORE" "\$@"
EOF
  chmod +x "$T/store-fails"
  t_env STORE_BIN="$T/store-fails"
  installed
  old_records
  : >"$T/import-fails"
  t_run "$install"
  t_eq "$t_rc" 1
  t_contains "$t_out" "store: busy the database: a stand-in"
  t_contains "$t_out" "tehas: the store could not import the old records under $th/state; the files are installed, the database holds what was imported, and install.sh can run again"
  t_eq "$([ -f "$th/state/cycles/7.taken" ] && echo there)" "there"
  rm -f "$T/import-fails"
  t_run "$install"
  t_eq "$t_rc" 0
  t_contains "$t_out" "store: imported cycles 1"
fi
t_env

t_done
