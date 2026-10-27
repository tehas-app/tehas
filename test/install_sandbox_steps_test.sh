#!/usr/bin/env bash
# Tests for install.sh: what it owes the sandbox image. A changed instance
# sandbox.Dockerfile or image pin owes the build; a host-only pin does not.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

# The instance's own sandbox.Dockerfile is not copied anywhere: the build reads it
# from the instance directory. A change to it owes the sandbox step the same way a
# change to the tracked Dockerfile does, and a host that has not changed it since
# the last install owes nothing for it.
t_case "an instance sandbox.Dockerfile that changed since the last install owes the sandbox step"
fresh
instance
sandbox_file=$TEHAS_INSTANCE_DIR/sandbox.Dockerfile
printf 'ARG TEHAS_BASE\nFROM $TEHAS_BASE\n' >"$sandbox_file"
t_run "$install"
t_eq "$t_rc" 0
# The first install has never seen the file, so it owes the build.
t_contains "$t_out" "next: sandbox"
t_eq "$(printf '%s\n' "$t_out" | sed -n 's/^next: //p' | grep -c '^sandbox$')" 1
t_eq "$(cat "$th/state/update-pending")" "$(next_steps)"
t_eq "$([ -f "$th/state/sandbox.Dockerfile.sha256" ] && echo there)" "there"
# Nothing more to do, and the build is not owed a second time.
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$t_out" "next: sandbox"
t_lacks "$t_out" "sandbox.Dockerfile"
# The same content written again is no change either: what is compared is the
# file, not its mtime.
printf 'ARG TEHAS_BASE\nFROM $TEHAS_BASE\n' >"$sandbox_file"
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$t_out" "next: sandbox"
# A line added is a change the host has to build.
printf 'ARG TEHAS_BASE\nFROM $TEHAS_BASE\nRUN true\n' >"$sandbox_file"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "next: sandbox"
t_contains "$(cat "$th/state/update-pending")" "sandbox"
# --check says the change too: the file is the instance's to keep, so an install
# cannot bring it current and only seeing the same content again clears the drift.
# The build step stays owed after that, which is what the earlier cases of this
# file already prove for every step, so the record is cleared here to let the
# check answer about the files alone.
printf 'ARG TEHAS_BASE\nFROM $TEHAS_BASE\nRUN false\n' >"$sandbox_file"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "changed: $sandbox_file since the last install"
t_run "$install"
t_eq "$t_rc" 0
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 0
t_lacks "$t_out" "sandbox.Dockerfile"
# The file is not copied onto the host: the build reads it from here.
t_eq "$([ -e "$th/sandbox/sandbox.Dockerfile" ] && echo copied)" ""

# Installing a factory unattended: `bootstrap.sh` then `install.sh` run on a
# machine nobody has prepared by hand, with nothing to answer, and again
# afterwards. `--check` exits 0 on a machine installed as the instance says.
# The cases here are the parts a shell test can prove: the run takes no
# answer, the facts of the machine come back on one line, a machine that owes
# a follow-up is not called installed, and what an install refuses is said
# with the name of the thing to fix.
# A change to the host's copy of versions.env is judged by the lines the sandbox
# image reads, not by the file. The cases edit the installed copy under $th, never
# the checkout, so the pins this repository installs stay what they are.
versions_line() { # <name> <value>: one pin in the installed copy, where it was
  local name=$1 value=$2 line rest head at
  line="$T/versions.$name"
  rest="$T/versions.$name.rest"
  head="$T/versions.$name.head"
  if [ -z "$value" ]; then
    # The line is removed altogether: an installed copy that holds none of the
    # image's pins differs from the checkout, which is a change and not a match.
    grep -v "^$name=" "$th/versions.env" >"$line"
  else
    at=$(grep -n "^$name=" "$th/versions.env" | head -n 1 | cut -d: -f1)
    [[ $at =~ ^[0-9]+$ ]] || { t_bad "versions.env has no $name"; return; }
    head -n "$((at - 1))" "$th/versions.env" >"$head"
    tail -n "+$((at + 1))" "$th/versions.env" >"$rest"
    printf '%s=%s\n' "$name" "$value" >"$line"
    cat "$head" "$line" "$rest" >"$line.all"
    mv "$line.all" "$line"
  fi
  mv "$line" "$th/versions.env"
  rm -f "$rest" "$head"
}

t_case "versions.env whose host-only pin changed asks for no sandbox build"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
rm -f "$th/state/update-pending"
# A pin only bootstrap.sh reads: the file is still copied so the host holds
# checkout's pins, and the later: line still names bootstrap.sh user, but the
# image reads nothing here so no build is asked for. The line is printed only on
# a run that copied the file, so this run is the one that reads for it.
versions_line NODE_VERSION 22.22.2
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "installed: $th/versions.env"
t_contains "$t_out" "later: $T_ROOT/bootstrap.sh user   (versions.env changed"
t_lacks "$t_out" "next: sandbox"
t_lacks "$(cat "$th/state/update-pending" 2>/dev/null)" "sandbox"
# The same file read again owes nothing at all: what is compared is its content.
t_run "$install"
t_eq "$t_rc" 0
# The run that copies the file is the one that says the pins moved, so the pin is
# moved once more for it: a run with nothing to copy calls the host current and
# says nothing about a file it did not touch.
versions_line NODE_VERSION 22.22.3
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "installed: $th/versions.env"
t_contains "$t_out" "later: $T_ROOT/bootstrap.sh user   (versions.env changed"
t_lacks "$t_out" "next: sandbox"

t_case "versions.env whose image pin changed asks for the sandbox build"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
rm -f "$th/state/update-pending"
versions_line PLAYWRIGHT_VERSION 1.55.2
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "next: sandbox"
# recorded, not merely printed
t_contains "$(cat "$th/state/update-pending")" "sandbox"
# The two runs of this case ask the same question of the host's copy and get the
# same answer: the pins the image reads are read as they stand when the run is
# asked, before the file is replaced, and the copy this run makes is the one the
# next run reads. So SHELLCHECK_VERSION is gone from the host's copy and
# PLAYWRIGHT_VERSION still holds the old value: the build this run makes reads
# neither of them as the checkout pins them, and says so before it copies.
versions_line SHELLCHECK_VERSION ""
versions_line PLAYWRIGHT_VERSION 1.55.3
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "next: sandbox"
# Nothing is copied now, so the host holds the pins the run above built from, and
# the build is not owed a second time for them.
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$t_out" "next: sandbox"

t_case "a moved jq pin asks for the sandbox build"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
rm -f "$th/state/update-pending"
versions_line JQ_VERSION 1.7.1
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "next: sandbox"

t_case "a fresh install owes the sandbox build whatever its versions.env holds"
fresh
instance
# No installed copy to compare with: a host that has never had one has never had
# a build made from it, so the build is owed rather than guessed at.
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "next: sandbox"

t_done
