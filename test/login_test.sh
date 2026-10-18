#!/usr/bin/env bash
# Tests for the way into a session from another account on the host:
# host/login/tehas, the command such an account types, and libexec/dev-login,
# which sudo runs for it as the tehas user. A stub sudo records what it was
# asked to run; a stub docker records the session's container.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

front="$T_ROOT/host/login/tehas"
entry="$T_ROOT/libexec/dev-login"
etc=$T/etc
mkdir -p "$etc/tehas" "$T/boss/tehas/bin"
echo boss >"$etc/tehas/manager"
printf '#!/bin/sh\necho "the real tehas: $*"\n' >"$T/boss/tehas/bin/tehas"
chmod +x "$T/boss/tehas/bin/tehas"

t_stub sudo 'printf "%s\n" "$@" >"$T/calls/sudo.args"'
t_stub getent 'case "$*" in "passwd boss") echo "boss:x:1001:1001::$T/boss:/bin/bash" ;; *) exit 2 ;; esac'
t_stub id 'case "$*" in -un) echo alice ;; *) exit 1 ;; esac'
printf '[user]\n\tname = Alice Example\n\temail = alice@example.com\n' >"$T/home/.gitconfig"
sudo_args() { cat "$T/calls/sudo.args" 2>/dev/null; }
again() { rm -f "$T"/calls/*; }

# --- the command another account types ----------------------------------------
T_ENV=(TEHAS_ETC="$etc")

t_case "tehas dev asks sudo for the session, as the tehas user, with the caller's own git identity"
t_run "$front dev"
t_eq "$t_rc" 0
t_eq "$(sudo_args | tr '\n' '|')" "-H|-u|boss|--|$T/boss/tehas/libexec/dev-login|Alice Example <alice@example.com>|--|"

t_case "what follows -- goes along for pi"
again
t_run "$front dev -- -r --model 'a b'"
t_eq "$(sudo_args | tail -n 4 | tr '\n' '|')" "--|-r|--model|a b|"

t_case "the identity is the account's own, not that of a repository it stands in"
again
git init -q "$T/elsewhere"
git -C "$T/elsewhere" config user.name "Somebody Else"
t_run "cd $T/elsewhere && $front dev"
t_contains "$(sudo_args)" "Alice Example <alice@example.com>"

t_case "a clone cannot be named and an author cannot be given from here"
again
for bad in "dev bob" "dev --author 'Bob <bob@example.com>'" "dev -r"; do
  t_run "$front $bad"
  t_eq "$t_rc" 64
done
t_eq "$(sudo_args)" ""

t_case "any other command is refused: it is the tehas user's to run"
for cmd in dispatch "work 7" update labels status nonsense; do
  t_run "$front $cmd"
  t_eq "$t_rc" 64
  t_contains "$t_out" "boss"
done
t_eq "$(sudo_args)" ""

t_case "help prints the usage and asks sudo for nothing"
for h in "" help --help "dev --help"; do
  t_run "$front $h"
  t_eq "$t_rc" 0
  t_contains "$t_out" "tehas dev"
done
t_eq "$(sudo_args)" ""

t_case "no git identity stops it before sudo"
mv "$T/home/.gitconfig" "$T/home/.gitconfig.kept"
t_run "GIT_CONFIG_NOSYSTEM=1 $front dev"
t_eq "$t_rc" 64
t_contains "$t_out" "git config --global user.name"
t_eq "$(sudo_args)" ""
printf '[user]\n\tname = Alice Example\n' >"$T/home/.gitconfig"
t_run "GIT_CONFIG_NOSYSTEM=1 $front dev"
t_eq "$t_rc" 64
t_eq "$(sudo_args)" ""
mv "$T/home/.gitconfig.kept" "$T/home/.gitconfig"

t_case "a host that was never set up for this says so"
T_ENV=(TEHAS_ETC="$T/nowhere")
t_run "$front dev"
t_eq "$t_rc" 78
t_contains "$t_out" "bootstrap.sh dev"
t_eq "$(sudo_args)" ""
T_ENV=(TEHAS_ETC="$etc")
echo nobody >"$etc/tehas/manager"
t_run "$front dev"
t_eq "$t_rc" 78
t_eq "$(sudo_args)" ""
echo boss >"$etc/tehas/manager"

t_case "the tehas user itself gets its own tehas, whatever the command"
t_stub id 'case "$*" in -un) echo boss ;; *) exit 1 ;; esac'
t_run "$front work 7"
t_eq "$t_out" "the real tehas: work 7"
t_eq "$(sudo_args)" ""
rm -f "$T/bin/id" "$T/bin/getent" "$T/bin/sudo"

# --- what sudo runs, as the tehas user ----------------------------------------
checkout=$T/home/tehas/repos/widget
devs=$T/home/tehas/dev
upstream=$T/git/acme/widget
mkdir -p "$upstream"
git init -q -b main "$upstream"
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m first
git clone -q "$upstream" "$checkout"
# tehas dev starts nothing without the slice: a host that has it.
t_stub docker 'if [ "$1" = info ]; then echo "systemd 2"; exit; fi
printf "%s\n" "$@" >"$T/calls/docker.args.$$"'
t_stub systemctl 'printf "%s\n" FragmentPath=/etc/systemd/system/tehas.slice ActiveState=active MemoryMax=8589934592'
slice_state 0.00
printf '[user]\n\tname = Test Bot\n\temail = bot@example.com\n' >"$T/home/.gitconfig"
# Pi's settings as install.sh leaves them, which every container is given.
mkdir -p "$T/home/.pi/agent"
echo '{}' >"$T/home/.pi/agent/settings.json"
echo '{"providers":{}}' >"$T/home/.pi/agent/models.json"
session_args() { cat "$T"/calls/docker.args.* 2>/dev/null; }
containers() { find "$T/calls" -name 'docker.args.*' | wc -l; }
passed() { session_args | awk 'prev == "-e" { print } { prev = $0 }' | sed -n "s/^$1=//p"; }
t_instance "${T_MINIMAL[@]}"

t_case "the session is in the clone named after the account sudo was asked by"
again
T_ENV=(GH_TOKEN=test-token SUDO_USER=alice "TEHAS_CGROUP_ROOT=$T_CGROUP")
t_run "$entry 'Alice Example <alice@example.com>' --"
t_eq "$t_rc" 0
t_eq "$(containers)" 1
t_eq "$(session_args | awk 'prev == "-w" { print } { prev = $0 }')" "$devs/alice"
t_eq "$(session_args | awk 'prev == "--name" { print } { prev = $0 }')" "tehas-dev-alice"
t_eq "$(passed GIT_CONFIG_VALUE_2)" "Alice Example"
t_eq "$(passed GIT_CONFIG_VALUE_3)" "alice@example.com"
t_eq "$(cat "$devs/alice.author")" "Alice Example <alice@example.com>"

t_case "the arguments after -- reach pi, and only as pi's"
again
t_run "$entry 'Alice Example <alice@example.com>' -- -r bob --author 'X <x@example.com>'"
t_eq "$(session_args | sed -n '/^pi$/,$p' | tr '\n' '|')" "pi|-r|bob|--author|X <x@example.com>|"
t_eq "$(passed GIT_CONFIG_VALUE_2)" "Alice Example"
t_eq "$([ -e "$devs/bob" ] && echo there)" ""

t_case "nothing but an author and pi's arguments is taken"
again
for bad in "" "'Alice Example <alice@example.com>' bob" "bob 'Alice Example <alice@example.com>'" \
  "'Alice Example <alice@example.com>' --author 'X <x@example.com>'" "'not an author' --" "--help"; do
  t_run "$entry $bad"
  t_eq "$t_rc" 64
done
t_eq "$(containers)" 0
t_eq "$([ -e "$devs/bob" ] && echo there)" ""

t_case "it is not for running by hand: without sudo's account there is no session"
T_ENV=(GH_TOKEN=test-token "TEHAS_CGROUP_ROOT=$T_CGROUP")
t_run "$entry 'Alice Example <alice@example.com>' --"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas dev"
t_eq "$(containers)" 0

t_case "an account whose name cannot name a clone is refused"
for login in ../main a.b -x 'a b'; do
  T_ENV=(GH_TOKEN=test-token "SUDO_USER=$login" "TEHAS_CGROUP_ROOT=$T_CGROUP")
  t_run "$entry 'Alice Example <alice@example.com>' --"
  t_eq "$t_rc" 64
done
t_eq "$(containers)" 0

t_case "it does not depend on the directory sudo was called from"
T_ENV=(GH_TOKEN=test-token SUDO_USER=alice "TEHAS_CGROUP_ROOT=$T_CGROUP")
mkdir -p "$T/closed"
t_run "cd $T/closed && chmod 000 . ; $entry 'Alice Example <alice@example.com>' --"
t_eq "$t_rc" 0
t_eq "$(containers)" 1
chmod 700 "$T/closed"

t_done
