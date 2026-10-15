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
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"
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
# The old Protolab project is not running here: the edge step asks about it first.
docker_up() { t_stub docker 'if [ "$1" = ps ]; then case "$4" in *tehas-protolab) ;; *) printf "c1\n" ;; esac; fi'; }
systemctl_active() { t_stub systemctl 'if [ "$2" = is-active ]; then echo active; fi'; }
real_timeout=$(command -v timeout)
bound_calls() { t_stub timeout "exec $real_timeout \"\$@\""; }

# docker that answers ps for the docs project only, which is what a host without
# a Protolab server looks like. The liveness question is `docker ps --quiet
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
  printf '%s\n' "$t_out" | sed -n 's/^\(systemd: restarted\|sandbox: building\|docs: updating\|edge: updating\).*/\1/p'
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

t_case "more than one argument is refused, naming what it was given"
instance
platform 0
t_run "$update --check --help"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas update: takes at most one argument, got --check --help"
# The same mistake answered the same way as in the dispatcher: the count is
# refused before any of the arguments is read as a mode.
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
t_run "exec 8>>'$th/state/update.lock'; flock -n 8 || exit 9; '$update'"
t_eq "$t_rc" 1
t_contains "$t_out" "tehas update: another update is running ($th/state/update.lock); nothing was changed"
t_eq "$(state)" "$before"
t_eq "$(t_calls install)" ""
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""
t_eq "$(t_calls systemctl)" ""

t_case "--check takes no lock"
t_run "exec 8>>'$th/state/update.lock'; flock -n 8 || exit 9; '$update' --check"
t_lacks "$t_out" "another update is running"
t_eq "$(t_calls install)" "install --check"

t_case "install.sh is handed the lock the update holds"
instance
platform 0 'if [ "/dev/fd/${TEHAS_UPDATE_LOCK_FD:-x}" -ef "$TEHAS_HOME/state/update.lock" ] && flock -n "$TEHAS_UPDATE_LOCK_FD"; then echo "the lock is ours"; fi
( exec 6>>"$TEHAS_HOME/state/update.lock"; flock -n 6 ) || echo "held against others"'
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "the lock is ours"
t_contains "$t_out" "held against others"

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
ps --quiet --filter label=com.docker.compose.project=tehas-protolab
ps --quiet --filter label=com.docker.compose.project=tehas-edge
compose -f $th/edge/router.compose.yaml up -d --force-recreate"
t_eq "$(t_calls systemctl)" "--user daemon-reload
--user is-active tehas-dispatch.timer
--user is-active tehas-energy.service
--user is-active tehas-top.service
--user is-active tehas-preview.timer
--user is-active tehas-preview-sweep.timer
--user is-active tehas-protolab.timer
--user is-active tehas-improve.timer
--user restart tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer"
# Every call outside the command went through a timeout, install.sh included,
# and nothing is owed now.
t_eq "$(t_calls timeout | sed 's/^[0-9]* //')" "git -C $checkout fetch --prune origin
$checkout/install.sh
systemctl --user daemon-reload
systemctl --user is-active tehas-dispatch.timer
systemctl --user is-active tehas-energy.service
systemctl --user is-active tehas-top.service
systemctl --user is-active tehas-preview.timer
systemctl --user is-active tehas-preview-sweep.timer
systemctl --user is-active tehas-protolab.timer
systemctl --user is-active tehas-improve.timer
systemctl --user restart tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer
$th/sandbox/build.sh
docker ps --quiet --filter label=com.docker.compose.project=tehas-docs
docker compose -f $th/edge/docs.compose.yaml up -d --force-recreate
docker ps --quiet --filter label=com.docker.compose.project=tehas-protolab
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
ps --quiet --filter label=com.docker.compose.project=tehas-protolab
ps --quiet --filter label=com.docker.compose.project=tehas-edge"
t_eq "$(t_calls systemctl)" "--user daemon-reload
--user is-active tehas-dispatch.timer
--user is-active tehas-energy.service
--user is-active tehas-top.service
--user is-active tehas-preview.timer
--user is-active tehas-preview-sweep.timer
--user is-active tehas-protolab.timer
--user is-active tehas-improve.timer
--user restart tehas-dispatch.timer tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer"
t_eq "$(owed_steps)" ""

