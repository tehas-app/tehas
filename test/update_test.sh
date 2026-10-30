#!/usr/bin/env bash
# Tests for libexec/update: the checkout steps, what install.sh is asked to
# do, and which follow-ups an update may run. git runs for real against a bare
# repository in the scratch directory (no network); the cases that run this
# repository's real install.sh in the stubbed host point the two paths
# bootstrap.sh dev writes at scratch, so both sides of the follow-up protocol are
# checked against its producer without reading the host's /etc or /usr/local/bin.
# The sandbox build, docker, systemctl, sudo and timeout are stubs that record
# their arguments.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck disable=SC2329  # push_branch, push_settings_change and
# push_old_config are defined here and called by test/update_checkout_test.sh
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/update_lib.sh
. "$(dirname "$0")/update_lib.sh" || exit 1
# The one place the follow-ups are named. Every case that needs install.sh's
# "next:" lines takes them from here, so the fixture is the text install.sh
# produces rather than a copy of it someone could keep out of step.
# shellcheck source=lib/update-steps.sh
. "$T_ROOT/lib/update-steps.sh"

update="$T_ROOT/libexec/update"
th=$T/home/tehas
checkout=$th/platform
owed_file=$(TEHAS_HOME=$th tehas_pending_file)   # the path, from the one definition

# bootstrap.sh dev writes two files outside any home, and install.sh reads them to
# know whether this host lets other accounts start a session. Pointed at scratch in
# every run here, so no test reads or writes the real /etc or /usr/local/bin.
dev_etc=$T/etc
dev_bin=$T/usr-local-bin
# T_IN_TEST: install.sh skips its dependency check and its own test run here.
T_ENV=(BOOTSTRAP_ETC="$dev_etc" BOOTSTRAP_BIN="$dev_bin" T_IN_TEST=1)

