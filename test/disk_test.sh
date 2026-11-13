#!/usr/bin/env bash
# Tests for `tehas disk`: how full this host is, and what fills it. The
# reading itself, its bounds and its figures are the console's and are tested
# of console/ops/src/disk.rs; what is tested here is the command and the run it
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
# The file's one docker stub answers every question any case asks of the daemon,
# because a stub is one file and a case that stubs the command again replaces it
# for the cases after it: the figures of the cycles below come of the files a
# case writes under $T/fx, each named for the call it answers, so a case says
# what the daemon says without naming a second stub. No file is the daemon
# having no answer for that call, as a host with no container is no listing.
# The free figures come of a file a case may write, for the same reason the
# daemon's answers do: a case that stubs df again replaces this stub for the
# cases after it, and a case that wants no figure says so with the file.
t_stub df '[ -e "$T/fx/df" ] || exit "${DF_RC:-0}"
cat "$T/fx/df"'
# `case` takes the first arm whose pattern fits, so the exact listing of the
# cycles is named before `ps `: the stopped and ended containers a `--full`
# reading asks for are the same question with another answer, and a pattern of
# `ps ` would be met by both.
t_stub docker 'case "$*" in
  *DockerRootDir*) echo /var/lib/docker ;;
  "system df"*) printf "TYPE\tTOTAL\tACTIVE\tSIZE\tRECLAIMABLE\nImages\t2\t1\t1.2GB\t0B (0%%)\n" ;;
  "ps --format {{.Names}}") [ -e "$T/fx/ps" ] || exit "${PS_RC:-0}"; cat "$T/fx/ps" ;;
  "ps "*) [ -e "$T/fx/containers" ] || exit "${PS_RC:-0}"; cat "$T/fx/containers" ;;
  inspect\ --size*) [ -e "$T/fx/inspect" ] || exit "${INSPECT_RC:-0}"; cat "$T/fx/inspect" ;;
  stats\ --no-stream*) [ -e "$T/fx/stats" ] || exit "${STATS_RC:-0}"; cat "$T/fx/stats" ;;
esac'

