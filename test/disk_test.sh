#!/usr/bin/env bash
# Tests for `tehas disk`: how full this host is, and what fills it. The
# reading itself, its bounds and its figures are the console's and are tested
# of console/src/disk.rs; what is tested here is the command and the run it
# makes: that the settings of the instance name the file systems the answer
# holds, and that settings that are malformed stop the run before the host is
# asked anything.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

th=$T/home/tehas
# What the reading asks of the host: one df per file system and one docker
# info for the directory of the images. They are stubbed so a case says what
# the host answers and nothing is read of the machine the tests run on.
t_stub df 'echo "Filesystem 1024-blocks Used Available Capacity Mounted on"
echo "/dev/x 60000000 1 41943040 1% /"'
t_stub docker 'case "$*" in *DockerRootDir*) echo /var/lib/docker ;; esac'

reset() {
  rm -rf "$T"/calls/* "$th"
  t_instance "${T_MINIMAL[@]}"
  T_ENV=()
}

# The command as a host has it: the console beside it, where install.sh puts
# it, so a run needs no name in its environment.
T_DISK_TREE=$T/disk-tree
t_disk() { # the command, with the console of this checkout beside it
  [ -x "$T_DISK_TREE/libexec/console" ] && return 0
  t_console || return 1
  mkdir -p "$T_DISK_TREE/libexec" "$T_DISK_TREE/bin"
  ln -sfn "$T_ROOT/lib" "$T_DISK_TREE/lib"
  cp "$T_ROOT/bin/tehas" "$T_DISK_TREE/bin/tehas"
  cp "$T_CONSOLE" "$T_DISK_TREE/libexec/console"
}
disk="$T_DISK_TREE/bin/tehas disk"

t_case "it prints the file systems of the host, one line each, and asks no more of df than one call each"
t_disk || t_skip "no console: $t_console_why"
reset
t_run "$disk"
t_eq "$t_rc" 0
t_contains "$t_out" "disk      $th:"
t_contains "$t_out" "40 GiB of"
t_contains "$t_out" "disk      /var/lib/docker:"
# One reading of the host: one df for each path, and no path asked twice. The
# figures of paths that stand on one file system are one entry, which is the
# grouping the answer is judged on and is the console's to make.
t_eq "$(t_calls df | sort | uniq -d)" ""
t_eq "$(t_calls docker | head -n 1)" "info --format {{.DockerRootDir}}"

t_case "--json prints one object whose filesystems name the home and the scratch disk"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_SCRATCH_DIR="$T"/scratch
mkdir -p "$T/scratch"
t_run "$disk --json"
t_eq "$t_rc" 0
t_eq "$(jq -s length <<<"$t_out")" 1
t_eq "$(jq -r '.api' <<<"$t_out")" 1
t_eq "$(jq -r '[.filesystems[].paths[]] | @json' <<<"$t_out")" \
  "[\"$th\",\"/var/lib/docker\",\"/tmp\",\"$T/scratch\"]"

# The scratch disk of the instance is the fourth file system, and it is named
# only when the settings name one: the same reading with no TEHAS_SCRATCH_DIR
# has three paths, not an entry for a disk the host does not have.
t_case "with no scratch disk in the settings the answer has no entry for one"
reset
t_run "$disk --json"
t_eq "$(jq -r '[.filesystems[].paths[]] | @json' <<<"$t_out")" \
  "[\"$th\",\"/var/lib/docker\",\"/tmp\"]"

# A figure that cannot be read is no figure and a reason, never 0: a person
# who reads 0 of a file system would read plenty where there is no reading.
t_case "a df that answers nothing leaves that file system with no figure and a reason"
reset
t_stub df 'exit 1'
t_run "$disk --json"
t_eq "$t_rc" 0
t_eq "$(jq -r '[.filesystems[].freeBytes] | @json' <<<"$t_out")" '[null,null,null]'
t_eq "$(jq -r '[.filesystems[].why | type] | @json' <<<"$t_out")" '["string","string","string"]'

# --full is the reading of what fills them, biggest first, and it is the form
# that asks the daemon the extra questions. The places are the entries of the
# home and of the agent directory and each clone's build output.
t_case "--full names the places that fill the home, biggest first, and the docker figure"
reset
mkdir -p "$th/logs" "$th/dev/alpha.state/target"
dd if=/dev/zero of="$th/logs/big.log" bs=1k count=200 status=none
dd if=/dev/zero of="$th/dev/alpha.state/target/out" bs=1k count=50 status=none
t_stub docker 'case "$*" in
  *DockerRootDir*) echo /var/lib/docker ;;
  "system df"*) printf "TYPE\tTOTAL\tACTIVE\tSIZE\tRECLAIMABLE\nImages\t2\t1\t1.2GB\t0B (0%%)\n" ;;
  "ps "*) ;;
esac'
t_run "$disk --full --json"
t_eq "$t_rc" 0
t_eq "$(jq -r '.full.places | type' <<<"$t_out")" array
# The entries of the home, and each clone's build output beside its tree: the
# logs directory carries the 200k of the log inside it, and the target is
# named for what the session built.
t_eq "$(jq -r '[.full.places[].path] | any(. == "'"$th"'/logs")' <<<"$t_out")" true
t_eq "$(jq -r '[.full.places[].path] | any(. == "'"$th"'/dev/alpha.state/target")' <<<"$t_out")" true
# Biggest first of the home's own: the logs directory, at 200k, is above the
# 50k build output. The machine's own agent directory is not ours to rank.
t_eq "$(jq -r --arg home "$th" '[.full.places[] | select(.path | startswith($home))][0].path' <<<"$t_out")" "$th/logs"
t_eq "$(jq -r '.full.docker.state' <<<"$t_out")" read

# --publish is the one form that writes, and it writes the same object whole,
# in the file the MCP server reads. The stopped containers belong to that file
# and not to what a person is printed, so it is the only form asked of them.
t_case "--publish writes live/disk.json whole and names the containers left stopped"
reset
mkdir -p "$th/logs"
dd if=/dev/zero of="$th/logs/big.log" bs=1k count=200 status=none
t_run "$disk --publish --json"
t_eq "$t_rc" 0
t_eq "$([ -e "$th/live/disk.json" ] && echo written)" written
t_eq "$(jq -r '.api' "$th/live/disk.json")" 1
t_eq "$(jq -r '.full.stoppedContainers' "$th/live/disk.json")" 0
# The file is one object on one line, and no side file of the run is left.
t_eq "$(jq -s length "$th/live/disk.json")" 1
t_eq "$(find "$th/live" -mindepth 1 | wc -l)" 1

t_case "an unknown argument is 64 and writes nothing"
reset
t_run "$disk --everything"
t_eq "$t_rc" 64
t_eq "$([ -e "$th/live/disk.json" ] && echo written)" ""

# The refusals of the run: the settings are read by the console, so settings
# that are malformed stop the run before the host is asked anything, and a
# host where install.sh put no console says so and names the fix.
t_case "an instance.env that starts with a comment still names the scratch disk"
reset
# Every real instance.env starts with the comments of the manual: the reader
# that stops at the first line with no `=` sees no settings at all, so the
# scratch disk is left out and a malformed value is never refused.
t_instance "# Tehas instance settings. One host, one product repository." \
  "TEHAS_REPO=acme/widget" "# the disk a cycle writes its temporary files to" \
  "" "TEHAS_SCRATCH_DIR=$T/scratch"
mkdir -p "$T/scratch"
t_run "$disk --json"
t_eq "$t_rc" 0
t_eq "$(jq -r '[.filesystems[].paths[]] | @json' <<<"$t_out")" \
  "[\"$th\",\"/var/lib/docker\",\"/tmp\",\"$T/scratch\"]"

t_case "a malformed scratch disk in a commented file is 78 and the host is never asked anything"
reset
t_instance "# Tehas instance settings. One host, one product repository." \
  "TEHAS_REPO=acme/widget" "# not an absolute path" "TEHAS_SCRATCH_DIR=scratch"
t_run "$disk"
t_eq "$t_rc" 78
t_eq "$(t_calls df)" ""
t_contains "$t_out" "TEHAS_SCRATCH_DIR is not an absolute path"

t_case "malformed settings are 78 and the host is never asked anything"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_SCRATCH_DIR=scratch
t_run "$disk"
t_eq "$t_rc" 78
t_eq "$(t_calls df)" ""
t_contains "$t_out" "TEHAS_SCRATCH_DIR is not an absolute path"

t_case "no instance to read the home from is 78 and says to set one up"
reset
# The name of the instance is given and the file it holds is not there: the
# run has no home to read, and reads nothing of the host for it.
t_no_instance
t_run "TEHAS_INSTANCE_DIR=$T/no-instance $disk"
t_eq "$t_rc" 78
t_eq "$(t_calls df)" ""
t_contains "$t_out" "could not be read"

t_done
