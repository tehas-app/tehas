#!/usr/bin/env bash
# Every docker run tehas makes for a container of its own: the whole argument
# list, in order, against a golden file in test/fixtures/argv. One file per
# call, <case>.<n>, one argument per line. These files are what a port of a
# command to Rust must keep, so a port does not change them.
#
# What differs from run to run is made one word: the scratch directory, the uid
# and gid, a commit, a mktemp suffix, a run's time stamp and the cycle's
# deadline. The -e PI_ENSEMBLE_* block comes from
# the order of the environment, which bash decides, so that block is sorted.
# Everything else stands in the order it was given. Other docker calls (info,
# image inspect, rm, stop) start no container and are not recorded.
#
# To record the files again, after a change that means to change them, and read
# the diff before committing it:
#
#   TEHAS_TEST_ARGV_RECORD=1 bash test/sandbox_argv_test.sh
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/work_lib.sh
. "$(dirname "$0")/work_lib.sh" || exit 1

goldens=$T_ROOT/test/fixtures/argv

# The product of every command here. On top of work_lib's gate: what the
# contract asks for, and the files that make --sandbox run all it can.
mkdir -p "$upstream/.pi"
echo rules >"$upstream/AGENTS.md"
echo 'make full' >"$upstream/.pi/verify-cmd-full"
echo 'make smoke' >"$upstream/.pi/smoke-cmd"
echo true >"$upstream/.pi/worktree-setup"
git -C "$upstream" add -f -- AGENTS.md .pi
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q -m product
git -C "$checkout" fetch -q "$upstream" +refs/heads/main:refs/remotes/origin/main
git -C "$checkout" checkout -q -B main origin/main
git -C "$checkout" symbolic-ref refs/remotes/origin/HEAD refs/remotes/origin/main

# Every command but tehas work runs beside the real console, as a host has it.
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_console || t_bad "no console: $t_console_why"
shim=$T/shim
mkdir -p "$shim/libexec"
ln -s "$T_ROOT/lib" "$shim/lib"
for c in plan review index dev check-product improve; do
  ln -s "$T_ROOT/libexec/$c" "$shim/libexec/$c"
done
cp "$T_CONSOLE" "$shim/libexec/console"
# shellcheck disable=SC2154  # t_cycle_why is set by t_cycle (test/lib.sh)
t_cycle || t_bad "no tehas-cycle: $t_cycle_why"
cp "$T_CYCLE" "$shim/libexec/tehas-cycle"

# What an image answers that can do all a command here asks of it.
jq '. + {contract: "0.14.0"}
  | .features.planTurns = true
  | .verbs.plan.flags = ["--start", "--context", "--resume", "--answer", "--confirm", "--issue"]
  | .verbs["lens-review"].flags = ["--base", "--head", "--since", "--full", "--context", "--pr", "--json"]' \
  "$T/caps.json" >"$T/caps.new" && mv "$T/caps.new" "$T/caps.json"
echo sha256:1111111111111111111111111111111111111111111111111111111111111111 >"$T/image.inspect"
reads_image 1

# Each docker run in a file of its own, one argument per line. A backslash and
# a line end inside an argument are written as escapes, so one argument stays
# one line. Then the container does the least its command needs to go on.
t_stub docker 'case $1 in
  info) cat "$T/docker.info"; exit ;;
  image) case " $* " in *"{{.Id}}"*) cat "$T/image.inspect" ;; *) cat "$T/image.label" ;; esac; exit ;;
  run) ;;
  *) exit 0 ;;
esac
n=$(($(cat "$T/docker.n" 2>/dev/null || echo 0) + 1))
echo "$n" >"$T/docker.n"
for a in "$@"; do
  a=${a//\\/\\\\}
  printf "%s\n" "${a//$'"'"'\n'"'"'/\\n}"
done >"$T/calls/argv.$n"
prev=""
for a in "$@"; do
  case $prev in
    --cidfile) cat "$T/cid" >"$a" ;;
    --doc) doc=$a ;;
  esac
  case $a in
    *:/home/vscode/.tehas-cache) written=${a%%:*} ;;
    *:/improve/out) out=${a%%:/improve/out} ;;
  esac
  prev=$a
done
case " $* " in
  *" --capabilities "*) cat "$T/caps.json" ;;
  *"pi-rukas-headless work "*) echo cycle >&2 ;;
  *"pi-rukas-headless plan "*)
    mkdir -p "$T/home/tehas/work/widget-plan-7/.git/pi-plan-sessions/abc-123"
    echo "{}" >"$T/home/tehas/work/widget-plan-7/.git/pi-plan-sessions/abc-123/session.json"
    cat "$T/fx/turn.json"
    exit 11 ;;
  *" lens-review "*) cat "$T/fx/review.json" | tee "${doc:-/dev/null}" ;;
  *" index_repository "*)
    echo index >"$written/widget.db"
    echo "{\"project\":\"p\",\"status\":\"indexed\",\"nodes\":5,\"edges\":7}" ;;
  *" /improve "*) echo "{\"findings\":[]}" >"$out/findings.json" ;;
esac'
# Only what the runs here ask of GitHub, so that no run reaches the real one.
t_stub gh 'case "$1 $2" in "issue list") echo "[]" ;; esac'