# The host's normal answers: containers running, every unit active, and a
# timeout that records what it bounds and then runs the real one. platform()
# puts them back for each case, because a stub an earlier case installed would
# otherwise answer this one.
# A sandbox build asks the base for its capabilities: the answer of the image
# of pi-rukas main.
docker_up() { t_stub docker 'if [ "$1" = ps ]; then printf "c1\n"; fi
if [ "$1" = run ]; then cat "$T_ROOT/test/fixtures/headless-c5ddc0a/capabilities.json"; fi'; }
systemctl_active() { t_stub systemctl 'if [ "$2" = is-active ]; then echo active; fi'; }
real_timeout=$(command -v timeout)
bound_calls() { t_stub timeout "exec $real_timeout \"\$@\""; }

# docker that answers ps for the docs project only, so only the docs container
# is renewed. The liveness question is `docker ps --quiet
# --filter label=com.docker.compose.project=<name>`; the renewal is
# `docker compose -f <file> up -d`, so ps is the first argument and the label the
# fourth.
docker_up_docs() { t_stub docker 'if [ "$1" = ps ]; then case "$4" in *tehas-docs) printf "c1\n" ;; esac; fi'; }
# docker that cannot answer the question at all: no daemon, or a docker with no
# compose plugin. The command must not read that as "no container is running".
docker_no_answer() { t_stub docker 'if [ "$1" = ps ]; then echo "could not connect to the Docker daemon" >&2; exit 125; fi'; }

# systemctl that prints an error and still exits 0, which is what the wrapper
# some containers ship in place of systemd does. Neither an answer of "active"
# nor one of "inactive": the command has to refuse it.
systemctl_no_answer() { t_stub systemctl 'if [ "$2" = is-active ]; then echo "systemd is not running in this container"; exit 0; fi'; }

# install.sh as this case wants it answered. It records that it ran, so a case
# can tell "ran with no arguments" from "never ran". It lives in the fake
# checkout and is committed with it: an uncommitted stub would read as a local
# change, which is one of the things the command refuses.
write_install() { # <what the stub prints> [<dir to write it in>]
  local dir=${2:-$checkout}
  printf '#!/usr/bin/env bash\nprintf "install %%s\\n" "%s" >>"%s/install"\n%s\n' \
    '${*:-}' "$T/calls" "$1" >"$dir/install.sh"
  chmod +x "$dir/install.sh"
}

# The follow-ups install.sh asks for when its units, its sandbox files and both
# compose servers changed - taken from lib/update-steps.sh, in the order it
# prints them (systemd first), which is the order an update runs them in.
next_lines() {
  tehas_steps_for systemd/tehas-dispatch.timer sandbox/Dockerfile versions.env \
    edge/docs.compose.yaml edge/router.compose.yaml
}

# install.sh asking for every follow-up it knows, as an install stub body.
ask_all() {
  printf 'echo "installed: libexec/update"\n'
  while IFS= read -r step; do printf 'echo "next: %s"\n' "$step"; done < <(next_lines)
}
ASK=$(ask_all)

# A platform checkout with a remote beside it, plus the parts of a host an
# update touches. Arguments: how many commits the remote holds that the
# checkout does not, and what install.sh should print.
platform() { # [behind] [install body]
  local n=${1:-0} body=${2-}
  [ -n "$body" ] || body='echo "installed: nothing"'
  rm -rf "$th" "$T/origin" "$T/upstream" "$T/calls"/*
  systemctl_active
  docker_up
  bound_calls
  mkdir -p "$checkout/libexec" "$checkout/lib" "$th/sandbox" "$th/edge"
  write_install "$body"
  # The host's own `tehas`, as the labels step runs it: by the path
  # $TEHAS_HOME/bin/tehas, and not by name. A stub on PATH would be shadowed by
  # this repository's real bin/tehas, which a case has on it.
  labels_ok
  # The libraries the checkout carries are this repository's own: after a
  # fast-forward, tehas update re-checks the instance with the settings rules
  # that came with the code it pulled.
  cp "$T_ROOT/lib/config.sh" "$T_ROOT/lib/update-steps.sh" "$checkout/lib/"
  : >"$checkout/libexec/work"
  git init -q --initial-branch=main "$checkout"
  git -C "$checkout" config user.name test
  git -C "$checkout" config user.email test@example.com
  git -C "$checkout" add -A
  git -C "$checkout" commit -qm first
  git clone --bare -q "$checkout" "$T/origin"
  git -C "$checkout" remote add origin "$T/origin"
  # The sandbox build and the two compose files, as install.sh would have put
  # them. The build is a stub; the compose files only have to exist.
  printf '#!/usr/bin/env bash\nprintf "%%s\\n" "build" >>"%s/build"\necho "built tehas/pi-rukas"\n' \
    "$T/calls" >"$th/sandbox/build.sh"
  chmod +x "$th/sandbox/build.sh"
  : >"$th/edge/docs.compose.yaml"
  : >"$th/edge/router.compose.yaml"
  for i in $(seq 1 "$n"); do
    push_one "change $i"
  done
}

# One commit from somebody else: clone the remote, change it, push it back.
push_one() { # <line to append>
  rm -rf "$T/upstream"
  git clone -q "$T/origin" "$T/upstream"
  git -C "$T/upstream" config user.name other
  git -C "$T/upstream" config user.email other@example.com
  echo "$1" >>"$T/upstream/notes"
  git -C "$T/upstream" add -A
  git -C "$T/upstream" commit -qm pushed
  git -C "$T/upstream" push -q origin main
}

# New install.sh content on the remote, as somebody else's commit: the next run
# pulls it without wiping the host, so a case can watch what an earlier run owed
# survive into this one.
push_install() { # <what the stub prints>
  rm -rf "$T/upstream"
  git clone -q "$T/origin" "$T/upstream"
  git -C "$T/upstream" config user.name other
  git -C "$T/upstream" config user.email other@example.com
  write_install "$1" "$T/upstream"
  git -C "$T/upstream" add -A
  git -C "$T/upstream" commit -qm "new install.sh"
  git -C "$T/upstream" push -q origin main
}

# A remote whose checkout no longer has a file, as somebody else's commit: the
# next run pulls a checkout that cannot answer a question about it.
push_delete() { # <path to remove>
  rm -rf "$T/upstream"
  git clone -q "$T/origin" "$T/upstream"
  git -C "$T/upstream" config user.name other
  git -C "$T/upstream" config user.email other@example.com
  git -C "$T/upstream" rm -q "$1"
  git -C "$T/upstream" commit -qm "removed $1"
  git -C "$T/upstream" push -q origin main
}

# A branch of its own on the remote, as another person would have pushed it.
push_branch() { # <branch name>
  rm -rf "$T/upstream"
  git clone -q "$T/origin" "$T/upstream"
  git -C "$T/upstream" config user.name other
  git -C "$T/upstream" config user.email other@example.com
  git -C "$T/upstream" checkout -q -b "$1"
  echo "on $1" >>"$T/upstream/notes"
  git -C "$T/upstream" add -A
  git -C "$T/upstream" commit -qm "on $1"
  git -C "$T/upstream" push -q origin "$1"
}

# Where the checkout stands: branch, its commit, what origin has. Twelve hex
# chars, which is what the command prints: git's own --short grows with the object
# count, and a test that compared two of those could disagree for the wrong reason.
state() {
  printf '%s %s %s\n' \
    "$(git -C "$checkout" symbolic-ref --quiet --short HEAD)" \
    "$(git -C "$checkout" rev-parse --short=12 HEAD)" \
    "$(git -C "$checkout" rev-parse --short=12 refs/remotes/origin/main 2>/dev/null || echo none)"
}
head_sha() { git -C "$checkout" rev-parse --short=12 HEAD; }
owed_steps() { cat "$owed_file" 2>/dev/null || true; }

instance() { t_instance "${T_MINIMAL[@]}"; }
# An instance install.sh accepts too: it wants the model list beside the settings.
instance_models() {
  instance
  printf '%s\n' '{"providers": {"test-provider": {"models": [{"id": "test-model"}]}}}' \
    >"$TEHAS_INSTANCE_DIR/models.json"
}

# The platform checkout, its remote, and the parts of a host an update touches,
# with this repository's real install.sh, its libraries and its host/ files in
# the checkout. A case can then run the seam between the two commands against
# its producer.
real_platform() {
  rm -rf "${T:?}/home" "$T/origin" "$T/upstream" "$T/calls"/*
  mkdir -p "$T/home"
  systemctl_active
  docker_up
  bound_calls
  mkdir -p "$checkout"
  cp -R "$T_ROOT/install.sh" "$T_ROOT/bootstrap.sh" "$T_ROOT/bin" "$T_ROOT/libexec" \
    "$T_ROOT/lib" "$T_ROOT/host" "$checkout/"
  t_console_source "$checkout"
  t_stub_cargo
  chmod +x "$checkout/install.sh"
  git init -q --initial-branch=main "$checkout"
  git -C "$checkout" config user.name test
  git -C "$checkout" config user.email test@example.com
  git -C "$checkout" add -A
  git -C "$checkout" commit -qm first
  git clone --bare -q "$checkout" "$T/origin"
  git -C "$checkout" remote add origin "$T/origin"
  push_one "change 1"
}

# What `sudo ./bootstrap.sh dev` leaves, in the scratch paths above: the marker
# naming the tehas user, and the copy of the command the other accounts type.
# login_age makes that copy not the checkout's, as an update of host/login/tehas
# would while nobody ran sudo again.
login_setup() {
  mkdir -p "$dev_etc/tehas" "$dev_bin"
  echo boss >"$dev_etc/tehas/manager"
  install -m 755 "$checkout/host/login/tehas" "$dev_bin/tehas"
  t_stub sudo
}
login_age() { echo "# the copy an earlier bootstrap.sh dev installed" >>"$dev_bin/tehas"; }
login_state() { cksum "$dev_bin/tehas"; }

# The order an update ran the steps it was asked for, read off its own lines.
step_order() {
  printf '%s\n' "$t_out" | sed -n 's/^\(systemd: restarted\|sandbox: building\|docs: updating\|edge: updating\|labels: making\).*/\1/p'
}

# A change on the remote that adds a required setting to lib/config.sh, as some-
# body else's commit: the run pulls it, and the rules it then has to answer are
# the ones that came with it.
push_settings_change() {
  rm -rf "$T/upstream"
  git clone -q "$T/origin" "$T/upstream"
  git -C "$T/upstream" config user.name other
  git -C "$T/upstream" config user.email other@example.com
  cat >>"$T/upstream/lib/config.sh" <<'EOF'
# added by this case: a required setting nothing can give a default for.
tehas_require_instance_settings() {
  tehas_require TEHAS_REPO TEHAS_CONTROL_ISSUE TEHAS_ALLOWED_LABELERS \
    PI_ENSEMBLE_SUBAGENT_MODEL PI_ENSEMBLE_SUBAGENT_PROVIDER TEHAS_NEW_SETTING
}
EOF
  git -C "$T/upstream" add -A
  git -C "$T/upstream" commit -qm "a change that adds a required setting"
  git -C "$T/upstream" push -q origin main
}

# A checkout whose lib/config.sh predates the helpers this command calls for its
# pre-check. It defers to install.sh, which checks the settings itself, rather than
# reporting the missing function as a settings problem.
push_old_config() {
  rm -rf "$T/upstream"
  git clone -q "$T/origin" "$T/upstream"
  git -C "$T/upstream" config user.name other
  git -C "$T/upstream" config user.email other@example.com
  printf '%s\n' 'tehas_require_instance_settings() { return 0; }' >"$T/upstream/lib/config.sh"
  git -C "$T/upstream" add -A
  git -C "$T/upstream" commit -qm "a lib/config.sh that does not define tehas_load"
  git -C "$T/upstream" push -q origin main
}

t_case "missing settings stop it before git, install.sh, Docker or systemctl"
instance
rm -rf "$T/instance"
platform 2
t_run "$update"
t_eq "$t_rc" 78
t_contains "$t_out" "no instance.env in"
t_eq "$(state)" "main $(head_sha) none"
t_eq "$(t_calls install)" ""

t_case "a required setting left commented out stops it before git and install.sh"
t_instance TEHAS_REPO=acme/widget TEHAS_CONTROL_ISSUE=1 TEHAS_ALLOWED_LABELERS=alice
platform 2
before=$(state)
t_run "$update"
t_eq "$t_rc" 78
t_contains "$t_out" "tehas: PI_ENSEMBLE_SUBAGENT_MODEL is not set"
t_eq "$(state)" "$before"
t_eq "$(t_calls install)" ""

t_case "a malformed TEHAS_ALLOWED_LABELERS is refused before the checkout moves"
t_instance "${T_MINIMAL[@]}" 'TEHAS_ALLOWED_LABELERS="alice  bob"'
platform 2
before=$(state)
t_run "$update"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_ALLOWED_LABELERS is not valid: expected GitHub logins separated by single spaces"
t_eq "$(state)" "$before"
t_eq "$(t_calls install)" ""

t_case "an unknown argument is refused before anything is read"
instance
platform 0
t_run "$update --dryrun"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas update: unknown argument: --dryrun"
t_eq "$(t_calls install)" ""

t_case "--help with another argument is refused, naming what it was given"
instance
platform 0
t_run "$update --check --help"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas update: --help takes no other argument, got --check --help"
t_eq "$(t_calls install)" ""

t_case "--help prints the usage with no instance and no checkout"
rm -rf "$T/instance" "$th"
t_run "$update --help"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas update --check"

t_case "no platform checkout: exit 78 saying where it looked"
instance
rm -rf "$th"
t_run "$update"
t_eq "$t_rc" 78
t_contains "$t_out" "no platform checkout at $checkout"

t_case "a checkout that is a git worktree is a checkout too"
instance
platform 0 'echo "installed: nothing"'
# Move the clone aside and put a linked worktree of it where the command looks:
# there .git is a file, not a directory.
mv "$checkout" "$T/main"
git -C "$T/main" checkout -q -b hold
git -C "$T/main" worktree add -q "$checkout" main
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "checkout: main is up to date at $(head_sha)"
t_eq "$(t_calls install)" "install "

t_case "an up to date checkout fetches, installs and runs no follow-up"
instance
platform 0 'echo "tehas is up to date"'
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "checkout: main is up to date at $(head_sha)"
t_contains "$t_out" "tehas is up to date"
t_eq "$(state)" "main $(head_sha) $(head_sha)"
t_eq "$(t_calls install)" "install "
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""
t_eq "$(t_calls systemctl)" ""

t_case "a second update while one runs exits 1 and calls nothing"
instance
platform 2 "$ASK"
before=$(state)
mkdir -p "$th/state"
# The cases that hold the lock here expect a refusal, so they ask to be refused
# at once: no_wait is the knob for that, named beside t_run in test/lib.sh.
# The default wait is 10 seconds, so the bound below is the timing that makes
# this case mean something: a knob that never reaches the command, or a 0 the
# rule throws away, waits the default and is red here instead of green after 10
# seconds of waiting for a refusal that was meant to come at once.
t_start
t_run "$no_wait exec 8>>'$th/state/update.lock'; flock -n 8 || exit 9; '$update'"
t_eq "$t_rc" 1
t_fast 5000
t_contains "$t_out" "tehas update: another update is running ($th/state/update.lock), still held after 0s; nothing was changed"
t_eq "$(state)" "$before"
t_eq "$(t_calls install)" ""
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""
t_eq "$(t_calls systemctl)" ""

t_case "--check takes no lock"
t_run "exec 8>>'$th/state/update.lock'; flock -n 8 || exit 9; '$update' --check"
t_lacks "$t_out" "another update is running"
t_eq "$(t_calls install)" "install --check"

# The lock is waited for, not refused at once, so a run that follows one which
# stopped partway can still get in: the holder below lets go after a second and
# the update runs. The wait is longer than the hold on purpose, and the case is
# timed, so a wait that was not taken shows as a slow run rather than passing.
# The wait is exported in the snippet, where t_run's env -i carries it to the
# command: see test/lib.sh for the same reason no_wait is exported.
t_case "an update that finds the lock held waits for it and runs when it is let go"
platform 0
# platform 0 clears the home, so the state directory it makes is made here.
mkdir -p "$th/state"
( exec 8>>"$th/state/update.lock"; flock -n 8; sleep 1 ) & holder=$!
t_start
t_run "export TEHAS_UPDATE_LOCK_WAIT=5; '$update'"
wait "$holder"
t_eq "$t_rc" 0
# Under the 10s default and over the second the holder keeps it: the wait the
# case names is the one that ran, not the default and not a refusal.
t_fast 9000
t_lacks "$t_out" "another update is running"
t_eq "$(t_calls install)" "install "

# The rule a held lock is measured by, read from the one place both commands get
# it: what each form of the knob becomes, the default for the shapes it refuses,
# and the ceiling both commands share. Asked of the rule itself, in one run, so
# the range is asserted and not only inferred from a timing bound. libexec/update
# reads the value through this same function, so the answers below are the wait
# its flock -w gets.
lock_default=$(sed -n 's/^tehas_update_lock_wait_default=//p' "$T_ROOT/lib/update-steps.sh")
lock_max=$(sed -n 's/^tehas_update_lock_wait_max=//p' "$T_ROOT/lib/update-steps.sh")
[ -n "$lock_default" ] && [ -n "$lock_max" ] ||
  t_bad "lib/update-steps.sh names neither the default nor the ceiling of the wait"
# Every value is asked in one snippet, each on its own line, so no earlier form
# can leave the variable set for the next one.
asks=()
answers=''
for asked in 0 1 5 45 "$lock_max" "$((lock_max + 1))" 9999 999999999999999999999 00 05 0000000009 -1 1.5 1e3 soon ''; do
  # A whole number, no leading zero, of at most three digits and no more than
  # the ceiling. Anything else is the default.
  answer=$lock_default
  case $asked in
  0)
    answer=0
    ;;
  [1-9] | [1-9][0-9] | [1-9][0-9][0-9])
    [ "$asked" -le "$lock_max" ] && answer=$asked
    ;;
  esac
  asks+=("$asked")
  answers="$answers$answer
