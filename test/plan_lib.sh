# shellcheck shell=bash
# What the tests of tehas plan share: a stub docker that answers as pi-rukas's
# headless runner does, so no model runs. Source this after test/lib.sh.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck disable=SC2034  # the variables here are read by the test files
# shellcheck disable=SC2154  # t_rc and t_console_why are set by test/lib.sh
# The variables a work cycle's container gets, by name.
cycle_vars="GH_TOKEN
GIT_CONFIG_COUNT
GIT_CONFIG_KEY_0
GIT_CONFIG_KEY_1
GIT_CONFIG_KEY_2
GIT_CONFIG_KEY_3
GIT_CONFIG_VALUE_0
GIT_CONFIG_VALUE_1
GIT_CONFIG_VALUE_2
GIT_CONFIG_VALUE_3
GIT_TERMINAL_PROMPT
CBM_CACHE_DIR
PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS
PI_ENSEMBLE_MAX_WORKSTREAMS
PI_ENSEMBLE_SANDBOX_MODE
PI_ENSEMBLE_SUBAGENT_MODEL
PI_ENSEMBLE_SUBAGENT_PROVIDER
PI_ENSEMBLE_VERIFY_SUITE_TIMEOUT_MS
PI_ENSEMBLE_VERIFY_TIMEOUT_MS
PI_ENSEMBLE_WORKSPACE_ROOT"
# The CPU quota and the ticket ceiling follow the host's CPUs: the same on
# every test machine.
t_stub nproc 'echo 16'

# The command as a host has it: beside the console, which shapes what a turn
# says (console/src/plan.rs) and is a compiled program a checkout does not hold.
t_console || t_bad "no console: $t_console_why"
shim=$T/tree
t_front "$shim"
# shellcheck disable=SC2154  # t_cycle_why is set by t_cycle (test/lib.sh)
t_cycle || t_bad "no tehas-cycle: $t_cycle_why"
cp "$T_CYCLE" "$shim/libexec/tehas-cycle"
plan="$shim/bin/tehas plan"
fx=$T/fx
mkdir -p "$fx"
checkout=$T/home/tehas/repos/widget
ws=$T/home/tehas/work/widget
# The directory on the host behind it, for issue 7, and the turn's own state.
tree=$ws-plan-7
state=$tree.state
upstream=$T/git/acme/widget
mkdir -p "$upstream"
git init -q -b main "$upstream"
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m first
git clone -q "$upstream" "$checkout"
# One case runs tehas work, which parks a product with no gate. The gate is
# read at the clone's commit, so it is committed here.
mkdir -p "$upstream/.pi"
printf 'scripts/verify.sh\n' >"$upstream/.pi/verify-cmd"
git -C "$upstream" add -f -- .pi/verify-cmd
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q -m gate
git -C "$checkout" fetch -q "$upstream" +refs/heads/main:refs/remotes/origin/main
git -C "$checkout" checkout -q main


# The workspace goes with the turn, so the container notes what it was handed.
# The runner answers $T/fx/doc.json, exits $T/fx/doc.rc and keeps $T/fx/keeps.
runner=/opt/pi-rukas/bin/pi-rukas-headless
t_stub docker 'if [ "$1" = info ]; then cat "$T/docker.info"; exit; fi
# tehas plan and tehas work ask the image what it supports.
case " $* " in *" --capabilities "*) cat "$T/caps.json" 2>/dev/null; exit ;; esac
printf "%s\n" "$@" >"$T/calls/docker.args.$$"
printf "%s\n" "${GH_TOKEN:-}" >"$T/calls/docker.token.$$"
case " $* " in
  *" stop "*) : >"$T/calls/stopped" ;;
  *" /opt/pi-rukas/bin/pi-rukas-headless plan "*)
  ws=$T/home/tehas/work/widget-plan-7
  cat "$ws/.tehas/brief.md" >"$T/calls/brief.seen" 2>/dev/null
  cat "$ws/.tehas/answer.md" >"$T/calls/answer.seen" 2>/dev/null
  git -C "$ws" status --porcelain >"$T/calls/tree.status" 2>&1
  cp "$ws/.git/info/exclude" "$T/calls/exclude.seen" 2>/dev/null
  mkdir -p "$ws/.tehas/definition" && echo "{}" >"$ws/.tehas/definition/app.json"
  git -C "$ws" status --porcelain -uall >"$T/calls/tree.status.definition" 2>&1
  git -C "$ws" rev-parse HEAD >"$T/calls/head.seen" 2>&1
  cat "$ws.images/manifest.json" >"$T/calls/manifest.seen" 2>/dev/null
  (cd "$ws/.git/pi-plan-sessions" 2>/dev/null && find . -type f | LC_ALL=C sort) >"$T/calls/sessions.given"
  cat "$ws/.git/pi-plan-sessions/abc-123/session.json" >"$T/calls/sessions.first" 2>/dev/null
  [ ! -e "$T/fx/keeps" ] || . "$T/fx/keeps"
  [ ! -e "$T/fx/live" ] || . "$T/fx/live"
  cat "$T/fx/doc.json" 2>/dev/null
  exit "$(cat "$T/fx/doc.rc" 2>/dev/null || echo 11)" ;;
  *" image inspect "*) cat "$T/image.label" 2>/dev/null ;;