mkdir -p "$fx"
echo '{"subcommand":"plan","mode":"turn","exitCode":11,"status":"question","session":{"id":"abc-123","turn":1},"text":"One question.","questions":[{"id":"q1","question":"Which store?","options":[],"resolution":"SQLite"}]}' >"$fx/turn.json"
echo '{"verdict":"APPROVED","exitCode":0}' >"$fx/review.json"
printf '# Faster search\n\nMake it fast.\n' >"$T/brief.md"
printf 'yes\n' >"$T/answer.md"
# One record for the improvement loop to read.
rec=7-261010-200140
# Two tokens by name, as a host's models.json has them, so their order is pinned.
# No backupBaseUrl, so no run asks an endpoint.
echo '{"providers":{"test-provider":{"baseUrl":"http://models.test/v1","apiKey":"$TEHAS_MODEL_KEY_MAIN","backupApiKey":"$TEHAS_MODEL_KEY_FLEET"}}}' >"$T/home/.pi/agent/models.json"

# The instance of every case, and the tokens of the host's environment: all a
# container can be given by name. A container that takes no token shows it too.
all=("${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9 TEHAS_REVIEW=1 TEHAS_IMPROVE_REPO=acme/factory)
clean() {
  fresh
  rm -f "$T/docker.n"
  t_instance "${all[@]}"
  T_ENV=("${env_ok[@]}" TEHAS_INDEX_LOCK_WAIT=1 TEHAS_MODEL_KEY_MAIN=main-secret TEHAS_MODEL_KEY_FLEET=fleet-secret)
  slice_good
}

# The -e PI_ENSEMBLE_<name> block, by name, in place. Such an option carries no
# value: the ones of the library that do are not in the block.
sort_env_block() {
  awk -v sorter='LC_ALL=C sort | tr " " "\n"' '
    function flush() { if (held) { fflush(); close(sorter); held = 0 } }
    want { want = 0
      if ($0 ~ /^PI_ENSEMBLE_[A-Z0-9_]+$/) { print "-e " $0 | sorter; held = 1; next }
      flush(); print "-e"; print; next }
    /^-e$/ { want = 1; next }
    { flush(); print }
    END { if (want) print "-e"; flush() }'
}
normalised() { # <file of one call>
  sed -E -e "s|$T|@T@|g" \
    -e "s|tehas/pi-rukas:uid$(id -u)|tehas/pi-rukas:uid@UID@|" \
    -e "s|^$(id -u):$(id -g)$|@UID@:@GID@|" \
    -e 's/\b[0-9a-f]{40}\b/@COMMIT@/g' \
    -e 's|(/tmp/[a-z-]+\.)[A-Za-z0-9]{6}\b|\1@RANDOM@|g' \
    -e 's|/run-[0-9]{6}-[0-9]{6}/|/run-@STAMP@/|' \
    -e 's/^(PI_ENSEMBLE_CYCLE_DEADLINE=)[0-9]+$/\1@MS@/' "$1" | sort_env_block
}

# compare <case>: each call of the run against its golden, and no golden left
# over. With TEHAS_TEST_ARGV_RECORD set the run writes them instead.
compare() {
  local name=$1 n=0 got=$T/argv.got
  [ -z "${TEHAS_TEST_ARGV_RECORD:-}" ] || rm -f "$goldens/$name".[0-9]*
  while [ -e "$T/calls/argv.$((n + 1))" ]; do
    n=$((n + 1))
    normalised "$T/calls/argv.$n" >"$got"
    if [ -n "${TEHAS_TEST_ARGV_RECORD:-}" ]; then
      cp "$got" "$goldens/$name.$n"
    elif [ ! -e "$goldens/$name.$n" ]; then
      t_bad "a call more than the goldens hold: $name.$n: $(tr '\n' ' ' <"$got")"
    else
      diff -u "$goldens/$name.$n" "$got" >"$T/argv.diff" || t_bad "$name.$n differs: $(cat "$T/argv.diff")"
    fi
  done
  t_eq "$([ "$n" -gt 0 ] && echo called)" called
  [ ! -e "$goldens/$name.$((n + 1))" ] || t_bad "a call fewer than the goldens hold: $name.$((n + 1))"
}

t_case "a cycle: the image's probe, the push check and the cycle's container"
clean
t_run "$work 7"
t_eq "$t_rc" 2 # the stub ends at once, so the cycle counts as died
compare work

t_case "a planning turn that starts a session"
clean
t_run "$shim/libexec/plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
compare plan-start

t_case "a planning turn that resumes the session"
rm -f "$T"/calls/argv.* "$T/docker.n"
t_run "$shim/libexec/plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
compare plan-resume

t_case "a review"
clean
t_run "$shim/libexec/review"
t_eq "$t_rc" 0
compare review

t_case "the indexer"
clean
t_run "$shim/libexec/index"
t_eq "$t_rc" 0
compare index

t_case "a manual session"
clean
t_run "$shim/libexec/dev"
t_eq "$t_rc" 0
compare dev

t_case "the product's gates in the sandbox"
clean
echo '{"providers":{"lab":{"models":[{"id":"big"}]}}}' >"$TEHAS_INSTANCE_DIR/models.json"
git -C "$checkout" config remote.origin.url https://github.com/acme/widget.git
t_run "$shim/libexec/check-product --sandbox"
git -C "$checkout" config remote.origin.url "$upstream"
t_eq "$t_rc" 0
compare check-product

t_case "the improvement loop's analyst"
clean
rm -f "$TEHAS_INSTANCE_DIR/models.json"
mkdir -p "$T/home/tehas/tmp/improve/window/$rec"
echo '{"issue":7}' >"$T/home/tehas/tmp/improve/window/$rec/cycle.json"
mkdir -p "$T/home/tehas/logs"
echo '{"type":"agent_end"}' >"$T/home/tehas/logs/work-$rec.jsonl"
t_run "$shim/libexec/improve"
t_eq "$t_rc" 0
compare improve

t_done