"
done
t_case "the wait rule takes 0 through $lock_max and the $lock_default-second default"
# Unset first: an inherited value of that name would answer for the empty one.
# Each value is quoted on the child's command line, so a form the rule turns
# into the default - or into nothing - is still one value asked, and none of them
# is taken as shell text by the snippet.
quoted=''
for v in "${asks[@]}"; do quoted="$quoted '$v'"; done
t_run "unset TEHAS_UPDATE_LOCK_WAIT; . '$T_ROOT/lib/update-steps.sh'
for v in$quoted; do TEHAS_UPDATE_LOCK_WAIT=\$v tehas_update_lock_wait; done"
# One answer a line, in the order asked: a rule that always answered the default
# could not pass the assertions on the two ends of the range below.
lines=$(printf '%s\n' "$t_out" | sed '/^$/d')
t_eq "$lines" "${answers%$'\n'}"
# The two ends of the range, so a rule that only ever answered the default could
# not pass: 0 is the refuse-at-once value and the ceiling is still a wait.
t_contains "$lines" "0"
t_contains "$lines" "$lock_max"
t_lacks "$lines" "$((lock_max + 1))"

t_case "install.sh is handed the lock the update holds"
instance
platform 0 'if [ "/dev/fd/${TEHAS_UPDATE_LOCK_FD:-x}" -ef "$TEHAS_HOME/state/update.lock" ] && flock -n "$TEHAS_UPDATE_LOCK_FD"; then echo "the lock is ours"; fi
( exec 6>>"$TEHAS_HOME/state/update.lock"; flock -n 6 ) || echo "held against others"'
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "the lock is ours"
t_contains "$t_out" "held against others"