t_case "a running server of the old project is taken over by the edge router"
instance
platform 1 "$ASK"
# Only the old project runs; `down` and `up` succeed.
t_stub docker 'if [ "$1" = ps ]; then case "$4" in *tehas-protolab) printf "c1\n" ;; esac; fi'
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "edge: taking over from the Protolab's server (tehas-protolab)"
t_contains "$(t_calls docker)" "ps --quiet --filter label=com.docker.compose.project=tehas-protolab
compose -p tehas-protolab down
compose -f $th/edge/router.compose.yaml up -d --force-recreate"
t_lacks "$(t_calls docker)" "project=tehas-edge"
t_lacks "$(owed_steps)" "edge"

t_case "a failed question about the old project leaves the step not done"
instance
platform 1 "$ASK"
t_stub docker 'if [ "$1" = ps ]; then case "$4" in *tehas-protolab) echo "could not connect" >&2; exit 125 ;; *) printf "c1\n" ;; esac; fi'
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "edge: docker ps failed; the step is not done"
t_lacks "$(t_calls docker)" "down"
t_contains "$(owed_steps)" "edge --force-recreate"

t_case "an old project that will not come down is not replaced"
instance
platform 1 "$ASK"
t_stub docker 'case "$1" in ps) case "$4" in *tehas-protolab) printf "c1\n" ;; esac ;; compose) [ "$2" = -p ] && { echo "cannot remove" >&2; exit 1; } ;; esac'
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "edge: docker compose down of tehas-protolab exited 1; the step is not done"
t_lacks "$(t_calls docker)" "router.compose.yaml up"
t_contains "$(owed_steps)" "edge --force-recreate"

# The takeover is two commands. One that stopped between them leaves a mark,
# so the next run does not find nothing running and call the step done.
t_case "a takeover whose router did not come up is not done, and leaves a mark"
instance
platform 1 "$ASK"
# The old project runs until it is taken down; `up` of the router fails.
t_stub docker 'case "$1" in
  ps) case "$4" in *tehas-protolab) [ -e "$T/calls/down" ] || printf "c1\n" ;; esac ;;
  compose)
    [ "$2" != -p ] || { : >"$T/calls/down"; exit 0; }
    case "$*" in *router.compose.yaml*) [ ! -e "$T/up-fails" ] || { echo "port is already allocated" >&2; exit 1; } ;; esac ;;
esac'
: >"$T/up-fails"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "edge: docker compose up exited 1; the step is not done"
t_contains "$(t_calls docker)" "compose -p tehas-protolab down
compose -f $th/edge/router.compose.yaml up -d --force-recreate"
t_eq "$(owed_steps)" "edge --force-recreate"
t_eq "$(grep -cE '^[0-9]+$' "$th/state/edge-takeover")" 1

t_case "the next run finishes the takeover with nothing of either project running"
rm -f "$T"/calls/docker
push_install 'echo "tehas is up to date"'
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "edge: finishing the takeover from the Protolab's server"
t_lacks "$t_out" "left alone"
t_contains "$(t_calls docker)" "compose -f $th/edge/router.compose.yaml up -d --force-recreate"
t_lacks "$(t_calls docker)" "down"
t_eq "$(owed_steps)" "edge --force-recreate"
t_eq "$(find "$th/state" -name edge-takeover | wc -l)" 1
# And once the router comes up, the mark goes and the step is done.
rm -f "$T/up-fails" "$T"/calls/docker
push_install 'echo "tehas is up to date!"'
t_run "$update"
t_eq "$t_rc" 0
t_lacks "$t_out" "left alone"
t_eq "$(t_calls docker)" "compose -f $th/edge/router.compose.yaml up -d --force-recreate"
t_eq "$(owed_steps)" ""
t_eq "$(find "$th/state" -name edge-takeover | wc -l)" 0
# After that the step is what it was: nothing runs, so nothing is started.
printf 'edge --force-recreate\n' >"$owed_file"
rm -f "$T"/calls/docker
push_install 'echo "tehas is up to date!!"'
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "edge: not running on this host, left alone"
t_eq "$(t_calls docker | grep -c '^compose ')" 0

