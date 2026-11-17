#!/usr/bin/env bash
# Tests for host/sandbox/vipune-shim: vipune in a cycle's container, which
# sends each call to the factory's memory through the relay directory when the
# host marks it, and runs the image's own vipune otherwise. The real vipune is
# a stub named by TEHAS_VIPUNE_BIN that prints its arguments; a fake host in
# the background answers the first request it finds, as
# console/cycle/src/relay.rs does, and keeps what it was asked.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

shim="$T_ROOT/host/sandbox/vipune-shim"
relay=$T/relay
mkdir -p "$T/bin"
cat >"$T/bin/vipune" <<'EOF'
#!/bin/sh
printf 'local:%s\n' "$*"
exit 5
EOF
chmod +x "$T/bin/vipune"

fake_host() { # <rc> [local]
  (
    for _ in $(seq 100); do
      for r in "$relay"/req.*/ready; do
        [ -e "$r" ] || continue
        d=${r%/ready}
        pid=${d##*/req.}
        cp "$d/args" "$T/asked.args"
        cp "$d/kind" "$T/asked.kind"
        cp "$d/env" "$T/asked.env"
        rm -rf "$d"
        mkdir "$relay/res.$pid"
        if [ -n "${2:-}" ]; then
          : >"$relay/res.$pid/local"
        else
          echo '{"memories":[]}' >"$relay/res.$pid/out"
          echo noise >"$relay/res.$pid/err"
          echo "$1" >"$relay/res.$pid/rc"
        fi
        : >"$relay/res.$pid/done"
        exit 0
      done
      sleep 0.1
    done
  ) &
}
reset() { # [marked]
  rm -rf "$relay" "$T"/asked.*
  mkdir -p "$relay"
  if [ -n "${1:-}" ]; then : >"$relay/vipune"; fi
}
run_shim() { # <args...>
  T_ENV=(TEHAS_VIPUNE_BIN="$T/bin/vipune" TEHAS_RELAY_DIR="$relay" TEHAS_VIPUNE_WAIT="${wait:-5}" "${vars[@]}")
  t_run "$shim $* >'$T/shim.out' 2>'$T/shim.err'"
  T_ENV=()
}
vars=()

t_case "a call goes to the factory's memory, and its out, err and rc come back"
reset marked
fake_host 3
vars=(VIPUNE_RECENCY_WEIGHT=0.3 VIPUNE_PROJECT=acme/widget)
run_shim "search 'the  cache' --json"
vars=()
wait
t_eq "$t_rc" 3
t_eq "$(cat "$T/shim.out")" '{"memories":[]}'
t_eq "$(cat "$T/shim.err")" "noise"
t_eq "$(cat "$T/asked.kind")" "vipune"
t_eq "$(od -An -c "$T/asked.args" | tr -s ' ')" "$(printf '%s\0' search 'the  cache' --json | od -An -c | tr -s ' ')"
t_eq "$(od -An -c "$T/asked.env" | tr -s ' ')" "$(printf '%s\0' VIPUNE_RECENCY_WEIGHT=0.3 VIPUNE_PROJECT=acme/widget | od -An -c | tr -s ' ')"
t_eq "$(ls -A "$relay")" "vipune"

t_case "a refusal comes back as the factory's usage exit, nothing on stdout"
reset marked
(
  for _ in $(seq 100); do
    for r in "$relay"/req.*/ready; do
      [ -e "$r" ] || continue
      d=${r%/ready}
      pid=${d##*/req.}
      rm -rf "$d"
      mkdir "$relay/res.$pid"
      : >"$relay/res.$pid/out"
      echo "vipune: the factory's memory runs add, search, get, list, validate, version only" >"$relay/res.$pid/err"
      echo 64 >"$relay/res.$pid/rc"
      : >"$relay/res.$pid/done"
      exit 0
    done
    sleep 0.1
  done
) &
run_shim "delete 1"
wait
t_eq "$t_rc" 64
t_eq "$(cat "$T/shim.out")" ""
t_contains "$(cat "$T/shim.err")" "runs add, search"

t_case "with no variables set the request carries none"
reset marked
fake_host 0
run_shim "list"
wait
t_eq "$t_rc" 0
t_eq "$(wc -c <"$T/asked.env" | tr -d ' ')" 0

t_case "the host's memory off: the image's own vipune runs"
reset marked
fake_host 0 local
run_shim "list --json"
wait
t_eq "$t_rc" 5
t_eq "$(cat "$T/shim.out")" "local:list --json"
t_eq "$(ls -A "$relay")" "vipune"

t_case "a relay directory the host did not mark: the image's own vipune runs"
reset
run_shim "add 'a fact'"
t_eq "$t_rc" 5
t_eq "$(cat "$T/shim.out")" "local:add a fact"
t_eq "$(ls -A "$relay")" ""

t_case "no relay directory: the image's own vipune runs"
T_ENV=(TEHAS_VIPUNE_BIN="$T/bin/vipune" TEHAS_RELAY_DIR="$T/no-relay")
t_run "$shim version >'$T/shim.out'"
T_ENV=()
t_eq "$t_rc" 5
t_eq "$(cat "$T/shim.out")" "local:version"

t_case "no answer in time exits 1 with the line, and the request goes"
reset marked
wait=1 run_shim "list"
t_eq "$t_rc" 1
t_eq "$(cat "$T/shim.out")" ""
t_eq "$(cat "$T/shim.err")" "vipune: the factory's memory did not answer"
t_eq "$(ls -A "$relay")" "vipune"

t_done