# A child that outlives the update must not keep the lock: the next update would
# read it as another one running. systemctl stands for every such child here.
t_case "a child the update starts and that outlives it does not keep the lock"
instance
platform 0 'echo "next: units tehas-dispatch.timer"'
t_stub systemctl 'if [ "$2" = is-active ]; then echo active; fi
sleep 30 >/dev/null 2>&1 &
echo $! >>"'"$T"'/calls/sleepers"'
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "systemd: restarted tehas-dispatch.timer"
if ! (exec 6>>"$th/state/update.lock" && flock -n 6); then
  t_bad "a child of the update still holds $th/state/update.lock"
fi
while read -r pid; do kill "$pid" 2>/dev/null; done <"$T/calls/sleepers"
rm -f "$T/calls/sleepers"
systemctl_active

t_case "a unit the pulled step library adds is restarted by the update that pulled it"
instance
platform 0 'echo "next: units tehas-dispatch.timer tehas-new.timer"'
rm -rf "$T/upstream"
git clone -q "$T/origin" "$T/upstream"
sed -i 's/^tehas_units() { printf .%s\\n. /&tehas-new.timer /' "$T/upstream/lib/update-steps.sh"
grep -q 'tehas-new.timer' "$T/upstream/lib/update-steps.sh" || t_bad "the fixture did not add a unit"
git -C "$T/upstream" -c user.name=other -c user.email=other@example.com commit -qam "a new unit"
git -C "$T/upstream" push -q origin main
t_run "$update"
t_eq "$t_rc" 0
t_lacks "$t_out" "WARN"
t_contains "$(t_calls systemctl)" "--user restart tehas-dispatch.timer tehas-new.timer"
t_eq "$(owed_steps)" ""

t_case "a later: line of install.sh is printed and is no step"
instance
platform 0 'echo "installed: libexec/update"
echo "later: ./install.sh again when the running cycle has ended"'
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "later: ./install.sh again when the running cycle has ended"
t_lacks "$t_out" "WARN"
t_eq "$(owed_steps)" ""

t_case "a fast-forward runs the follow-ups install.sh asked for"
instance
platform 1 "$ASK"
before=$(head_sha)
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "checkout: fast-forwarded $before -> $(head_sha) (1 commits)"
t_contains "$t_out" "installed: libexec/update"
# In the order install.sh printed them, systemd first. A line install.sh asked
# for is not echoed back as if it were still work for a person to run.
t_eq "$(step_order)" "systemd: restarted
sandbox: building
docs: updating
edge: updating"
t_lacks "$t_out" "next:"
t_eq "$(t_calls build)" "build"
t_eq "$(t_calls docker)" "ps --quiet --filter label=com.docker.compose.project=tehas-docs
compose -f $th/edge/docs.compose.yaml up -d --force-recreate
ps --quiet --filter label=com.docker.compose.project=tehas-edge
compose -f $th/edge/router.compose.yaml up -d --force-recreate"
t_eq "$(t_calls systemctl)" "--user daemon-reload
--user is-active tehas-dispatch.timer
--user is-active tehas-energy.service
--user is-active tehas-top.service
--user is-active tehas-mcp.service
--user is-active tehas-preview.timer
--user is-active tehas-preview-sweep.timer
--user is-active tehas-protolab.timer
--user is-active tehas-improve.timer
--user is-active tehas-refresh.timer
--user is-active tehas-review.timer
--user is-active tehas-clean.timer
--user restart tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-mcp.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer tehas-refresh.timer tehas-review.timer tehas-clean.timer"
# Every call outside the command went through a timeout, install.sh included,
# and nothing is owed now.
t_eq "$(t_calls timeout | sed 's/^[0-9]* //')" "git -C $checkout fetch --prune origin
$checkout/install.sh
systemctl --user daemon-reload
systemctl --user is-active tehas-dispatch.timer
systemctl --user is-active tehas-energy.service
systemctl --user is-active tehas-top.service
systemctl --user is-active tehas-mcp.service
systemctl --user is-active tehas-preview.timer
systemctl --user is-active tehas-preview-sweep.timer
systemctl --user is-active tehas-protolab.timer
systemctl --user is-active tehas-improve.timer
systemctl --user is-active tehas-refresh.timer
systemctl --user is-active tehas-review.timer
systemctl --user is-active tehas-clean.timer
systemctl --user restart tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-mcp.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer tehas-refresh.timer tehas-review.timer tehas-clean.timer
$th/sandbox/build.sh
docker ps --quiet --filter label=com.docker.compose.project=tehas-docs
docker compose -f $th/edge/docs.compose.yaml up -d --force-recreate
docker ps --quiet --filter label=com.docker.compose.project=tehas-edge
docker compose -f $th/edge/router.compose.yaml up -d --force-recreate"
t_eq "$(owed_steps)" ""

t_case "a container that is not running is left alone, a unit that is not active too"
instance
platform 1 "$ASK"
docker_up_docs
t_stub systemctl 'if [ "$2" = is-active ]; then
  case "$3" in *.timer ) echo active ;; * ) echo inactive; exit 3 ;; esac