t_case "a takeover that went through leaves no mark"
instance
platform 1 "$ASK"
t_stub docker 'if [ "$1" = ps ]; then case "$4" in *tehas-protolab) printf "c1\n" ;; esac; fi'
t_run "$update"
t_eq "$t_rc" 0
t_eq "$(find "$th/state" -name edge-takeover | wc -l)" 0

t_case "an old project that will not come down leaves no mark"
instance
platform 1 "$ASK"
t_stub docker 'case "$1" in ps) case "$4" in *tehas-protolab) printf "c1\n" ;; esac ;; compose) [ "$2" = -p ] && { echo "cannot remove" >&2; exit 1; } ;; esac'
t_run "$update"
t_eq "$t_rc" 1
t_eq "$(find "$th/state" -name edge-takeover | wc -l)" 0

# The mark is the memory of one half-finished takeover, not a standing order to
# start the router. A person who stopped it on purpose would otherwise have it
# started by a mark some run gave up on, weeks later.
t_case "a mark older than the day it was made is not acted on"
instance
platform 0 'echo "tehas is up to date"'
# Nothing of either project runs: without the mark this step would leave the
# router alone.
t_stub docker
mkdir -p "$th/state"
printf '%s\n' "$(( $(date -u +%s) - 90000 ))" >"$th/state/edge-takeover"
printf 'edge --force-recreate\n' >"$owed_file"
rm -f "$T"/calls/docker
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "the mark of the takeover is"
t_contains "$t_out" "edge: not running on this host, left alone"
t_lacks "$t_out" "finishing the takeover"
t_eq "$(t_calls docker | grep -c '^compose ')" 0
t_eq "$(find "$th/state" -name edge-takeover | wc -l)" 0
t_eq "$(owed_steps)" ""

t_case "a mark of the same day is acted on, and the age comes from the mark"
instance
platform 0 'echo "tehas is up to date"'
mkdir -p "$th/state"
printf '%s\n' "$(( $(date -u +%s) - 3600 ))" >"$th/state/edge-takeover"
printf 'edge --force-recreate\n' >"$owed_file"
rm -f "$T"/calls/docker
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "edge: finishing the takeover from the Protolab's server"
t_contains "$(t_calls docker)" "compose -f $th/edge/router.compose.yaml up -d --force-recreate"
t_eq "$(find "$th/state" -name edge-takeover | wc -l)" 0
t_eq "$(owed_steps)" ""

t_case "a mark with no time on it is honoured once and gets the time of this run"
instance
platform 0 'echo "tehas is up to date"'
mkdir -p "$th/state"
: >"$th/state/edge-takeover"
printf 'edge --force-recreate\n' >"$owed_file"
t_stub docker 'case "$1" in compose) case "$*" in *router.compose.yaml*) echo "port is already allocated" >&2; exit 1 ;; esac ;; esac'
rm -f "$T"/calls/docker
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "edge: finishing the takeover from the Protolab's server"
t_eq "$(owed_steps)" "edge --force-recreate"
t_eq "$(grep -cE '^[0-9]+$' "$th/state/edge-takeover")" 1

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
t_contains "$t_out" "systemd: restarted tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer"
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
t_contains "$t_out" "systemd: restarted tehas-energy.service tehas-top.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer"
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
t_eq "$(owed_steps)" "units tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer
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

t_case "--check reports follow-ups an earlier run left undone, and starts none"
instance
platform 0 'echo "Instance:     $TEHAS_INSTANCE_DIR OK"
echo "Installed:    OK"'
mkdir -p "$th/state"
printf 'edge --force-recreate\n' >"$owed_file"
before=$(head_sha)
t_run "$update --check"
t_eq "$t_rc" 1
t_contains "$t_out" "follow-up(s) an earlier run did not finish"
t_contains "$t_out" "next: edge --force-recreate"
t_contains "$t_out" "Installed:    OK"
t_eq "$(head_sha)" "$before"
t_eq "$(t_calls docker)" ""
# Nothing was started and nothing was taken back off the list.
t_eq "$(owed_steps)" "edge --force-recreate"

