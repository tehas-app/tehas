#!/usr/bin/env bash
# Tests for bin/tehas and bin/tehasbot: which script each command runs, what
# is passed on, and that neither does anything else. They run against a
# scratch copy of the two files with recording scripts in libexec/, and once
# against the real tree to see that every command has its script.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

# A scratch tree: the two commands, and a libexec/ whose scripts record their
# name and arguments and exit with $RC.
tree=$T/tree
mkdir -p "$tree/bin" "$tree/libexec" "$tree/lib"
cp "$T_ROOT/bin/tehas" "$T_ROOT/bin/tehasbot" "$tree/bin/"
# The settings library is there to be read, so that not reading it means something.
cp "$T_ROOT/lib/config.sh" "$tree/lib/config.sh"
# One line per call: the name, then each argument after a "|".
for name in dispatch cycle work plan dev index setup update labels summary status health energy protolab preview improve refresh review clean top top-publish pause resume stop api limit drain analytics-cmd mcp; do
  printf '%s\n' '#!/usr/bin/env bash' "# The text of $name." '#' "# Usage: $name" 'set -u' 'IFS="|"' \
    "printf '%s\\n' \"$name\${*:+|\$*}\" >>\"\$T/calls/libexec\"" 'exit "${RC:-0}"' >"$tree/libexec/$name"
  chmod +x "$tree/libexec/$name"