esac'
# The command looks at the container once a second. Here it looks every 0.05s:
# a sleep that returned at once would spin until the stubbed run ends.
t_stub sleep 'exec "$(command -pv sleep)" 0.05'
printf '[user]\n\tname = Test Bot\n\temail = bot@example.com\n' >"$T/home/.gitconfig"
# Pi's settings as install.sh leaves them on a host.
mkdir -p "$T/home/.pi/agent"
echo '{"retry":{"provider":{"timeoutMs":600000,"maxRetries":8,"maxRetryDelayMs":60000}}}' >"$T/home/.pi/agent/settings.json"
echo '{"providers":{}}' >"$T/home/.pi/agent/models.json"
# A host with the root step done: an 8 GiB slice (a 2048m container ceiling),
# Docker on systemd and cgroup v2.
t_stub systemctl 'cat "$T/slice.show"'
slice_good() {
  printf '%s\n' FragmentPath=/etc/systemd/system/tehas.slice ActiveState=active MemoryMax=8589934592 >"$T/slice.show"
  echo "systemd 2" >"$T/docker.info"
  slice_state 0.00
}
limits="--cgroup-parent --memory --memory-swap --memory-reservation --cpus --pids-limit --cpu-shares"
# test/docker_args.sh holds the security options as a literal, so every
# container is pinned against one list. docker_opts is unknown_options's
# allow-list: the slice's options and the security ones.
# shellcheck source=test/docker_args.sh
. "$(dirname "$0")/docker_args.sh" || exit 1
docker_opts="$limits --cap-drop --cap-add --security-opt --tmpfs"

turn_args() { grep -l -x -e /opt/pi-rukas/bin/pi-rukas-headless "$T"/calls/docker.args.* 2>/dev/null | head -n 1 | xargs -r cat; }
after() { turn_args | awk -v flag="$1" 'prev == flag { print } { prev = $0 }'; }
image="tehas/pi-rukas:uid$(id -u)"
tail_of() { turn_args | sed -n "/^$(printf '%s' "$image" | sed 's/[\/.]/\\&/g')\$/,\$p" | tr '\n' ' '; }
# What docker is given before the image, one argument per line, without the
# container's name and without the cid file, which only a cycle has.
options() {
  awk -v image="$image" '$0 == image { exit } $0 == "--cidfile" { skip = 1; next } skip { skip = 0; next }
    prev == "--name" { $0 = "<name>" } { print; prev = $0 }'
}


# What the runner answers. turn_doc <exit status> <document>.
turn_doc() { printf '%s\n' "$2" >"$fx/doc.json"; printf '%s\n' "$1" >"$fx/doc.rc"; }
# A turn that asked one question, as pi-rukas numbers them.
asked() {
  turn_doc 11 '{"subcommand":"plan","mode":"turn","exitCode":11,"status":"question","session":{"id":"abc-123","turn":1},
    "text":"The plan needs answers before it can be drafted (1 question).",
    "questions":[{"id":"q1","question":"Which store, SQLite or files?","options":[],"resolution":"SQLite"}]}'
}
# A turn that drafted a plan.
shown() {
  turn_doc 12 '{"subcommand":"plan","mode":"turn","exitCode":12,"status":"shown","session":{"id":"abc-123","turn":2},
    "text":"Drafted plan: Faster search. It has 1 open point: answer them, ask for changes, or confirm to file it as it is.",
    "title":"Faster search","spec":"## Context\n\nSearch is slow.\n\n## Acceptance criteria\n\n- p95 under 100 ms","reviewedDigest":"d1",
    "questions":[{"id":"q1","question":"Index on the title too?","options":[],"resolution":"Yes, both"}]}'
}
# What the runner keeps of the session in the clone's git directory, as it
# would leave it: runner_keeps <the text of its record>.
runner_keeps() {
  cat >"$fx/keeps" <<EOF
mkdir -p "\$ws/.git/pi-plan-sessions/abc-123"
printf '%s\n' '$1' >"\$ws/.git/pi-plan-sessions/abc-123/session.json"
EOF
}
caps_plan() { jq -n '{contract: "0.14.0", features: {planTurns: true}}' >"$T/caps.json"; }
# An issue's attachments are read by the real attachment checker.
t_attach_home "$T/home/tehas"
reset() {
  rm -rf "$T"/calls/* "$T/out.json" "$fx/live" "$fx/keeps" "$T/keep"
  asked
  caps_plan
  t_instance "${T_MINIMAL[@]}"
  T_ENV=(GH_TOKEN=test-token "TEHAS_CGROUP_ROOT=$T_CGROUP")
  slice_good
  printf '# Faster search\n\nMake it fast.\n' >"$T/brief.md"
}
# The approved text as a turn is given it: the brief, then the issue brief.
with_brief() { cat "$1"; printf '\n\n---\n\n'; cat "$T_ROOT/lib/issue-brief.md"; }
# A session whose first turn ran and kept its record.
first_turn_ran() {
  reset
  runner_keeps '{"turn":1}'
  t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
  rm -f "$fx/keeps" "$T"/calls/*
}

for bad in "" "7" "x --out $T/out.json --keep $T/keep --brief $T/brief.md" "7 --out $T/out.json --keep $T/keep" \
  "7 --out $T/out.json --brief $T/brief.md" "7 --keep $T/keep --brief $T/brief.md" \
  "7 --out $T/out.json --keep $T/keep --brief $T/brief.md --session abc" "7 --out $T/out.json --keep $T/keep --session abc" \
  "7 --out $T/out.json --keep $T/keep --brief $T/missing.md" "7 --out $T/out.json --keep $T/keep --session 'a b;c' --answer $T/brief.md" \
  "7 --out $T/out.json --keep $T/keep --brief $T/brief.md --verbose" "7 --out $T/out.json --keep $T/keep --brief $T/brief.md --kind plan"; do
  t_case "a call that is not a turn ('$bad') is refused before anything runs"
  reset
  t_run "$plan $bad"
  t_eq "$t_rc" 64
  t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
done