t_case "a checkout on a branch that is not the remote's default says so"
instance
platform 0 'echo "installed: nothing"'
push_branch other
git -C "$checkout" fetch -q origin
git -C "$checkout" symbolic-ref refs/remotes/origin/HEAD refs/remotes/origin/main
git -C "$checkout" checkout -q other
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "checkout: $checkout is on branch other; the default branch of origin is main"
t_contains "$t_out" "checkout: other is up to date at $(head_sha)"
t_eq "$(git -C "$checkout" symbolic-ref --quiet --short HEAD)" "other"
t_eq "$(t_calls install)" "install "

t_case "a dirty checkout changes nothing and runs no install.sh"
instance
platform 0 "$ASK"
before=$(state)
echo "half-finished work" >"$checkout/libexec/new-thing"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "has local changes; nothing was changed"
t_contains "$t_out" "libexec/new-thing"
t_eq "$(state)" "$before"
t_eq "$(t_calls install)" ""
t_eq "$(t_calls build)" ""

t_case "a checkout with commits of its own is not fast-forwarded"
instance
platform 0 "$ASK"
echo local >"$checkout/notes"
git -C "$checkout" add -A
git -C "$checkout" commit -qm "local work"
push_one "remote work"
before=$(head_sha)
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "cannot fast-forward to origin/main"
t_eq "$(head_sha)" "$before"
t_eq "$(t_calls install)" ""

t_case "a remote that cannot be reached stops before install.sh"
instance
platform 0 'echo "installed: nothing"'
git -C "$checkout" remote set-url origin "$T/no-such-place"
before=$(head_sha)
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "could not fetch from origin; nothing was changed"
t_eq "$(head_sha)" "$before"
t_eq "$(t_calls install)" ""

t_case "a checkout on a branch with no commit yet says what is missing"
instance
platform 0 'echo "installed: nothing"'
git -C "$checkout" checkout -q --orphan brand-new
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "on branch brand-new with no commit yet; nothing was changed"
t_lacks "$t_out" "cannot fast-forward"
t_eq "$(git -C "$checkout" symbolic-ref --quiet --short HEAD)" "brand-new"
t_eq "$(t_calls install)" ""

t_case "a detached checkout is refused rather than pulled into"
instance
platform 0 'echo "installed: nothing"'
git -C "$checkout" checkout -q --detach HEAD
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "not on a branch"
t_eq "$(t_calls install)" ""

t_case "a failed install stops before the follow-ups"
instance
platform 2 'echo "installed: libexec/dispatch"
echo "tehas: /x is missing from this checkout" >&2
exit 1'
before=$(head_sha)
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "install: install.sh exited 1; no follow-up was run"
# The pull happened: the checkout moved, and nothing after install.sh ran.
[ "$(head_sha)" = "$before" ] && t_bad "the checkout did not move to origin/main"
t_eq "$(t_calls install)" "install "
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""

t_case "invalid instance settings found by install.sh are passed through as 78"
instance
platform 0 'echo "tehas: models.json does not list model x" >&2
exit 78'
t_run "$update"
t_eq "$t_rc" 78
t_contains "$t_out" "install: install.sh exited 78"
t_eq "$(t_calls build)" ""

t_case "settings the pulled checkout adds are answered before anything is installed"
instance
platform 1 "$ASK"
push_settings_change
t_run "$update"
t_eq "$t_rc" 78
t_contains "$t_out" "the checkout this update pulled asks for settings this instance does not have"
t_contains "$t_out" "TEHAS_NEW_SETTING is not set"
t_eq "$(t_calls install)" ""
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""

t_case "a checkout whose config.sh predates the settings helpers defers to install.sh"
instance
platform 1 'echo "installed: nothing"'
push_old_config
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "does not define tehas_load; install.sh will check the settings itself"
t_lacks "$t_out" "asks for settings this instance does not have"
t_eq "$(t_calls install)" "install "