fi'
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "docs: updating the container"
t_contains "$t_out" "edge: not running on this host, left alone"
t_eq "$(t_calls docker)" "ps --quiet --filter label=com.docker.compose.project=tehas-docs
compose -f $th/edge/docs.compose.yaml up -d --force-recreate
ps --quiet --filter label=com.docker.compose.project=tehas-edge"
t_eq "$(t_calls systemctl)" "--user daemon-reload
--user is-active tehas-dispatch.timer
--user is-active tehas-energy.service
--user is-active tehas-top.service
--user is-active tehas-mcp.service
--user is-active tehas-preview.timer
--user is-active tehas-preview-sweep.timer
--user is-active tehas-protolab.timer
--user is-active tehas-improve.timer
--user is-active tehas-refresh.timer
--user is-active tehas-review.timer
--user is-active tehas-clean.timer
--user restart tehas-dispatch.timer tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer tehas-refresh.timer tehas-review.timer tehas-clean.timer"
t_eq "$(owed_steps)" ""

t_case "every state of systemd's active family restarts the unit"
instance
platform 1 "$ASK"
# A unit between crash-loop attempts answers activating; one caught mid-reload
# answers reloading. Both run on this host, so both have to get the new unit
# file - and the update has to stay green, because that is a real answer.
t_stub systemctl 'if [ "$2" = is-active ]; then
  case "$3" in
    tehas-dispatch.timer) echo activating ;;
    tehas-energy.service) echo reloading ;;
    *) echo active ;;
  esac
fi'
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "systemd: restarted tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-mcp.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer tehas-refresh.timer tehas-review.timer tehas-clean.timer"
t_lacks "$t_out" "no unit was restarted"

t_case "a unit systemd does not know is an answer, and leaves that unit alone"
instance
platform 1 "$ASK"
# The unit file is on the host but systemd has never loaded it: is-active says
# unknown. That is not a failed query, and it is not a reason to stop.
t_stub systemctl 'if [ "$2" = is-active ]; then
  case "$3" in tehas-dispatch.timer ) echo unknown; exit 3 ;; * ) echo active ;; esac
fi'
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "systemd: restarted tehas-energy.service tehas-top.service tehas-mcp.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer tehas-refresh.timer tehas-review.timer tehas-clean.timer"
t_lacks "$t_out" "no unit was restarted"

t_case "a docker query that fails stops the update instead of reading as not running"
instance
platform 1 "$ASK"
docker_no_answer
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "docs: docker ps failed; the step is not done"
t_contains "$t_out" "could not connect to the Docker daemon"
t_lacks "$t_out" "not running on this host"
# install.sh and the steps before docs ran: the units and the sandbox build. The
# edge router renewal was never reached, and is owed.
t_eq "$(t_calls docker)" "ps --quiet --filter label=com.docker.compose.project=tehas-docs"
t_eq "$(t_calls build)" "build"
t_contains "$t_out" "systemd: restarted tehas-dispatch.timer"
t_eq "$(owed_steps)" "docs --force-recreate
edge --force-recreate"

t_case "a systemd query that fails stops the update before any other step"
instance
platform 1 "$ASK"
t_stub systemctl 'if [ "$2" = is-active ]; then echo "Failed to connect to bus: No such file or directory" >&2; exit 1; fi'
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "systemd: systemctl is-active gave no answer for tehas-dispatch.timer; no unit was restarted"
t_contains "$t_out" "Failed to connect to bus"
t_lacks "$t_out" "none of them runs on this host"
# systemd is the step install.sh asks for first, so nothing else ran, and all
# four steps are still owed.
t_eq "$(t_calls docker)" ""
t_eq "$(t_calls build)" ""
t_eq "$(owed_steps)" "units tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-mcp.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer tehas-refresh.timer tehas-review.timer tehas-clean.timer
sandbox
docs --force-recreate
edge --force-recreate"

t_case "a systemctl that prints an error and exits 0 is not read as active"
instance
platform 1 "$ASK"
systemctl_no_answer
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "systemd: systemctl is-active gave no answer for tehas-dispatch.timer; no unit was restarted"
t_contains "$t_out" "systemd is not running in this container"
t_lacks "$t_out" "systemd: restarted"
t_lacks "$t_out" "none of them runs on this host"
# The reload was asked for first and answered cleanly; the state question was
# asked once, and its answer stopped the run before any restart.
t_eq "$(t_calls systemctl)" "--user daemon-reload
--user is-active tehas-dispatch.timer"

t_case "a call that hits its timeout fails saying so"
instance
platform 1 "$ASK"
# A remote that stops answering: 124 is what timeout answers when a call runs
# out of its limit.
t_stub timeout 'echo "timed out"; exit 124'
before=$(state)
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "could not fetch from origin; nothing was changed"
t_contains "$t_out" "the fetch gave no answer in 120s"
t_eq "$(state)" "$before"
t_eq "$(t_calls install)" ""

t_case "a step that fails is recorded, and the next run finishes it"
instance
platform 1 "$ASK"
docker_no_answer
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "docs: docker ps failed; the step is not done"
t_contains "$t_out" "2 of 4 follow-up(s) are not done, recorded in $owed_file"
t_eq "$(owed_steps)" "docs --force-recreate
edge --force-recreate"

# The next run: install.sh has nothing new to report - its one-shot list was
# spent by the run that copied the files - and the containers come up anyway.
docker_up
push_install 'echo "tehas is up to date"'
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "follow-up(s) an earlier run did not finish (see $owed_file)"
t_contains "$t_out" "next: docs"
t_contains "$t_out" "docs: updating the container"
t_contains "$t_out" "edge: updating the container"
# The steps that already ran are not run again.
t_eq "$(t_calls build)" "build"
t_eq "$(owed_steps)" ""


# The seam between the two commands, checked against its producer: this
# repository's real install.sh, run by tehas update in the stubbed host.
t_case "the real install.sh through tehas update runs its follow-ups in its order"
instance_models
# The labels step of the run calls the host's own `tehas`, which calls gh: the stub
# of test/labels_test.sh, which makes every label of the command that asks.
t_stub gh
real_platform
# A host of one install before this run: the marker says so, or install.sh spends
# its last act on the one install of a new host (the labels and the core units) and
# asks for no labels step.
t_run "mkdir -p '$th/state'; : >'$th/state/first-install'
'$update'"
t_eq "$t_rc" 0
t_contains "$t_out" "installed: $th/libexec/update"
t_contains "$t_out" "systemd: restarted tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-mcp.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer tehas-refresh.timer tehas-review.timer tehas-clean.timer"
t_contains "$t_out" "docs: updating the container"
t_contains "$t_out" "edge: updating the container"
# The command that makes the labels is among the files this run copied, so the
# labels step is owed too, and the run is green: the stubbed host's command makes
# them, so nothing is left looking like work for a person.
t_eq "$(step_order)" "systemd: restarted
sandbox: building
docs: updating
edge: updating
labels: making"
t_lacks "$t_out" "next:"
t_eq "$(owed_steps)" ""
# The step ran the host's own command, which is the way a person runs it: every
# label the command names is made in the product repository of the instance, and by
# gh and nothing else. The copies of this run made the labels twice over: install.sh
# makes them itself, and then the step this change adds makes them again.
t_eq "$(t_calls gh | grep '^label create ' | sed 's/^label create \([^ ]*\).*/\1/' | sort -u | wc -l)" \
  "$(sed -n 's/^mk \([^ ]*\).*/\1/p' "$T_ROOT/libexec/labels" | sort -u | wc -l)"
