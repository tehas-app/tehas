#!/usr/bin/env bash
# Tests for tehas state: what a stopped factory's state moves as, and when
# pack and unpack refuse. The tar is real, the store is the real store of this
# checkout, systemctl is a stub that lists the units in $T/units, and no case
# touches the factory home of the machine it runs on.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

state="$T_ROOT/bin/tehas state"
src=$T/src/tehas
dst=$T/home/tehas
tar_file=$T/state.tar
t_stub gh
t_stub git
t_stub systemctl '[ ! -f "$T/units" ] || cat "$T/units"'
t_console || t_bad "no console: $t_console_why"
T_ENV_ALWAYS=(TEHAS_CONSOLE="$T_CONSOLE")

# make_home <home> <tag>: a factory with the store, the release it runs and a
# file in each directory that moves and in each that does not.
make_home() {
  rm -rf "$1"
  t_store_home "$1"
  mkdir -p "$1/docs" "$1/state/cycles" "$1/repos/product" "$1/live" "$1/mcp" "$1/logs" "$1/work/w" "$T/home/.config/tehas"
  echo "$2" >"$1/docs/version.txt"
  echo "ledger" >"$1/state/ledger.txt"
  echo "code" >"$1/repos/product/main.c"
  echo '{}' >"$1/live/a.json"
  echo "key" >"$1/mcp/sessions"
  echo "log" >"$1/logs/work.log"
  echo "tree" >"$1/work/w/f"
  echo "secret" >"$T/home/.config/tehas/env"
}
reset() {
  rm -rf "$T"/calls/* "$T/held" "$T/src" "$T/units" "$T/x" "$tar_file"
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
  make_home "$dst" v0.17.0
  make_home "$src" v0.17.0
}
# pack <args>: tehas state pack from the source factory into $tar_file.
pack() {
  T_ENV=(TEHAS_HOME="$src")
  t_run "$state pack $* - >'$tar_file'"
  T_ENV=()
}
unpack() { t_run "$state unpack - <'$1'"; }
names() { tar -tf "$1" | sed 's|/$||' | LC_ALL=C sort; }
# fingerprint <home>: the names and bytes of a home, but for the files SQLite
# makes and removes as it is read.
fingerprint() {
  (cd "$1" && find . ! -name '*-wal' ! -name '*-shm' | LC_ALL=C sort && find . -type f ! -name '*-wal' ! -name '*-shm' -print0 | LC_ALL=C sort -z | xargs -0 sha256sum)
}
# whole: the target factory and what sits beside it.
whole() { fingerprint "$T/home"; }
# retar <manifest json>: $tar_file with its manifest replaced, others kept.
retar() {
  rm -rf "$T/x" && mkdir -p "$T/x"
  tar -xf "$tar_file" -C "$T/x"
  printf '%s\n' "$1" >"$T/x/tehas-state.json"
  tar -cf "$T/bad.tar" -C "$T/x" tehas-state.json state repos live mcp
}

t_case "pack refuses a running cycle: exit 1, the issue, nothing on stdout"
reset
t_live "$src" 12 true running work
pack
t_eq "$t_rc" 1
t_contains "$t_out" "#12"
t_eq "$(wc -c <"$tar_file")" 0

t_case "pack refuses a planning turn: exit 1, the issue, nothing on stdout"
reset
t_plan "$src" 13 '{"turns":1,"running":true}'
pack
t_eq "$t_rc" 1
t_contains "$t_out" "#13"
t_eq "$(wc -c <"$tar_file")" 0

t_case "pack refuses an active unit: exit 1, the unit, nothing on stdout"
reset
echo "tehas-poll.timer loaded active waiting Poll" >"$T/units"
pack
t_eq "$t_rc" 1
t_contains "$t_out" "tehas-poll.timer"
t_eq "$(wc -c <"$tar_file")" 0

t_case "rehearse packs a running factory and changes none of its files"
reset
t_live "$src" 12 true running work
echo "tehas-poll.timer loaded active waiting Poll" >"$T/units"
before=$(fingerprint "$src")
pack --rehearse
t_eq "$t_rc" 0
t_eq "$(fingerprint "$src")" "$before"
t_eq "$(tar -tf "$tar_file" | head -n 1)" tehas-state.json

t_case "pack holds the listed paths and not the logs, the work or the settings"
reset
pack
t_eq "$t_rc" 0
t_eq "$(tar -tf "$tar_file" | head -n 1)" tehas-state.json
t_eq "$(tar -xOf "$tar_file" tehas-state.json)" '{"api":1,"schema":9,"tag":"v0.17.0"}'
list=$(names "$tar_file")
t_contains "$list" "state/ledger.txt"
t_contains "$list" "repos/product/main.c"
t_contains "$list" "live/a.json"
t_contains "$list" "mcp/sessions"
t_contains "$list" "state/tehas.db"
t_lacks "$list" "logs"
t_lacks "$list" "work"
t_lacks "$list" ".config"
t_lacks "$list" "tehas.db-wal"
t_lacks "$list" "tehas.db-shm"

t_case "pack takes the store from a copy the store made, one that reads as the store"
reset
t_ended "$src" 5 -
pack
t_eq "$t_rc" 0
rm -rf "$T/x" && mkdir -p "$T/x/state" "$T/x/libexec"
tar -xf "$tar_file" -C "$T/x" state
cp "$T_STORE" "$T/x/libexec/store"
t_eq "$(TEHAS_HOME=$T/x "$T/x/libexec/store" check | jq -r '.integrity')" ok
t_eq "$(TEHAS_HOME=$T/x "$T/x/libexec/store" check | jq -r '.rows.cycles')" 1

t_case "unpack refuses an active unit and writes nothing"
reset
pack
echo "tehas-dispatch.service loaded active running Dispatch" >"$T/units"
before=$(whole)
unpack "$tar_file"
t_eq "$t_rc" 1
t_contains "$t_out" "tehas-dispatch.service"
t_eq "$(whole)" "$before"

t_case "unpack refuses another release, names both and writes nothing"
reset
pack
echo v0.16.0 >"$dst/docs/version.txt"
before=$(whole)
unpack "$tar_file"
t_eq "$t_rc" 1
t_contains "$t_out" "v0.17.0"
t_contains "$t_out" "v0.16.0"
t_eq "$(whole)" "$before"

t_case "unpack refuses another schema, names both and writes nothing"
reset
pack
retar '{"api":1,"tag":"v0.17.0","schema":8}'
before=$(whole)
unpack "$T/bad.tar"
t_eq "$t_rc" 1
t_contains "$t_out" "schema 8"
t_contains "$t_out" "schema 9"
t_eq "$(whole)" "$before"

t_case "unpack refuses a manifest that is missing, not first or of an unknown api"
reset
pack
before=$(whole)
retar '{"api":2,"tag":"v0.17.0","schema":9}'
unpack "$T/bad.tar"
t_eq "$t_rc" 1
t_contains "$t_out" manifest
rm -rf "$T/x" && mkdir -p "$T/x" && tar -xf "$tar_file" -C "$T/x"
tar -cf "$T/bad.tar" -C "$T/x" state repos live mcp
unpack "$T/bad.tar"
t_eq "$t_rc" 1
tar -cf "$T/bad.tar" -C "$T/x" state tehas-state.json repos live mcp
unpack "$T/bad.tar"
t_eq "$t_rc" 1
t_contains "$t_out" "not the first entry"
t_eq "$(whole)" "$before"

t_case "unpack refuses a path outside the home: .. and absolute"
reset
pack
rm -rf "$T/x" && mkdir -p "$T/x" && tar -xf "$tar_file" -C "$T/x"
echo evil >"$T/x/evil"
before=$(whole)
tar -cf "$T/bad.tar" -C "$T/x" tehas-state.json --transform 's|^evil$|../evil|' evil state
unpack "$T/bad.tar"
t_eq "$t_rc" 1
t_contains "$t_out" "archive"
tar -cPf "$T/bad.tar" -C "$T/x" tehas-state.json --transform "s|^evil\$|$T/abs|" evil state
unpack "$T/bad.tar"
t_eq "$t_rc" 1
t_contains "$t_out" "outside the state"
t_eq "$(whole)" "$before"
t_eq "$([ -e "$T/evil" ] || [ -e "$T/abs" ] && echo written || echo none)" none

t_case "unpack replaces the state, checks the store and starts no unit"
reset
t_ended "$src" 5 -
echo "only on the source" >"$src/state/new.txt"
echo "only on the target" >"$dst/state/old.txt"
echo "stale" >"$dst/live/stale.json"
pack
t_eq "$t_rc" 0
rm -f "$T/calls/systemctl"
unpack "$tar_file"
t_eq "$t_rc" 0
t_eq "$([ -e "$dst/state/old.txt" ] || [ -e "$dst/live/stale.json" ] && echo kept || echo gone)" gone
t_eq "$(cat "$dst/state/new.txt")" "only on the source"
t_eq "$(cat "$dst/repos/product/main.c")" code
t_eq "$(TEHAS_HOME=$dst "$dst/libexec/store" check | jq -r '.integrity')" ok
t_eq "$(TEHAS_HOME=$dst "$dst/libexec/store" check | jq -r '.rows.cycles')" 1
t_eq "$(cat "$dst/logs/work.log")" log
t_eq "$(compgen -G "$T/home/.tehas-incoming-*" || echo none)" none
t_lacks "$(t_calls systemctl 2>/dev/null)" "start"

t_case "unpack refuses a rehearsal and writes nothing"
reset
pack --rehearse
t_eq "$(tar -xOf "$tar_file" tehas-state.json | jq -c .rehearsal)" true
before=$(whole)
unpack "$tar_file"
t_eq "$t_rc" 1
t_contains "$t_out" rehearsal
t_eq "$(whole)" "$before"

t_case "unpack --rehearse accepts a rehearsal, and an unmarked archive as a normal unpack"
reset
echo "only on the source" >"$src/state/new.txt"
pack --rehearse
t_run "$state unpack --rehearse - <'$tar_file'"
t_eq "$t_rc" 0
t_eq "$(cat "$dst/state/new.txt")" "only on the source"
reset
echo "only on the source" >"$src/state/new.txt"
pack
t_run "$state unpack --rehearse - <'$tar_file'"
t_eq "$t_rc" 0
t_eq "$(cat "$dst/state/new.txt")" "only on the source"

# staged: a tar of the source's pack with $T/x as the tree to edit.
staged() {
  reset
  pack
  rm -rf "$T/x" && mkdir -p "$T/x" && tar -xf "$tar_file" -C "$T/x"
}
rebuilt() { tar -cf "$T/bad.tar" -C "$T/x" tehas-state.json state repos live mcp; }

t_case "pack leaves out the live token"
reset
mkdir -p "$src/state/github"
echo "ghs_live" >"$src/state/github/token"
pack
t_eq "$t_rc" 0
t_lacks "$(names "$tar_file")" "github"
t_contains "$(t_calls systemctl)" "--state=active,activating,deactivating,reloading"

t_case "unpack drops a token the archive holds and keeps the target's own"
staged
mkdir -p "$T/x/state/github"
echo "theirs" >"$T/x/state/github/token"
rebuilt
mkdir -p "$dst/state/github"
echo "mine" >"$dst/state/github/token"
unpack "$T/bad.tar"
t_eq "$t_rc" 0
t_eq "$(cat "$dst/state/github/token")" mine

t_case "unpack refuses a link at a top-level name"
staged
rm -rf "$T/x/live"
ln -s "$T/outside" "$T/x/live"
rebuilt
before=$(whole)
unpack "$T/bad.tar"
t_eq "$t_rc" 1
t_contains "$t_out" "outside the state"
t_eq "$(whole)" "$before"

t_case "unpack refuses a link with an absolute target and one that leaves its directory"
staged
ln -s /etc/passwd "$T/x/repos/product/abs"
rebuilt
before=$(whole)
unpack "$T/bad.tar"
t_eq "$t_rc" 1
t_contains "$t_out" "absolute"
rm "$T/x/repos/product/abs"
ln -s ../../state/tehas.db "$T/x/repos/product/esc"
rebuilt
unpack "$T/bad.tar"
t_eq "$t_rc" 1
t_contains "$t_out" "leaves its directory"
t_eq "$(whole)" "$before"

t_case "unpack writes nothing through a link of the archive"
staged
mkdir -p "$T/outside" "$T/y"
echo pwn >"$T/y/pwn"
ln -s "$T/outside" "$T/x/repos/l"
rebuilt
tar -rf "$T/bad.tar" -C "$T/y" --transform 's|^pwn$|repos/l/pwn|' pwn
before=$(whole)
unpack "$T/bad.tar"
t_eq "$t_rc" 1
t_eq "$([ -e "$T/outside/pwn" ] && echo written || echo none)" none
t_eq "$(whole)" "$before"

t_case "unpack keeps relative links that stay in their directory, as a git checkout has"
staged
ln -s main.c "$T/x/repos/product/ok"
mkdir -p "$T/x/repos/other"
ln -s ../product/main.c "$T/x/repos/other/ok2"
rebuilt
unpack "$T/bad.tar"
t_eq "$t_rc" 0
t_eq "$(readlink "$dst/repos/product/ok")" main.c
t_eq "$(cat "$dst/repos/other/ok2")" code

t_case "unpack refuses a tar with no store"
staged
rm "$T/x/state/tehas.db"
rebuilt
before=$(whole)
unpack "$T/bad.tar"
t_eq "$t_rc" 1
t_contains "$t_out" "no state store"
t_eq "$(whole)" "$before"

t_case "a failed swap puts the old state back and leaves nothing beside the home"
if [ "$(id -u)" = 0 ]; then
  t_skip "root can move a directory it may not write"
else
  reset
  pack
  echo "mine" >"$dst/state/old.txt"
  chmod a-w "$dst/live"
  before=$(whole)
  unpack "$tar_file"
  chmod u+w "$dst/live"
  t_eq "$t_rc" 1
  t_contains "$t_out" "could not move live aside"
  t_eq "$(whole)" "$before"
  t_eq "$(cat "$dst/state/old.txt")" mine
  t_eq "$(TEHAS_HOME=$dst "$dst/libexec/store" check | jq -r '.integrity')" ok
  t_eq "$(compgen -G "$T/home/.tehas-incoming-*" || echo none)" none
fi

t_case "rehearse goes on past a file it cannot read, and pack stops at it"
if [ "$(id -u)" = 0 ]; then
  t_skip "root reads every file"
else
  reset
  chmod 000 "$src/repos/product/main.c"
  pack
  t_eq "$t_rc" 1
  pack --rehearse
  t_eq "$t_rc" 0
  t_eq "$(tar -tf "$tar_file" | head -n 1)" tehas-state.json
  chmod 644 "$src/repos/product/main.c"
fi

t_case "pack refuses when it cannot tell whether a cycle runs"
reset
rm -rf "$src/state/cycles"
echo "not a directory" >"$src/state/cycles"
pack
t_eq "$t_rc" 1
t_eq "$(wc -c <"$tar_file")" 0

t_case "pack holds a row that is still in the write-ahead log"
if ! command -v sqlite3 >/dev/null 2>&1; then
  t_skip "no sqlite3 to hold the log open (the store's own copy test covers it)"
else
  reset
  mkfifo "$T/hold"
  sqlite3 "$src/state/tehas.db" <"$T/hold" >/dev/null &
  holder=$!
  exec 7>"$T/hold"
  printf '%s\n' "BEGIN;" "SELECT count(*) FROM cycles;" ".shell touch '$T/held'" >&7
  for _ in $(seq 50); do
    [ -e "$T/held" ] && break
    read -r -t 0.1 <> <(:) || true
  done
  t_eq "$([ -e "$T/held" ] && echo holding || echo not)" holding
  t_ended "$src" 5 -
  t_eq "$([ -s "$src/state/tehas.db-wal" ] && echo live || echo folded)" live
  pack
  t_eq "$t_rc" 0
  exec 7>&-
  kill "$holder" 2>/dev/null
  wait "$holder" 2>/dev/null
  t_lacks "$(names "$tar_file")" "tehas.db-wal"
  rm -rf "$T/x" && mkdir -p "$T/x/state" "$T/x/libexec"
  tar -xf "$tar_file" -C "$T/x" state
  cp "$T_STORE" "$T/x/libexec/store"
  t_eq "$(TEHAS_HOME=$T/x "$T/x/libexec/store" check | jq -r '.rows.cycles')" 1
fi

t_case "SIGTERM while the archive is still coming removes the directory made beside the home"
reset
mkfifo "$T/in"
env -i PATH="$T/bin:$PATH" HOME="$T/home" TEHAS_HOME="$dst" T="$T" TEHAS_INSTANCE_DIR="$TEHAS_INSTANCE_DIR" \
  TEHAS_CONSOLE="$T_CONSOLE" $state unpack - <"$T/in" >/dev/null 2>&1 &
unpacker=$!
exec 8>"$T/in"
for _ in $(seq 50); do
  compgen -G "$T/home/.tehas-incoming-*" >/dev/null && break
  read -r -t 0.1 <> <(:) || true
done
t_eq "$(compgen -G "$T/home/.tehas-incoming-*" >/dev/null && echo there || echo none)" there
kill -TERM "$unpacker"
wait "$unpacker"
t_eq "$?" 143
exec 8>&-
t_eq "$(compgen -G "$T/home/.tehas-incoming-*" || echo none)" none

t_case "unpack refuses a chain of links that climbs out of its directory"
staged
ln -s .. "$T/x/repos/product/up"
ln -s up/../.. "$T/x/repos/product/esc"
rebuilt
before=$(whole)
unpack "$T/bad.tar"
t_eq "$t_rc" 1
t_contains "$t_out" ".. after a name"
t_eq "$(whole)" "$before"

t_case "unpack refuses a setuid, setgid or world-writable entry"
for mode in 4755 2755 666 777; do
  staged
  echo x >"$T/x/repos/product/odd"
  chmod "$mode" "$T/x/repos/product/odd"
  rebuilt
  before=$(whole)
  unpack "$T/bad.tar"
  t_eq "$t_rc" 1
  t_contains "$t_out" "setuid, setgid or world-writable"
  t_eq "$(whole)" "$before"
done

t_case "a pack that fails in a pipe sends nothing, and the pipe leaves the target as it was"
if [ "$(id -u)" = 0 ]; then
  t_skip "root reads every file"
else
  reset
  chmod 000 "$src/repos/product/main.c"
  echo "only on the source" >"$src/state/new.txt"
  before=$(whole)
  t_run "set -o pipefail; TEHAS_HOME='$src' '$state' pack - | '$state' unpack -"
  chmod 644 "$src/repos/product/main.c"
  t_eq "$([ "$t_rc" != 0 ] && echo failed || echo passed)" failed
  t_eq "$(whole)" "$before"
  t_eq "$([ -e "$dst/state/new.txt" ] && echo replaced || echo kept)" kept
fi

t_case "a bad argument is 64, missing settings are 78, and neither starts a tar"
reset
t_run "$state pack"
t_eq "$t_rc" 64
t_run "$state unpack --bogus -"
t_eq "$t_rc" 64
rm -rf "$T/instance"
unset TEHAS_INSTANCE_DIR
t_run "$state pack - >'$tar_file'"
t_eq "$t_rc" 78
t_eq "$(wc -c <"$tar_file")" 0

t_done