t_case "--check reports what would change and leaves the checkout as it was"
instance
platform 2 'echo "Instance:     $TEHAS_INSTANCE_DIR OK"
echo "Installed:    DRIFT (1 files)"
echo "  differs: '"$th"'/libexec/dispatch"
exit 1'
before=$(head_sha)
t_run "$update --check"
t_eq "$t_rc" 1
t_contains "$t_out" "checkout: would fast-forward"
t_contains "$t_out" "DRIFT (1 files)"
t_lacks "$t_out" "installed:"
# install.sh --check answers 1 when it found drift. That is its report, not a
# failed step: in check mode nothing was meant to run.
t_lacks "$t_out" "no follow-up was run"
t_contains "$t_out" "install: install.sh --check found something to update"
t_lacks "$t_out" "does not fix"
# Nothing moved: no merge, no file changed, nothing started.
t_eq "$(head_sha)" "$before"
t_eq "$(git -C "$checkout" status --porcelain)" ""
t_eq "$(t_calls install)" "install --check"
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""

t_case "--check exits 1 when only the checkout is behind"
instance
platform 2 'echo "Instance:     $TEHAS_INSTANCE_DIR OK"
echo "Installed:    OK"'
before=$(head_sha)
t_run "$update --check"
t_eq "$t_rc" 1
t_contains "$t_out" "checkout: would fast-forward $before"
t_contains "$t_out" "Installed:    OK"
t_eq "$(head_sha)" "$before"
t_eq "$(t_calls install)" "install --check"

t_case "--check names what an update does not fix apart from something to update"
for finding in "Dependencies: NOT OK" "Slice:        MISSING (sudo ./bootstrap.sh slice)" \
  "Login:        NOT CURRENT" "Tests:        FAILED"; do
  instance
  platform 0 'echo "Instance:     $TEHAS_INSTANCE_DIR OK"
echo "'"$finding"'"
echo "Installed:    OK"
exit 1'
  t_run "$update --check"
  t_eq "$t_rc" 1
  t_contains "$t_out" "$finding"
  t_contains "$t_out" "install: install.sh --check found something tehas update does not fix"
  t_lacks "$t_out" "found something to update"
done

t_case "--check on a host that is up to date reports it and exits 0"
instance
platform 0 'echo "Instance:     $TEHAS_INSTANCE_DIR OK"
echo "Installed:    OK"'
before=$(head_sha)
t_run "$update --check"
t_eq "$t_rc" 0
t_contains "$t_out" "checkout: main is up to date at $(head_sha)"
t_eq "$(head_sha)" "$before"
t_eq "$(t_calls install)" "install --check"

t_case "a follow-up that names no step this command knows is said out loud, not run"
instance
platform 0 'echo "next: rm -rf something"
echo "next: docker compose -f /somewhere/else.compose.yaml up -d"'
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: 2 step line(s) name no step this command knows"
t_contains "$t_out" "next: rm -rf something"
t_eq "$(t_calls docker)" ""
t_eq "$(t_calls build)" ""

t_case "a step line whose names are not unit names never reaches systemctl"
instance
platform 0 'echo "next: units --help tehas-dispatch.timer"'
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: 1 step line(s) name no step this command knows"
t_contains "$t_out" "next: units --help tehas-dispatch.timer"
t_eq "$(t_calls systemctl)" ""

t_case "a units step that names no unit is not run"
instance
platform 0 'echo "next: units"'
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: 1 step line(s) name no step this command knows"
t_contains "$t_out" "next: units"
t_eq "$(t_calls systemctl)" ""

t_case "a step asking for an argument it was not given is not run"
instance
platform 0 'echo "next: docs restart"'
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: 1 step line(s) name no step this command knows"
t_eq "$(t_calls docker)" ""

# The flag is part of the step, not decoration. docker compose up -d without
# --force-recreate leaves a running container serving the config it loaded at its
# start - so a bare docs would renew nothing, be counted as done, and leave the run
# green with a changed Caddyfile unapplied.
t_case "a compose step without its flag is not run, and stays owed"
instance
platform 0 'echo "next: docs"'
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: 1 step line(s) name no step this command knows"
t_contains "$t_out" "next: docs"
t_eq "$(t_calls docker)" ""
t_eq "$(owed_steps)" "docs"

t_case "install.sh's diagnostics are not read as follow-ups"
instance
platform 0 'echo "installed: libexec/dispatch"
echo "next: sandbox" >&2
echo "tehas: a step on this host did not run" >&2'
t_run "$update"
t_eq "$t_rc" 0
# stderr stays stderr, and is not parsed for steps: no build ran, and no WARN
# claims install.sh asked for one.
t_contains "$t_out" "tehas: a step on this host did not run"
t_contains "$t_out" "next: sandbox"
t_lacks "$t_out" "WARN:"
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""