reset() {
  # The docker and df stubs read a case's answers from $T/fx, so the fixtures
  # go with the call records: a case that names none is a host whose daemon
  # and df have nothing to say, and never reads the answers of the case before
  # it.
  rm -rf "$T"/calls/* "$T/fx" "$th"
  t_instance "${T_MINIMAL[@]}"
  T_ENV=()
}

# t_df: the one df answer every case of this file wants, the 40 GiB free of a
# host with room on it, which is what the reading is expected to print.
t_df() {
  mkdir -p "$T/fx"
  printf '%s\n' \
    "Filesystem 1024-blocks Used Available Capacity Mounted on" \
    '/dev/x 60000000 1 41943040 1% /' >"$T/fx/df"
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
t_df
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
t_df
t_instance "${T_MINIMAL[@]}" TEHAS_SCRATCH_DIR="$T"/scratch
mkdir -p "$T/scratch"
t_run "$disk --json"
t_eq "$t_rc" 0
t_eq "$(jq -s length <<<"$t_out")" 1
t_eq "$(jq -r '.api' <<<"$t_out")" 1
t_eq "$(jq -r '[.filesystems[].paths[]] | sort | @json' <<<"$t_out")" \
  "$(jq -cn --args '$ARGS.positional | sort' "$th" /var/lib/docker /tmp "$T/scratch")"

# The scratch disk of the instance is the fourth file system, and it is named
# only when the settings name one: the same reading with no TEHAS_SCRATCH_DIR
# has three paths, not an entry for a disk the host does not have.
t_case "with no scratch disk in the settings the answer has no entry for one"
reset
t_df
t_run "$disk --json"
t_eq "$(jq -r '[.filesystems[].paths[]] | sort | @json' <<<"$t_out")" \
  "$(jq -cn --args '$ARGS.positional | sort' "$th" /var/lib/docker /tmp)"

# A figure that cannot be read is no figure and a reason, never 0: a person
# who reads 0 of a file system would read plenty where there is no reading.
# How many entries the answer holds is the test host's: the console folds the
# watched paths on their device, so the home, the docker root, /tmp and the
# scratch disk are one entry or four as the mount table makes them. Every
# expectation is read off the answer, never a count of devices.
t_case "a df that answers nothing leaves that file system with no figure and a reason"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_SCRATCH_DIR="$T"/scratch
mkdir -p "$T/scratch"
# No $T/fx/df and DF_RC: df answers nothing, which is the whole of the case.
DF_RC=1 t_run "DF_RC=1 $disk --json"
t_eq "$t_rc" 0
n=$(jq -r '.filesystems | length' <<<"$t_out")
t_eq "$([ "${n:-0}" -ge 1 ] && echo entries)" entries
# Each watched name stands in some entry, whichever entry holds it.
for p in "$th" /var/lib/docker /tmp "$T/scratch"; do
  t_eq "$(jq -r --arg p "$p" '[.filesystems[].paths[]] | index($p) != null' <<<"$t_out")" true
done
# Every entry, however many there are, has no figure and a reason.
t_eq "$(jq -r '[.filesystems[] | select(.freeBytes == null and (.why | type) == "string")] | length' <<<"$t_out")" "$n"

# --full is the reading of what fills them, biggest first, and it is the form
# that asks the daemon the extra questions. The places are the entries of the
# home and of the agent directory and each clone's build output.
t_case "--full names the places that fill the home, biggest first, and the docker figure"
reset
t_df
mkdir -p "$th/logs" "$th/dev/alpha.state/target"
dd if=/dev/zero of="$th/logs/big.log" bs=1k count=200 status=none
dd if=/dev/zero of="$th/dev/alpha.state/target/out" bs=1k count=50 status=none
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
t_df
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

# --cycles is the one form that asks the daemon a container's own figures:
# the bytes of a cycle's writable layer and the bytes it has written. The
# figures themselves are the module's, and are tested of
# console/ops/src/disk/cycles.rs; what is tested here is the command: that the
# answer holds them, that it fits the schema beside which a program reads it,
# that a figure with no reading stays a figure with no reading, and that none
# of this is asked of the daemon when the flag was not given.
t_case "--cycles --json names each running cycle with both of its figures and fits the schema"
reset
t_disk || t_skip "no console: $t_console_why"
t_df
mkdir -p "$T/fx"
printf '%s\n' tehas-work-1872 tehas-work-51x tehas-dev-a tehas-work-5 >"$T/fx/ps"
# The two forms the daemon really prints for these two figures: `SizeRw` of
# `inspect --size` is a bare count of bytes in the order the containers were
# named, and `BlockIO` of `stats` is a decimal size, written over read.
printf '%s\n' 1234567890 2000000000 >"$T/fx/inspect"
printf '%s\n' 'tehas-work-5	1.2GB / 0B' 'tehas-work-1872	0B / 512kB' 'tehas-dev-a	9MB / 9MB' >"$T/fx/stats"
t_run "$disk --cycles --json"
t_eq "$t_rc" 0
t_eq "$(jq -s length <<<"$t_out")" 1
t_eq "$(jq -r '.cycles.state' <<<"$t_out")" read
t_eq "$(jq -r '[.cycles.list[].issue] | @json' <<<"$t_out")" '[5,1872]'
t_eq "$(jq -r '[.cycles.list[].container] | @json' <<<"$t_out")" \
  '["tehas-work-5","tehas-work-1872"]'
t_eq "$(jq -r '[.cycles.list[].writableBytes] | @json' <<<"$t_out")" '[1234567890,2000000000]'
t_eq "$(jq -r '[.cycles.list[].blockWrittenBytes] | @json' <<<"$t_out")" '[1200000000,0]'
# Neither figure is missing, so no cycle carries a reason.
t_eq "$(jq -r '[.cycles.list[] | has("why")] | @json' <<<"$t_out")" '[false,false]'
# The object is what the file beside it says it is: this is the contract a
# program copies.
jq . <<<"$t_out" >"$T/cycles-answer.json"
t_eq "$(jq -r --slurpfile schema "$T_ROOT/api/disk-cycles.schema.json" \
  -f "$T_ROOT/test/schema.jq" "$T/cycles-answer.json")" ""
# Three calls, however many cycles the host runs: the listing, the sizes and
# the stats, each asked once for the whole host.
t_eq "$(t_calls docker | grep -c '^ps --format')" 1
t_eq "$(t_calls docker | grep -c '^inspect --size')" 1
t_eq "$(t_calls docker | grep -c '^stats --no-stream')" 1

# A figure the daemon would not give is no figure and a reason: 0 would be
# read as a cycle that wrote nothing, which is another fact.
t_case "--cycles keeps a figure the daemon would not answer as null with why"
reset
t_df
mkdir -p "$T/fx"
printf '%s\n' tehas-work-7 >"$T/fx/ps"
printf '%s\n' '-' >"$T/fx/inspect"
# No stats line for the one container: the daemon named it and said nothing of
# what it wrote.
printf '%s\n' 'tehas-dev-a	9MB / 9MB' >"$T/fx/stats"
t_run "$disk --cycles --json"
t_eq "$t_rc" 0
t_eq "$(jq -r '.cycles.list[0].writableBytes | type' <<<"$t_out")" null
t_eq "$(jq -r '.cycles.list[0].blockWrittenBytes | type' <<<"$t_out")" null
t_eq "$(jq -r '.cycles.list[0].why' <<<"$t_out")" \
  'writableBytes and blockWrittenBytes: docker inspect --size gave no size / docker stats answered nothing for it'
jq . <<<"$t_out" >"$T/cycles-null.json"
t_eq "$(jq -r --slurpfile schema "$T_ROOT/api/disk-cycles.schema.json" \
  -f "$T_ROOT/test/schema.jq" "$T/cycles-null.json")" ""

# The reading of the cycles is asked for and nothing else: a host with no
# cycle running has an empty list, not an absence, and not a 0 of anything.
t_case "--cycles with no cycle running says so with an empty list and asks the daemon once"
reset
t_df
t_run "$disk --cycles --json"
t_eq "$t_rc" 0
t_eq "$(jq -r '.cycles.state' <<<"$t_out")" read
t_eq "$(jq -r '.cycles.list | @json' <<<"$t_out")" '[]'
t_eq "$(t_calls docker | grep -c '^inspect --size')" 0
t_eq "$(t_calls docker | grep -c '^stats --no-stream')" 0
t_run "$disk --cycles"
t_eq "$t_rc" 0
t_contains "$t_out" "cycles  no cycle is running"
t_eq "$(grep -c '^cycles' <<<"$t_out")" 1

# The listing is the one call whose failure ends the reading: with no names
# there is nothing to ask about, and the other two are not asked.
t_case "--cycles with a docker that fails the listing is unreadable with why, and the command still prints"
reset
t_df
# The env of the run is named to t_run as well as set here: t_run builds its
# environment with `env -i`, so a name the case merely holds is no part of the
# run's environment at all.
PS_RC=1 t_run "PS_RC=1 $disk --cycles --json"
t_eq "$t_rc" 0
t_eq "$(jq -r '.cycles.state' <<<"$t_out")" "unreadable"
t_eq "$(jq -r '.cycles | has("list")' <<<"$t_out")" false
t_eq "$(jq -r '.cycles.why | type' <<<"$t_out")" string
t_eq "$(t_calls docker | grep -c '^inspect --size')" 0
t_eq "$(t_calls docker | grep -c '^stats --no-stream')" 0
t_run "PS_RC=1 $disk --cycles"
t_contains "$t_out" "cycles  no reading: docker answered nothing"

# Without the flag the command is what it always was: no cycles key, and none
# of the three calls made.
t_case "without --cycles the answer has no cycles key and the daemon is asked nothing new"
reset
t_df
mkdir -p "$T/fx"
printf '%s\n' tehas-work-1872 >"$T/fx/ps"
printf '%s\n' 1000000000 >"$T/fx/inspect"
printf '%s\n' 'tehas-work-1872	1.2GB / 0B' >"$T/fx/stats"
t_run "$disk --json"
t_eq "$t_rc" 0
t_eq "$(jq -r 'has("cycles")' <<<"$t_out")" false
t_eq "$(jq -r '[.filesystems[].paths[]] | index("'"$th"'") != null' <<<"$t_out")" true
t_eq "$(t_calls docker | grep -c '^ps --format')" 0
t_eq "$(t_calls docker | grep -c '^inspect --size')" 0
t_eq "$(t_calls docker | grep -c '^stats --no-stream')" 0
t_run "$disk"
t_lacks "$t_out" "cycles"

# --publish keeps the object it has always written: the cycles are a person's
# question, not the cleaner's, and the file the MCP server reads is unchanged.
t_case "--publish writes the file it always wrote, with no cycles in it"
reset
t_df
mkdir -p "$T/fx" "$th/logs"
dd if=/dev/zero of="$th/logs/big.log" bs=1k count=200 status=none
printf '%s\n' tehas-work-1872 >"$T/fx/ps"
printf '%s\n' 1000000000 >"$T/fx/inspect"
printf '%s\n' 'tehas-work-1872	1.2GB / 0B' >"$T/fx/stats"
t_run "$disk --publish --cycles --json"
t_eq "$t_rc" 0
t_eq "$(jq -r '.api' "$th/live/disk.json")" 1
t_eq "$(jq -r 'has("cycles")' "$th/live/disk.json")" false
t_eq "$(jq -r 'has("cycles")' <<<"$t_out")" true
# The published file is the reading it has always been, so the daemon is asked
# the two figures once, for what was printed, and not again for the file.
t_eq "$(t_calls docker | grep -c '^inspect --size')" 1

# The sample a program copies is the shape the command makes.
t_case "api/disk-cycles.sample.json fits the schema"
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_disk || t_skip "no console: $t_console_why"
t_eq "$(jq -r --slurpfile schema "$T_ROOT/api/disk-cycles.schema.json" \
  -f "$T_ROOT/test/schema.jq" "$T_ROOT/api/disk-cycles.sample.json")" ""
# And the schema says the four keys the contract is judged on.
t_eq "$(jq -r '.required | sort | @json' "$T_ROOT/api/disk-cycles.schema.json")" \
  '["api","cycles","filesystems","generatedAt"]'

# The two keys this schema shares with the plain disk answer, `filesystems` and
# `full`, are restated here because api/ has no $ref: they are a copy, and a
# copy is only honest while it still names what the command prints. The
# reading below is the one that prints everything the command knows how to
# print - `--full` for the places and the docker figure, `--publish` for the
# stopped containers, and a df that answers nothing so the entry carries its
# `why` - so a key the code gained and the copy does not name is refused here.
t_case "the shared part of the schema names every key the disk answer can print"
t_disk || t_skip "no console: $t_console_why"
t_df
# One file system with no figure: the answer for it is an entry with a `why`
# and nothing else, which is the key a plain reading never prints.
DF_RC=1 t_run "DF_RC=1 $disk --full --cycles --publish --json"
t_eq "$t_rc" 0
{
  jq . <<<"$t_out"
  [ -e "$th/live/disk.json" ] && jq . "$th/live/disk.json"
} >"$T/disk-shapes.json"
t_eq "$(jq -sr 'length' "$T/disk-shapes.json")" 2
for key in filesystems full; do
  printed=$(jq -r ".[\"$key\"] | if type == \"array\" then .[] else . end | keys_unsorted[]" \
    "$T/disk-shapes.json" | sort -u)
  declared=$(jq -r ".properties[\"$key\"] | (.items.properties // .properties) | keys_unsorted[]" \
    "$T_ROOT/api/disk-cycles.schema.json" | sort -u)
  extra=$(comm -23 <(printf '%s\n' "$printed") <(printf '%s\n' "$declared"))
  [ -z "$extra" ] || t_bad "$key: the answer prints $(printf '%s' "$extra" | tr '\n' ' ') that the schema does not know"
done

# A name the daemon did not make for a cycle of this factory is no cycle: the
# widest issue the platform publishes is nine digits, and a container named
# past that is something else sharing the daemon. Reading it as a cycle would
# print an answer its own published schema refuses.
t_case "--cycles passes over a container named past the widest issue"
reset
t_df
mkdir -p "$T/fx"
printf '%s\n' tehas-work-1000000000 tehas-work-1872 >"$T/fx/ps"
printf '%s\n' 1000000000 2000000000 >"$T/fx/inspect"
printf '%s\n' 'tehas-work-1872	1.2GB / 0B' 'tehas-work-1000000000	1MB / 1MB' >"$T/fx/stats"
t_run "$disk --cycles --json"
t_eq "$t_rc" 0
t_eq "$(jq -r '[.cycles.list[].container] | @json' <<<"$t_out")" '["tehas-work-1872"]'
jq . <<<"$t_out" >"$T/cycles-wide.json"
t_eq "$(jq -r --slurpfile schema "$T_ROOT/api/disk-cycles.schema.json" \
  -f "$T_ROOT/test/schema.jq" "$T/cycles-wide.json")" ""

# The refusals of the run: the settings are read by the console, so settings
# that are malformed stop the run before the host is asked anything, and a
# host where install.sh put no console says so and names the fix.
t_case "an instance.env that starts with a comment still names the scratch disk"
reset
t_df
# Every real instance.env starts with the comments of the manual: the reader
# that stops at the first line with no `=` sees no settings at all, so the
# scratch disk is left out and a malformed value is never refused.
t_instance "# Tehas instance settings. One host, one product repository." \
  "TEHAS_REPO=acme/widget" "# the disk a cycle writes its temporary files to" \
  "" "TEHAS_SCRATCH_DIR=$T/scratch"
mkdir -p "$T/scratch"
t_run "$disk --json"
t_eq "$t_rc" 0
t_eq "$(jq -r '[.filesystems[].paths[]] | sort | @json' <<<"$t_out")" \
  "$(jq -cn --args '$ARGS.positional | sort' "$th" /var/lib/docker /tmp "$T/scratch")"

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