t_eq "$(t_calls gh | sed -n 's/^label create [^ ]* --repo \([^ ]*\).*/\1/p' | sort -u)" acme/widget
# Nothing of this host's real /etc or /usr/local/bin was read: the scratch marker
# is not there, so install.sh says nothing about root's copy.
t_lacks "$t_out" "Login:"

# The same seam with systemd refusing the reload: the step that restarts the
# units stops, so the run ends red with everything owed rather than green with
# unit files systemd never read.
t_case "changed unit files are not reported as applied when systemd did not reload"
instance_models
# The host's `tehas` calls gh for each label, so the step that runs it needs the
# stub of test/labels_test.sh rather than the gh of this host, which has no token.
t_stub gh
real_platform
t_stub systemctl 'if [ "$2" = daemon-reload ]; then echo "Failed to connect to bus: No such file or directory" >&2; exit 1; fi
if [ "$2" = is-active ]; then echo active; fi'
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "systemd: systemctl --user daemon-reload did not run; the changed unit files are not in effect, no unit was restarted, and the step is not done"
t_contains "$t_out" "Failed to connect to bus"
t_lacks "$t_out" "systemd: restarted"
# Nothing after the units step ran, and every step install.sh recorded is owed.
t_lacks "$t_out" "docs: updating the container"
t_eq "$(owed_steps)" "$(next_lines)
labels"

# The next run on the same host, with systemd answering: install.sh has nothing
# new to report - its one-shot list was spent by the run that copied the files -
# and the steps it recorded then are finished now.
t_stub systemctl 'if [ "$2" = is-active ]; then echo active; fi'
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "follow-up(s) an earlier run did not finish (see $owed_file)"
t_contains "$t_out" "systemd: restarted tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-mcp.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer tehas-refresh.timer tehas-review.timer tehas-clean.timer"
t_contains "$t_out" "docs: updating the container"
t_eq "$(owed_steps)" ""

# The one file of the host install.sh reports and never touches: root's copy of
# host/login/tehas. Checked here rather than in the install tests, because this is
# the fixture with a checkout of its own that holds the real install.sh.
t_case "tehas update prints install.sh's later line for a stale login copy and runs nothing for it"
instance_models
# The labels step calls the host's `tehas`, which calls gh: the stub makes them.
t_stub gh
real_platform
login_setup
login_age
before=$(login_state)
# The install before this one, so the labels of the copied command are owed as a
# step of this update rather than made by a first install on the spot.
t_run "mkdir -p '$th/state'; : >'$th/state/first-install'; '$update'"
t_eq "$t_rc" 0
t_contains "$t_out" "later: sudo $checkout/bootstrap.sh dev boss"
t_contains "$t_out" "$dev_bin/tehas is not this checkout's host/login/tehas"
# Not a step: nothing was recorded for it, no sudo ran, and the copy was not
# touched. The labels step of the copied command ran in the same call, so the
# record it clears is empty by the end of the run.
t_eq "$(t_calls sudo)" ""
t_eq "$(login_state)" "$before"
# The next --check still says it, and the line reaches the operator as install.sh
# printed it: this command filters only the follow-up lines it runs itself.
t_run "$update --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Login:        NOT CURRENT"
t_contains "$t_out" "differs: $dev_bin/tehas"
t_contains "$t_out" "sudo $checkout/bootstrap.sh dev boss"
t_eq "$(t_calls sudo)" ""
t_eq "$(login_state)" "$before"

t_case "a checkout that no longer has the login source says so, not that the host differs"
instance_models
real_platform
login_setup
login_age
before=$(login_state)
push_delete host/login/tehas
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "$checkout/host/login/tehas is missing from this checkout"
t_contains "$t_out" "no follow-up was run"
# A comparison that could not be made is no finding about the host's copy: the
# run says the checkout is missing a file, as it does for one it would install.
t_lacks "$t_out" "differs: $dev_bin/tehas"
t_lacks "$t_out" "sudo $checkout/bootstrap.sh dev boss"
t_eq "$(owed_steps)" ""
t_eq "$(login_state)" "$before"
# The next --check says the same, and not that the host's copy differs.
t_run "$update --check"
t_eq "$t_rc" 1
t_contains "$t_out" "$checkout/host/login/tehas is missing from this checkout"
t_lacks "$t_out" "Login:"
t_lacks "$t_out" "differs: $dev_bin/tehas"

t_case "a step line no version here knows stays owed until something knows it"
# The producer and the consumer of a step can be two versions of tehas: install.sh
# comes from the checkout it just pulled, this command is the one that was running.
# The line cannot be dropped, or the host would go green with the step never done.
instance
platform 0 'echo "next: renew-dashboard --now"'
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: 1 step line(s) name no step this command knows"
t_eq "$(owed_steps)" "renew-dashboard --now"
# The next run, install.sh reporting nothing new: still not run, still recorded,
# still red.
push_install 'echo "tehas is up to date"'
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: 1 step line(s) name no step this command knows"
t_contains "$t_out" "next: renew-dashboard --now"
t_eq "$(owed_steps)" "renew-dashboard --now"

t_case "steps an install.sh that died partway recorded are run by the next one"
instance
platform 1 'echo "installed: libexec/dispatch"
. "$T_ROOT/lib/update-steps.sh"
tehas_record_steps sandbox "docs --force-recreate"
echo "tehas: /x is missing from this checkout" >&2
exit 1'
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "install: install.sh exited 1; no follow-up was run"
# No report was printed, so nothing ran here - but the record it left behind is
# what the next run works from.
t_eq "$(t_calls build)" ""
t_eq "$(owed_steps)" "sandbox
docs --force-recreate"
push_install 'echo "tehas is up to date"'
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "sandbox: building the image"
t_contains "$t_out" "docs: updating the container"
t_eq "$(owed_steps)" ""

t_case "an install.sh that stops answering is stopped at its limit"
instance
platform 0 'sleep 3600'
# Only the install call is answered with a timeout: 1800s is this command's limit
# for install.sh, the step that copies files, checks dependencies and runs tests.
t_stub timeout "case \"\$*\" in
  *install.sh*) echo 'no answer'; exit 124 ;;
  *) exec $real_timeout \"\$@\" ;;
esac"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "install: install.sh gave no answer in 1800s and was stopped"
t_eq "$(owed_steps)" ""

t_case "an install.sh --check that stops answering claims no copies"
before=$(head_sha)
t_run "$update --check"
t_eq "$t_rc" 1
t_contains "$t_out" "install: install.sh --check gave no answer in 1800s and was stopped; it changes nothing, so nothing was installed"
t_lacks "$t_out" "what it had copied"
t_eq "$(head_sha)" "$before"