t_case "a status of install.sh this command does not know is reported as 1"
instance
platform 0 'echo "bash: Permission denied" >&2
exit 126'
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "install: install.sh exited 126; no follow-up was run"
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""

t_case "a step recorded under the Protolab's old id renews the edge router"
instance
platform 1 'echo "installed: nothing"'
mkdir -p "$th/state"
printf 'protolab --force-recreate\n' >"$owed_file"
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "edge: updating the container"
t_contains "$(t_calls docker)" "compose -f $th/edge/router.compose.yaml up -d --force-recreate"
t_eq "$(owed_steps)" ""

t_case "a compose file that is not installed stops the update"
instance
ask='echo "next: sandbox"
echo "next: edge --force-recreate"'
platform 1 "$ask"
rm "$th/edge/router.compose.yaml"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "edge: $th/edge/router.compose.yaml is not installed, left alone"
t_lacks "$t_out" "edge: updating the container"
t_eq "$(t_calls docker)" ""
# The steps run in the order lib/update-steps.sh lists them: the build that came
# before the edge router ran, and the failure stopped the run after it.
t_eq "$(t_calls build)" "build"
t_eq "$(owed_steps)" "edge --force-recreate"

t_case "a sandbox build that is not installed stops the update"
instance
platform 0 'echo "next: sandbox"'
chmod -x "$th/sandbox/build.sh"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "sandbox: $th/sandbox/build.sh is not installed"
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""

t_case "a branch origin does not have stops before install.sh"
instance
platform 0 'echo "installed: nothing"'
git -C "$checkout" checkout -q -b local-only
before=$(head_sha)
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "checkout: origin has no branch local-only in $checkout; nothing was changed"
t_eq "$(git -C "$checkout" symbolic-ref --quiet --short HEAD)" "local-only"
t_eq "$(head_sha)" "$before"
t_eq "$(t_calls install)" ""

# The record is the memory an unfinished run leaves behind, so it is read as what
# it says or not at all. A file this account cannot open is not an answer of
# "nothing owed": the next write would replace steps somebody else recorded with
# what this run happened to know.
t_case "a follow-up record that cannot be read stops the update before anything runs"
instance
platform 1 "$ASK"
before=$(state)
mkdir -p "$th/state"
printf 'sandbox\ndocs --force-recreate\n' >"$owed_file"
chmod 000 "$owed_file"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "cannot read the follow-up record $owed_file"
t_contains "$t_out" "nothing was run"
# Nothing ran: no merge, no install.sh, no step.
t_eq "$(state)" "$before"
t_eq "$(t_calls install)" ""
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""
# The steps that were owed are still there.
chmod 644 "$owed_file"
t_eq "$(owed_steps)" "sandbox
docs --force-recreate"

t_case "a record that turned unreadable after install.sh stops the follow-ups"
instance
platform 1 'echo "next: sandbox"
chmod 000 "$TEHAS_HOME/state/update-pending"'
mkdir -p "$th/state"
printf 'docs --force-recreate\n' >"$owed_file"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "cannot be read; no follow-up was run"
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""
chmod 644 "$owed_file"
t_eq "$(owed_steps)" "docs --force-recreate"

t_case "a record that cannot be written stops the run before any step"
instance
platform 1 'echo "next: sandbox"'
mkdir -p "$th/state"
# The lock is there from an earlier run; only the record cannot be written.
: >"$th/state/update.lock"
chmod 500 "$th/state"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "could not make a temporary file beside $owed_file"
t_eq "$(t_calls build)" ""
chmod 755 "$th/state"

t_case "a state directory where the lock cannot be made stops the run before git"
instance
platform 1 'echo "next: sandbox"'
before=$(state)
mkdir -p "$th/state"
chmod 500 "$th/state"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "tehas update: cannot open $th/state/update.lock; nothing was changed"
t_eq "$(state)" "$before"
t_eq "$(t_calls install)" ""
chmod 755 "$th/state"

