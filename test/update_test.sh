#!/usr/bin/env bash
# Tests for bin/tehas-update: the checkout steps, what install.sh is asked to
# do, and which follow-ups an update may run. git runs for real against a bare
# repository in the scratch directory (no network); install.sh, the sandbox
# build, docker and systemctl are stubs that record their arguments.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

update="$T_ROOT/bin/tehas-update"
th=$T/home/tehas
checkout=$th/platform

t_stub docker
# systemctl that answers the question: every unit it is asked about is active.
# is-active names the state on its own line, and only that word means anything
# to the command, so a stub that stays silent is one that gave no answer.
t_stub systemctl 'if [ "$2" = is-active ]; then echo active; fi'
T_ENV=()

# install.sh as this case wants it answered. It records that it ran, so a case
# can tell "ran with no arguments" from "never ran". It lives in the fake
# checkout and is committed with it: an uncommitted stub would read as a local
# change, which is one of the things the command refuses.
write_install() { # <what the stub prints>
  printf '#!/usr/bin/env bash\nprintf "install %%s\\n" "%s" >>"%s/install"\n%s\n' \
    '${*:-}' "$T/calls" "$1" >"$checkout/install.sh"
  chmod +x "$checkout/install.sh"
}

# A platform checkout with a remote beside it, plus the parts of a host an
# update touches. Arguments: how many commits the remote holds that the
# checkout does not, and what install.sh should print.
platform() { # [behind] [install body]
  local n=${1:-0} body=${2-}
  [ -n "$body" ] || body='echo "installed: nothing"'
  rm -rf "$th" "$T/origin" "$T/upstream" "$T/calls"/*
  mkdir -p "$checkout/bin" "$checkout/lib" "$th/sandbox" "$th/edge"
  write_install "$body"
  : >"$checkout/lib/config.sh"
  : >"$checkout/bin/tehas-work"
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
  : >"$th/edge/protolab.compose.yaml"
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

# Where the checkout stands: branch, its commit, what origin has.
state() {
  printf '%s %s %s\n' \
    "$(git -C "$checkout" symbolic-ref --quiet --short HEAD)" \
    "$(git -C "$checkout" rev-parse --short HEAD)" \
    "$(git -C "$checkout" rev-parse --short refs/remotes/origin/main 2>/dev/null || echo none)"
}
head_sha() { git -C "$checkout" rev-parse --short HEAD; }

instance() { t_instance "${T_MINIMAL[@]}"; }

# docker that answers ps with a running container: for every compose file, or
# for the docs server only, which is what a host without a Protolab server
# looks like. The calls are `docker compose -f <file> ps --quiet` and
# `... up -d`, so the subcommand is the fourth argument and the file the third.
docker_up() { t_stub docker 'if [ "$4" = ps ]; then printf "c1\n"; fi'; }
docker_up_docs() { t_stub docker 'if [ "$4" = ps ]; then case "$3" in *docs.compose.yaml ) printf "c1\n" ;; esac; fi'; }
# docker that cannot answer the question at all: no daemon, or a docker with no
# compose plugin. The command must not read that as "no container is running".
docker_no_answer() { t_stub docker 'if [ "$4" = ps ]; then echo "could not connect to the Docker daemon" >&2; exit 125; fi'; }

# systemctl that prints an error and still exits 0, which is what the wrapper
# some containers ship in place of systemd does. Neither an answer of "active"
# nor one of "inactive": the command has to refuse it.
systemctl_no_answer() { t_stub systemctl 'if [ "$2" = is-active ]; then echo "systemd is not running in this container"; exit 0; fi'; }

# install.sh asking for every follow-up it knows: the sandbox build, both
# compose servers, and units to enable and start.
ASK='echo "installed: bin/tehas-update"
echo "next: <TH>/sandbox/build.sh"
echo "next: . <TH>/env.sh && docker compose -f <TH>/edge/docs.compose.yaml up -d"
echo "next: . <TH>/env.sh && docker compose -f <TH>/edge/protolab.compose.yaml up -d --force-recreate"
echo "next: systemctl --user enable --now tehas-dispatch.timer tehas-energy.service"'

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
t_contains "$t_out" "tehas-update: unknown argument: --dryrun"
t_eq "$(t_calls install)" ""

t_case "more than one argument is refused, naming what it was given"
instance
platform 0
t_run "$update --check --help"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas-update: takes at most one argument, got --check --help"
t_eq "$(t_calls install)" ""

t_case "--help prints the usage with no instance and no checkout"
rm -rf "$T/instance" "$th"
t_run "$update --help"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas-update --check"

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

t_case "a fast-forward runs the follow-ups install.sh asked for"
instance
platform 1 "${ASK//<TH>/$th}"
docker_up
before=$(head_sha)
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "checkout: fast-forwarded $before -> $(head_sha) (1 commits)"
t_contains "$t_out" "installed: bin/tehas-update"
t_eq "$(t_calls build)" "build"
t_eq "$(t_calls docker)" "compose -f $th/edge/docs.compose.yaml ps --quiet
compose -f $th/edge/docs.compose.yaml up -d
compose -f $th/edge/protolab.compose.yaml ps --quiet
compose -f $th/edge/protolab.compose.yaml up -d --force-recreate"
t_eq "$(t_calls systemctl)" "--user is-active tehas-dispatch.timer
--user is-active tehas-energy.service
--user restart tehas-dispatch.timer tehas-energy.service"

t_case "a container that is not running is left alone, a unit that is not active too"
instance
docker_up_docs
t_stub systemctl 'if [ "$2" = is-active ]; then
  case "$3" in *.timer ) echo active ;; * ) echo inactive; exit 3 ;; esac
fi'
platform 1 "${ASK//<TH>/$th}"
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "docs: updating the container"
t_contains "$t_out" "protolab: not running on this host, left alone"
t_eq "$(t_calls docker)" "compose -f $th/edge/docs.compose.yaml ps --quiet
compose -f $th/edge/docs.compose.yaml up -d
compose -f $th/edge/protolab.compose.yaml ps --quiet"
t_eq "$(t_calls systemctl)" "--user is-active tehas-dispatch.timer
--user is-active tehas-energy.service
--user restart tehas-dispatch.timer"

t_case "a docker query that fails stops the update instead of reading as not running"
instance
docker_no_answer
platform 1 "${ASK//<TH>/$th}"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "docs: docker compose ps failed; nothing else was run"
t_contains "$t_out" "could not connect to the Docker daemon"
t_lacks "$t_out" "not running on this host"
# install.sh and the sandbox build ran; no container was upped, and the systemd
# step was never reached.
t_eq "$(t_calls docker)" "compose -f $th/edge/docs.compose.yaml ps --quiet"
t_eq "$(t_calls systemctl)" ""

t_case "a systemd query that fails stops the update instead of reading as inactive"
instance
docker_up
t_stub systemctl 'if [ "$2" = is-active ]; then echo "Failed to connect to bus: No such file or directory" >&2; exit 1; fi'
platform 1 "${ASK//<TH>/$th}"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "systemd: systemctl is-active gave no answer for tehas-dispatch.timer; no unit was restarted"
t_contains "$t_out" "Failed to connect to bus"
t_lacks "$t_out" "none of them runs on this host"
# The containers were renewed first; no unit was restarted.
t_eq "$(t_calls docker)" "compose -f $th/edge/docs.compose.yaml ps --quiet
compose -f $th/edge/docs.compose.yaml up -d
compose -f $th/edge/protolab.compose.yaml ps --quiet
compose -f $th/edge/protolab.compose.yaml up -d --force-recreate"
t_eq "$(t_calls systemctl)" "--user is-active tehas-dispatch.timer"

t_case "a systemctl that prints an error and exits 0 is not read as active"
instance
docker_up
systemctl_no_answer
platform 1 "${ASK//<TH>/$th}"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "systemd: systemctl is-active gave no answer for tehas-dispatch.timer; no unit was restarted"
t_contains "$t_out" "systemd is not running in this container"
t_lacks "$t_out" "systemd: restarted"
t_lacks "$t_out" "none of them runs on this host"
# It was asked once, and the answer stopped the run before a restart.
t_eq "$(t_calls systemctl)" "--user is-active tehas-dispatch.timer"

t_case "a checkout on a branch that is not the remote's default says so"
instance
docker_up
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
platform 0 "${ASK//<TH>/$th}"
before=$(state)
echo "half-finished work" >"$checkout/bin/new-thing"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "has local changes; nothing was changed"
t_contains "$t_out" "bin/new-thing"
t_eq "$(state)" "$before"
t_eq "$(t_calls install)" ""
t_eq "$(t_calls build)" ""

t_case "a checkout with commits of its own is not fast-forwarded"
instance
docker_up
platform 0 "${ASK//<TH>/$th}"
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
docker_up
platform 2 'echo "installed: bin/tehas-dispatch"
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
docker_up
platform 0 'echo "tehas: models.json does not list model x" >&2
exit 78'
t_run "$update"
t_eq "$t_rc" 78
t_contains "$t_out" "install: install.sh exited 78"
t_eq "$(t_calls build)" ""

t_case "--check reports what would change and leaves the checkout as it was"
instance
docker_up
platform 2 'echo "Instance:     $TEHAS_INSTANCE_DIR OK"
echo "Installed:    DRIFT (1 files)"
echo "  differs: '"$th"'/bin/tehas-dispatch"
exit 1'
before=$(head_sha)
t_run "$update --check"
t_eq "$t_rc" 1
t_contains "$t_out" "checkout: would fast-forward"
t_contains "$t_out" "DRIFT (1 files)"
t_lacks "$t_out" "installed:"
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

t_case "a follow-up this command does not know stays a line for a person"
instance
docker_up
platform 0 'echo "next: docker compose -f /somewhere/else.compose.yaml up -d"
echo "next: rm -rf something"'
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "next: rm -rf something"
t_eq "$(t_calls docker)" ""

t_case "a systemd line that names no unit stays a line for a person"
instance
docker_up
# The trailing space is the whole line: enable --now with nothing after it.
platform 0 'echo "next: systemctl --user enable --now "'
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "next: systemctl --user enable --now"
t_eq "$(t_calls systemctl)" ""

t_case "a status of install.sh this command does not know is reported as 1"
instance
docker_up
# What bash answers when the file lost its exec bit, as a flattened copy does.
platform 0 'echo "bash: Permission denied" >&2
exit 126'
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "install: install.sh exited 126; no follow-up was run"
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""

t_case "a compose file that is not installed stops the update"
instance
docker_up
ask='echo "next: <TH>/sandbox/build.sh"
echo "next: . <TH>/env.sh && docker compose -f <TH>/edge/protolab.compose.yaml up -d --force-recreate"'
platform 1 "${ask//<TH>/$th}"
rm "$th/edge/protolab.compose.yaml"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "protolab: $th/edge/protolab.compose.yaml is not installed, left alone"
t_lacks "$t_out" "protolab: updating the container"
t_eq "$(t_calls docker)" ""
# Steps run in the order install.sh printed them: the build that came first on
# that list ran, and the failure stopped the run after it.
t_eq "$(t_calls build)" "build"

t_case "a sandbox build that is not installed stops the update"
instance
docker_up
platform 0 "echo \"next: $th/sandbox/build.sh\""
chmod -x "$th/sandbox/build.sh"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "sandbox: $th/sandbox/build.sh is not installed"
t_eq "$(t_calls build)" ""
t_eq "$(t_calls docker)" ""

t_case "a branch origin does not have stops before install.sh"
instance
docker_up
platform 0 'echo "installed: nothing"'
git -C "$checkout" checkout -q -b local-only
before=$(head_sha)
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "checkout: origin has no branch local-only in $checkout; nothing was changed"
t_eq "$(git -C "$checkout" symbolic-ref --quiet --short HEAD)" "local-only"
t_eq "$(head_sha)" "$before"
t_eq "$(t_calls install)" ""

t_done