t_case "a step owed from an earlier run runs where its id says it runs"
instance
platform 1 'echo "next: units tehas-dispatch.timer"'
mkdir -p "$th/state"
printf 'docs --force-recreate\n' >"$owed_file"
t_run "$update"
t_eq "$t_rc" 0
# systemd first, whatever order the two of them were recorded in.
t_eq "$(step_order)" "systemd: restarted
docs: updating"
t_eq "$(owed_steps)" ""

t_case "the compose files pin the project names the liveness check filters on"
for pair in "docs.compose.yaml tehas-docs" "router.compose.yaml tehas-edge"; do
  t_contains "$(cat "$T_ROOT/host/edge/${pair%% *}")" "name: ${pair#* }"
done

# --- pi-rukas follows upstream ------------------------------------------------
# The installed versions.env names what to follow, and build.sh is a stub that keeps
# its arguments and exits with the given status.
rukas_build() { # <exit status>
  cat >"$th/sandbox/build.sh" <<EOF
#!/usr/bin/env bash
echo "build \$*" >>"$T/calls/build"
echo "pi-rukas: stub"
exit $1
EOF
  chmod +x "$th/sandbox/build.sh"
  echo "PI_RUKAS_FOLLOW=release" >"$th/versions.env"
}

t_case "an up to date checkout still moves the image to the pi-rukas commit it follows"
instance
platform 0
rukas_build 0
t_run "$update"
t_eq "$t_rc" 0
t_eq "$(t_calls build)" "build --if-moved"
t_contains "$(t_calls timeout)" "$th/sandbox/build.sh --if-moved"

t_case "a run that built the sandbox image does not look at the head again"
instance
platform 1 'echo "next: sandbox"'
rukas_build 0
t_run "$update"
t_eq "$t_rc" 0
t_eq "$(t_calls build)" "build "

t_case "an image that cannot be moved fails the update"
instance
platform 0
rukas_build 1
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "pi-rukas: the image was not moved to the commit versions.env follows"

t_case "--check says the commit moved and builds nothing"
instance
platform 0
rukas_build 1
t_run "$update --check"
t_eq "$t_rc" 1
t_eq "$(t_calls build)" "build --check"
t_contains "$t_out" "pi-rukas: stub"

t_case "a versions.env with no PI_RUKAS_FOLLOW leaves the image alone"
instance
platform 0
echo "PI_RUKAS_COMMIT=0123456789abcdef0123456789abcdef01234567" >"$th/versions.env"
t_run "$update"
t_eq "$t_rc" 0
t_eq "$(t_calls build)" ""

# The host's clone of pi-rukas only moves when bootstrap.sh user runs, and
# versions.env is not the only thing that can move the commit it follows: a
# followed release gets a new head with no file of tehas changing. The follow-up
# is printed for that case too, and the exit status is what it was - the clone is
# the operator's to move, as host/docs/host.md says.
image_head=$(printf 'b%.0s' {1..40})

t_case "an image past the host's clone asks for bootstrap.sh user on a later line"
instance
platform 0
rukas_build 0
rukas_clone "$image_head" >/dev/null
rukas_image "$image_head"
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "later: $checkout/bootstrap.sh user   (pi-rukas moved to ${image_head:0:7}"
t_contains "$t_out" "until it runs, --check says Dependencies: NOT OK)"
# The image step still ran, and this line is not a step: nothing was recorded.
t_eq "$(t_calls build)" "build --if-moved"
t_eq "$(owed_steps)" ""

t_case "a clone on the image's commit asks for nothing"
instance
platform 0
rukas_build 0
head=$(rukas_clone "$image_head")
rukas_image "$head"
t_run "$update"
t_eq "$t_rc" 0
t_eq "$head" "$(git -C "$th/repos/pi-rukas" rev-parse HEAD)"
t_lacks "$t_out" "pi-rukas moved to"
t_lacks "$t_out" "bootstrap.sh user"

t_case "a host with no clone, and one with no label on its image, say nothing and end 0"
instance
platform 0
rukas_build 0
rukas_clone none
rukas_image "$image_head"
t_run "$update"
t_eq "$t_rc" 0
t_lacks "$t_out" "pi-rukas moved to"
t_lacks "$t_out" "bootstrap.sh user"

instance
platform 0
rukas_build 0
rukas_clone "$image_head" >/dev/null
rukas_image none
t_run "$update"
t_eq "$t_rc" 0
t_lacks "$t_out" "pi-rukas moved to"
t_lacks "$t_out" "bootstrap.sh user"

# Once per run: install.sh already names the follow-up when it copied a changed
# versions.env, and a person reading the log needs the step once.
t_case "the line of a run that copied versions.env is not printed twice"
instance
platform 0 "echo 'later: $th/current/bootstrap.sh user   (versions.env changed; until it runs, --check says Dependencies: NOT OK)'"
rukas_build 0
rukas_clone "$image_head" >/dev/null
rukas_image "$image_head"
t_run "$update"
t_eq "$t_rc" 0
t_eq "$(grep -c 'bootstrap.sh user' <<<"$t_out")" 1
t_contains "$t_out" "versions.env changed"
t_lacks "$t_out" "pi-rukas moved to"

# --- tehas update --pending ---------------------------------------------------
# The follow-ups install.sh recorded, with no git and no install.sh in the way.
# The host this form is for has a checkout it cannot move - a release tag, or a
# remote that no longer answers - and the steps are all that are left of the
# install, so the record is the whole of the work list and nothing outside it is
# asked a question.
t_case "--pending runs the steps the record lists, in step order, and clears it"
instance
platform 0 'echo "install.sh must not run in this case"'
record "docs --force-recreate" sandbox
t_run "$update --pending"
t_eq "$t_rc" 0
t_contains "$t_out" "2 follow-up(s) an earlier run did not finish (see $owed_file)"
t_contains "$t_out" "next: docs --force-recreate"
# systemd first, whatever order the two lines were recorded in.
t_eq "$(step_order)" "sandbox: building
docs: updating"
t_eq "$(t_calls build)" "build"
t_eq "$(t_calls docker)" "ps --quiet --filter label=com.docker.compose.project=tehas-docs
compose -f $th/edge/docs.compose.yaml up -d --force-recreate"
t_eq "$(owed_steps)" ""
# Every call of this run went through a timeout, and none of them was git's: the
# fetch a real run makes first is not made at all here.
t_eq "$(t_calls timeout | sed 's/^[0-9]* //')" "$th/sandbox/build.sh
docker ps --quiet --filter label=com.docker.compose.project=tehas-docs
docker compose -f $th/edge/docs.compose.yaml up -d --force-recreate"

