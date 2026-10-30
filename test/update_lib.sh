# shellcheck shell=bash
# shellcheck disable=SC2016,SC2034  # stub bodies are expanded when the stub runs; the parts read the variables
# What the tests of libexec/update share: the scratch host, the bare
# repository it clones, the stubs and the helpers. test/update_*_test.sh
# source this.
# The part has sourced test/lib.sh before this.
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

# The host's own `tehas` as the labels step runs it: the file at the path
# $TEHAS_HOME/bin/tehas, not a stub on PATH, which this repository's real
# bin/tehas is ahead of in the checkout a fixture carries. The two helpers name the
# host's bin from the $th the case's file sets, and are called from a case.
labels_put() { # <bin dir> <body> - the host's tehas, and what its labels does
  # Written one line per argument of the outer printf: the nested printf has to
  # reach the file intact, and the stub records a call of two words as one line.
  [ -n "$1" ] || { echo "labels_put: no host bin given" >&2; return 2; }
  mkdir -p "$1"
  printf '%s\n' "#!/usr/bin/env bash" \
    "printf '%s\\n' \"\$*\" >>'$T/calls/tehas'" \
    "$2" >"$1/tehas"
  chmod +x "$1/tehas"
}
labels_ok() { labels_put "$th/bin" 'case "$*" in labels) echo "label tehas:ready" ;; esac'; }
labels_fail() { labels_put "$th/bin" 'case "$*" in labels) echo "tehas labels: could not make tehas:ready" >&2; exit 1 ;; esac'; }

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
platform() { # [behind] [install body] [bin of the host, where its tehas is put]
  local n=${1:-0} body=${2-} bin=${3-$th/bin}
  [ -n "$body" ] || body='echo "installed: nothing"'
  rm -rf "$th" "$T/origin" "$T/upstream" "$T/calls"/*
  systemctl_active
  docker_up
  bound_calls
  mkdir -p "$checkout/libexec" "$checkout/lib" "$th/sandbox" "$th/edge"
  write_install "$body"
  # The host's own `tehas`, as the labels step runs it: by the path
  # $TEHAS_HOME/bin/tehas, and not by name. A stub on PATH would be shadowed by
  # this repository's real bin/tehas, which a case has on it. The third argument
  # is the bin of the host: a fixture that makes the home somewhere else names it.
  labels_put "$th/bin" 'case "$*" in labels) echo "label tehas:ready" ;; esac'
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

# The record of steps owed, written as install.sh writes it. --pending takes the
# record as its whole work list, so a case names the lines it wants run rather
# than the sources an install stub would have changed. platform() wipes the host,
# so the state directory it would hold the record in is made here.
record() {
  mkdir -p "${owed_file%/*}"
  printf '%s\n' "$@" >"$owed_file"
}

# The tehas.rukas label of the sandbox image, as `docker image inspect` answers
# it. The rest of docker keeps answering what platform() gave it: the ps of the
# two compose projects, and the capabilities of a build. "none" is an image with
# no label, or no image at all: docker answers nothing and fails.
rukas_image() { # <commit or none>
  mkdir -p "$T/fx"
  if [ "$1" = none ]; then
    rm -f "$T/fx/label"
  else
    printf '%s\n' "$1" >"$T/fx/label"
  fi
  t_stub docker 'if [ "$1" = ps ]; then printf "c1\n"; fi
if [ "$1" = run ]; then cat "$T_ROOT/test/fixtures/headless-c5ddc0a/capabilities.json"; fi
if [ "$1" = image ]; then case "$*" in
  "image inspect tehas/pi-rukas:release-"*) exit 1 ;;
  *"image inspect"*) [ -e "$T/fx/label" ] || exit 1; cat "$T/fx/label" ;;
esac; fi'
}

# The host's clone of pi-rukas, at a commit of its own, or not there at all. A
# commit of 40 hex digits is made in git, so the case names the label it wants
# and reads back the head the clone came out on - the one thing it may not
# assume is the sha git wrote.
rukas_clone() { # <40 hex digits, or none>
  local dir=$th/repos/pi-rukas
  rm -rf "$dir"
  [ "$1" = none ] && return 0
  [[ $1 =~ ^[0-9a-f]{40}$ ]] || { echo "rukas_clone: $1 is not 40 hex digits" >&2; return 2; }
  mkdir -p "$dir"
  git init -q --initial-branch=main "$dir"
  git -C "$dir" config user.name test
  git -C "$dir" config user.email test@example.com
  echo "pi-rukas" >"$dir/README"
  git -C "$dir" add -A
  # One instant, whoever runs the case: two clones of one commit are one commit,
  # so a case that means them to agree is given one sha back. Named in git's own
  # ISO form - this git refuses the @-seconds form.
  GIT_AUTHOR_DATE=2026-01-01T00:00:00 GIT_COMMITTER_DATE=2026-01-01T00:00:00 \
    git -C "$dir" commit -q --no-gpg-sign -m "pi-rukas at $1"
  printf '%s\n' "$(git -C "$dir" rev-parse HEAD)"
}

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
# shellcheck disable=SC2154  # t_out is set by test/lib.sh, which the part sources first
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