t_case "a failed run leaves the record, and nothing else, in state/"
instance
platform 1 "$ASK"
docker_no_answer
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "2 of 4 follow-up(s) are not done"
# Each rewrite goes through a temporary file moved over the record, so a run that
# died mid-write leaves the steps already owed there rather than half of a new list
# - and a run that finished leaves no stray file. The lock is kept: removing it
# would let a run that opened it a moment before lock a file no longer there.
t_eq "$(ls -A "$th/state")" "update-pending
update.lock"
t_eq "$(owed_steps)" "docs --force-recreate
edge --force-recreate"

# The seam between the two commands, checked against its producer: this
# repository's real install.sh, run by tehas update in the stubbed host.
t_case "the real install.sh through tehas update runs its follow-ups in its order"
instance_models
real_platform
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "installed: $th/libexec/update"
t_contains "$t_out" "systemd: restarted tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer"
t_contains "$t_out" "docs: updating the container"
t_contains "$t_out" "edge: updating the container"
# The order install.sh prints is the order an update runs, and nothing is left
# looking like work for a person.
t_eq "$(step_order)" "systemd: restarted
sandbox: building
docs: updating
edge: updating"
t_lacks "$t_out" "next:"
t_eq "$(owed_steps)" ""
# Nothing of this host's real /etc or /usr/local/bin was read: the scratch marker
# is not there, so install.sh says nothing about root's copy.
t_lacks "$t_out" "Login:"

# The same seam with systemd refusing the reload: the step that restarts the
# units stops, so the run ends red with everything owed rather than green with
# unit files systemd never read.
t_case "changed unit files are not reported as applied when systemd did not reload"
instance_models
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
t_eq "$(owed_steps)" "$(next_lines)"

# The next run on the same host, with systemd answering: install.sh has nothing
# new to report - its one-shot list was spent by the run that copied the files -
# and the steps it recorded then are finished now.
t_stub systemctl 'if [ "$2" = is-active ]; then echo active; fi'
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "follow-up(s) an earlier run did not finish (see $owed_file)"
t_contains "$t_out" "systemd: restarted tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer"
t_contains "$t_out" "docs: updating the container"
t_eq "$(owed_steps)" ""

# The one file of the host install.sh reports and never touches: root's copy of
# host/login/tehas. Checked here rather than in install_test.sh, because this is
# the fixture with a checkout of its own that holds the real install.sh.
t_case "tehas update prints install.sh's later line for a stale login copy and runs nothing for it"
instance_models
real_platform
login_setup
login_age
before=$(login_state)
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "later: sudo $checkout/bootstrap.sh dev boss"
t_contains "$t_out" "$dev_bin/tehas is not this checkout's host/login/tehas"
# Not a step: nothing was recorded as owed, no sudo ran, and the copy was not touched.
t_lacks "$t_out" "next:"
t_eq "$(owed_steps)" ""
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

# --- pi-rukas follows its branch ---------------------------------------------
# The installed versions.env names a branch, and build.sh is a stub that keeps
# its arguments and exits with the given status.
rukas_build() { # <exit status>
  cat >"$th/sandbox/build.sh" <<EOF
#!/usr/bin/env bash
echo "build \$*" >>"$T/calls/build"
echo "pi-rukas: stub"
exit $1
EOF
  chmod +x "$th/sandbox/build.sh"
  echo "PI_RUKAS_BRANCH=main" >"$th/versions.env"
}

t_case "an up to date checkout still moves the image to the head of pi-rukas's branch"
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

t_case "an image that cannot be moved to the head fails the update"
instance
platform 0
rukas_build 1
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "pi-rukas: the image was not moved to the head of its branch"

t_case "--check says the head moved and builds nothing"
instance
platform 0
rukas_build 1
t_run "$update --check"
t_eq "$t_rc" 1
t_eq "$(t_calls build)" "build --check"
t_contains "$t_out" "pi-rukas: stub"

t_case "a versions.env that names no branch leaves the image alone"
instance
platform 0
echo "PI_RUKAS_COMMIT=0123456789abcdef0123456789abcdef01234567" >"$th/versions.env"
t_run "$update"
t_eq "$t_rc" 0
t_eq "$(t_calls build)" ""

t_done