t_case "--pending runs with no checkout and no current, and calls no git"
instance
# A host of the kind that needs this form: no platform checkout and no
# ~/tehas/current, so a plain update stops with 78 naming TEHAS_UPDATE_FROM. git
# is a stub that fails, so a single call would be both a wrong answer and a
# recorded one.
t_stub git 'echo "git: a --pending run must not call this" >&2; exit 128'
record "docs --force-recreate" sandbox
t_run "$update --pending"
t_eq "$t_rc" 0
t_eq "$(t_calls git)" ""
t_contains "$t_out" "sandbox: building the image"
t_contains "$t_out" "docs: updating the container"
t_eq "$(owed_steps)" ""

t_case "--pending runs no install.sh and reads no release"
t_stub git
instance
platform 0 'echo "install.sh ran"'
record sandbox
t_run "$update --pending"
t_eq "$t_rc" 0
t_eq "$(t_calls install)" ""
t_lacks "$t_out" "install.sh ran"
t_eq "$(t_calls build)" "build"

t_case "a checkout at a detached HEAD does not stop --pending"
instance
platform 0
# The checkout is at a tag with no branch on it, which is what a host installed
# from a release looks like: a plain update refuses it, and this form never asks.
git -C "$checkout" checkout -q --detach HEAD
[ -z "$(git -C "$checkout" symbolic-ref --quiet --short HEAD)" ] ||
  t_bad "the fixture left the checkout on a branch"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "$checkout is not on a branch (detached HEAD); nothing was changed"
record "docs --force-recreate" sandbox
rm -f "$T"/calls/install
t_run "$update --pending"
t_eq "$t_rc" 0
t_lacks "$t_out" "not on a branch"
t_contains "$t_out" "docs: updating the container"
t_eq "$(owed_steps)" ""

t_case "--pending with an empty record says nothing is owed"
instance
platform 0
t_run "$update --pending"
t_eq "$t_rc" 0
t_contains "$t_out" "update: nothing is owed"
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""
t_eq "$(t_calls systemctl)" ""
t_eq "$(t_calls install)" ""

t_case "--pending with a record of a step no version here knows is red, not empty"
instance
platform 0
record "renew-dashboard --now"
t_run "$update --pending"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: 1 step line(s) name no step this command knows"
t_lacks "$t_out" "nothing is owed"
t_eq "$(owed_steps)" "renew-dashboard --now"

t_case "--pending takes the update lock as a real run does"
instance
platform 0
mkdir -p "$th/state"
record sandbox
t_start
t_run "$no_wait exec 8>>'$th/state/update.lock'; flock -n 8 || exit 9; '$update' --pending"
t_eq "$t_rc" 1
t_fast 5000
t_contains "$t_out" "another update is running ($th/state/update.lock), still held after 0s; nothing was changed"
t_eq "$(t_calls build)" ""
t_eq "$(owed_steps)" "sandbox"

t_case "--pending with --check or --to is refused before the settings are read"
record sandbox
# No instance at all: the refusal is an answer about the arguments, so it comes
# back 64 where a run of any other mode would have stopped with 78.
t_run "unset TEHAS_INSTANCE_DIR; '$update' --pending --check"
t_eq "$t_rc" 64
t_contains "$t_out" "--pending runs the recorded follow-ups, and does not go with --check"
for args in "--check --pending" "--pending --to v1.2.3" "--to v1.2.3 --pending"; do
  t_run "$update $args"
  t_eq "$t_rc" 64
  t_contains "$t_out" "--pending runs the recorded follow-ups, and does not go with"
done
t_eq "$(owed_steps)" "sandbox"

# What a --pending run does not do, checked the other way round: a step that
# fails leaves its line in the record, so the run is red and the next one has the
# same work list the first was given.
t_case "a step that fails under --pending stays recorded for the next run"
instance
platform 0
docker_no_answer
record "docs --force-recreate" sandbox
t_run "$update --pending"
t_eq "$t_rc" 1
t_contains "$t_out" "docs: docker ps failed; the step is not done"
t_eq "$(t_calls build)" "build"
t_eq "$(owed_steps)" "docs --force-recreate"

# The hold is a person's way of keeping a plain update from moving the checkout
# forward after they moved it back. --pending moves nothing, so a host held on an
# older release still finishes the follow-ups install.sh recorded there.
t_case "--pending runs the steps with the platform held"
instance
platform 0
mkdir -p "$th/state"
: >"$th/state/update-hold"
record "docs --force-recreate"
t_run "$update --pending"
t_eq "$t_rc" 0
t_lacks "$t_out" "the platform is held"
t_contains "$t_out" "docs: updating the container"
t_eq "$(owed_steps)" ""
rm -f "$th/state/update-hold"

# A release that adds a label records the labels step when it copies the command
# that makes them, and the follow-up is `tehas labels` itself: the labels end up
# on GitHub rather than owed to nobody in the record.
t_case "--pending runs the labels step once and clears the record"
instance
platform 0
record labels
t_run "$update --pending"
t_eq "$t_rc" 0
t_contains "$t_out" "labels: making the tehas labels"
t_eq "$(t_calls tehas)" "labels"
# Like every other outside call of a run, it goes through a timeout, and the lock
# descriptor is closed for the child that would otherwise hold it after this run.
t_eq "$(t_calls timeout | sed 's/^[0-9]* //')" "$th/bin/tehas labels"
t_eq "$(owed_steps)" ""

# The labels of a product repository are the last thing an update touches, so a
# step that follows one would run the containers before the labels existed. There
# is no step after it in the order lib/update-steps.sh gives them.
t_case "the labels step runs after the other steps, and a failing one keeps it owed"
instance
platform 0
record "docs --force-recreate" labels
t_run "$update --pending"
t_eq "$t_rc" 0
t_eq "$(step_order)" "docs: updating
labels: making"
t_eq "$(t_calls tehas)" "labels"
t_eq "$(owed_steps)" ""
platform 0
labels_fail
record labels
t_run "$update --pending"
t_eq "$t_rc" 1
t_contains "$t_out" "labels: tehas labels exited 1; the step is not done"
t_contains "$t_out" "could not make tehas:ready"
t_eq "$(owed_steps)" "labels"
# The next run, with the host's command answering again, finishes what this one owed.
labels_ok
rm -f "$T/calls/tehas"
t_run "$update --pending"
t_eq "$t_rc" 0
t_eq "$(t_calls tehas)" "labels"
t_eq "$(owed_steps)" ""

# The step names the installed command by path rather than by whatever `tehas` the
# calling account happens to have on its PATH: a host with no command there has an
# update that says so and leaves the step owed.
t_case "the labels step names the host's command, and is owed when it is not there"
instance
platform 0
record labels
rm -f "$th/bin/tehas"
t_run "$update --pending"
t_eq "$t_rc" 1
t_contains "$t_out" "labels: $th/bin/tehas is not installed"
t_eq "$(t_calls tehas)" ""
t_eq "$(owed_steps)" "labels"

t_done