done
tehas=$tree/bin/tehas
bot=$tree/bin/tehasbot
ran() { cat "$T/calls/libexec" 2>/dev/null || true; }
reset() {
  rm -f "$T"/calls/*
  T_ENV=()
}

t_case "tehas runs the script of each command with the remaining arguments"
reset
t_run "$tehas dispatch --dry-run && $tehas work 7 --restart && $tehas index && $tehas labels"
t_eq "$t_rc" 0
t_eq "$(ran)" "dispatch|--dry-run
work|7|--restart
index
labels"

t_case "tehas dev passes on the name, the author and what follows --"
reset
t_run "$tehas dev alice --author 'A B <a@b.example>' -- -r && $tehas dev"
t_eq "$t_rc" 0
t_eq "$(ran)" "dev|alice|--author|A B <a@b.example>|--|-r
dev"

t_case "tehas plan passes on the turn's arguments"
reset
t_run "$tehas plan 7 --out /tmp/o --session abc-1 --answer /tmp/a"
t_eq "$t_rc" 0
t_eq "$(ran)" "plan|7|--out|/tmp/o|--session|abc-1|--answer|/tmp/a"

t_case "tehas update passes on --check"
reset
t_run "$tehas update --check && $tehas update"
t_eq "$(ran)" "update|--check
update"

t_case "tehas pause passes on the reason and tehas resume takes nothing"
reset
t_run "$tehas pause waiting for the gate && $tehas resume"
t_eq "$t_rc" 0
t_eq "$(ran)" "pause|waiting|for|the|gate
resume"

t_case "tehas stop passes on the issue and --keep-workspace"
reset
t_run "$tehas stop 7 && $tehas stop 7 --keep-workspace"
t_eq "$t_rc" 0
t_eq "$(ran)" "stop|7
stop|7|--keep-workspace"

t_case "tehas status prints; it is not the script that writes the snapshot"
reset
t_run "$tehas status"
t_eq "$(ran)" "summary"

t_case "tehas status --json and tehas api --json pass the flag on"
reset
t_run "$tehas status --json && $tehas api --json"
t_eq "$(ran)" "summary|--json
api|--json"

t_case "tehas analytics runs analytics-cmd, not the program it calls"
reset
t_run "$tehas analytics export copy.db --json && $tehas analytics report"
t_eq "$t_rc" 0
t_eq "$(ran)" "analytics-cmd|export|copy.db|--json
analytics-cmd|report"

t_case "tehas limit and tehas drain pass their arguments on"
reset
t_run "$tehas limit set --cycles 1 --reason 'a b' && $tehas drain --wait 5"
t_eq "$(ran)" "limit|set|--cycles|1|--reason|a b
drain|--wait|5"

t_case "tehas top runs its script with what follows it"
reset
t_run "$tehas top --once && $tehas top"
t_eq "$t_rc" 0
t_eq "$(ran)" "top|--once
top"

t_case "an argument with a space reaches the script as one argument"
reset
t_run "$tehas work 'a b' '' c && $bot energy 'a b'"
t_eq "$(ran)" "work|a b||c
energy|a b"

t_case "the exit status is the script's"
reset
T_ENV=(RC=3)
t_run "$tehas index"
t_eq "$t_rc" 3
t_run "$bot dispatch"
t_eq "$t_rc" 3

t_case "tehas alone, help, -h and --help print the usage and run nothing"
reset
for arg in "" help -h --help; do
  t_run "$tehas $arg 2>/dev/null"
  t_eq "$t_rc" 0
  t_contains "$t_out" "tehas work <issue> [--restart]"
  t_lacks "$t_out" "set -uo pipefail"
done
t_eq "$(ran)" ""

t_case "tehas help <command> prints the header of that command's script"
reset
t_run "$tehas help work"
t_eq "$t_rc" 0
t_contains "$t_out" "The text of work."
t_lacks "$t_out" "set -u"
t_eq "$(ran)" ""

t_case "tehas <command> --help and -h print the same text and run nothing"
reset
for name in dispatch work plan dev index setup update labels status api limit drain top analytics pause resume stop; do
  for flag in --help -h; do
    t_run "$tehas $name $flag 2>/dev/null"
    t_eq "$t_rc" 0
    t_contains "$t_out" "The text of"
  done
done
t_run "$tehas help help"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas help [command]"
t_eq "$(ran)" ""

t_case "--help with another argument is the script's to judge"
reset
t_run "$tehas work --help 7"
t_eq "$(ran)" "work|--help|7"

t_case "an unknown command is exit 64 on standard error and runs nothing"
reset
for arg in bogus --dry-run 7 "''" "help bogus"; do
  t_run "$tehas $arg 2>/dev/null"
  t_eq "$t_rc" 64
  t_eq "$t_out" ""
done
t_run "$tehas bogus"
t_contains "$t_out" "tehas: unknown command: bogus"
t_contains "$t_out" "tehas help [command]"
t_eq "$(ran)" ""

t_case "what only the units run is not a tehas command"
reset
for name in health energy protolab preview improve summary console ../bin/tehasbot; do
  t_run "$tehas $name"
  t_eq "$t_rc" 64
done
t_eq "$(ran)" ""

t_case "tehasbot runs the script of each command with the remaining arguments"
reset
t_run "$bot dispatch && $bot cycle 7 && $bot status && $bot health && $bot energy --every && $bot top --every && $bot protolab && $bot preview && $bot improve"
t_eq "$t_rc" 0
t_eq "$(ran)" "dispatch
cycle|7
status
health
energy|--every
top-publish|--every
protolab
preview
improve"

t_case "tehasbot status writes the snapshot; it is not the script that prints"
reset
t_run "$bot status"
t_eq "$(ran)" "status"

t_case "tehasbot alone and help print the usage; an unknown command is exit 64"
reset
t_run "$bot"
t_eq "$t_rc" 0
t_contains "$t_out" "tehasbot energy [--every]"
t_contains "$t_out" "tehasbot top [--every]"
t_run "$bot help"
t_eq "$t_rc" 0
for name in work plan dev index update labels summary bogus "''"; do
  t_run "$bot $name 2>/dev/null"
  t_eq "$t_rc" 64
  t_eq "$t_out" ""
done
t_run "$bot bogus"
t_contains "$t_out" "tehasbot: unknown command: bogus"
t_eq "$(ran)" ""

t_case "neither command reads the instance settings"
reset
t_instance 'echo "settings were read" >&2; exit 9'
t_run "$tehas index && $bot health"
t_eq "$t_rc" 0
t_lacks "$t_out" "settings were read"

t_case "a tree without libexec is an error, not a guess"
reset
mkdir -p "$T/lone/bin"
cp "$T_ROOT/bin/tehas" "$T_ROOT/bin/tehasbot" "$T/lone/bin/"
t_run "$T/lone/bin/tehas index"
t_eq "$t_rc" 1
t_contains "$t_out" "no libexec directory"
t_run "$T/lone/bin/tehasbot dispatch"
t_eq "$t_rc" 1

# The real tree.
t_case "every command in the usage of tehas has a script that says how to use it"
reset
names=$("$T_ROOT/bin/tehas" help | sed -n 's/^  tehas \([a-z-]*\).*/\1/p' | grep -v -x help | tr '\n' ' ')
t_eq "$names" "dispatch work plan dev index setup update labels check-product init-product status api limit drain history export top analytics mcp pause resume stop "
for name in $names; do
  t_run "$T_ROOT/bin/tehas help $name"
  t_eq "$t_rc" 0
  t_contains "$t_out" "tehas $name"
done

t_case "every command in the usage of tehasbot has an executable script"
names=$("$T_ROOT/bin/tehasbot" help | sed -n 's/^  tehasbot \([a-z-]*\).*/\1/p' | grep -v -x help | tr '\n' ' ')
t_eq "$names" "dispatch cycle status health energy top protolab preview improve refresh review clean mcp github-token "
missing=""
for name in $names; do
  # The command is `top`; its script is top-publish, because libexec/top is the
  # view a person runs.
  script=$name
  [ "$name" = top ] && script=top-publish
  [ -x "$T_ROOT/libexec/$script" ] || missing="$missing $name"
done
t_eq "$missing" ""

t_case "bin holds the two commands and nothing else"
t_eq "$(cd "$T_ROOT/bin" && find . -type f | LC_ALL=C sort | tr '\n' ' ')" "./tehas ./tehasbot "

t_case "every unit runs a command tehasbot knows"
unknown=""
while read -r name; do
  t_run "$bot $name"
  [ "$t_rc" = 0 ] || unknown="$unknown $name"
done < <(grep -h -o 'bin/tehasbot [a-z]*' "$T_ROOT"/host/systemd/*.service | sed 's/.* //')
t_eq "$unknown" ""
# A cycle's unit starts tehasbot cycle through tehas-cycle run, which runs it
# from the cycle's own copy of the code (#989).
t_eq "$(grep -L 'bin/tehasbot ' "$T_ROOT"/host/systemd/*.service)" "$T_ROOT/host/systemd/tehas-cycle@.service"
t_contains "$(grep -h -o 'libexec/tehas-cycle run %i' "$T_ROOT/host/systemd/tehas-cycle@.service")" "tehas-cycle run %i"

t_done
