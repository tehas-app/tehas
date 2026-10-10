#!/usr/bin/env bash
# Tests for bin/tehas and bin/tehasbot: which script each command runs, what
# is passed on, and that neither does anything else. They run against a
# scratch copy of the two files with recording scripts in libexec/, and once
# against the real tree to see that every command has its script.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

# A scratch tree: the two commands, and a libexec/ whose scripts record their
# name and arguments and exit with $RC.
tree=$T/tree
mkdir -p "$tree/bin" "$tree/libexec" "$tree/lib"
cp "$T_ROOT/bin/tehas" "$T_ROOT/bin/tehasbot" "$tree/bin/"
# The settings library is there to be read, so that not reading it means something.
cp "$T_ROOT/lib/config.sh" "$tree/lib/config.sh"
# One line per call: the name, then each argument after a "|".
for name in dispatch work dev index update labels summary status health energy protolab preview; do
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

t_case "tehas update passes on --check"
reset
t_run "$tehas update --check && $tehas update"
t_eq "$(ran)" "update|--check
update"

t_case "tehas status prints; it is not the script that writes the snapshot"
reset
t_run "$tehas status"
t_eq "$(ran)" "summary"

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
for name in dispatch work dev index update labels status; do
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
for name in health energy protolab preview summary console ../bin/tehasbot; do
  t_run "$tehas $name"
  t_eq "$t_rc" 64
done
t_eq "$(ran)" ""

t_case "tehasbot runs the script of each command with the remaining arguments"
reset
t_run "$bot dispatch && $bot status && $bot health && $bot energy --every && $bot protolab && $bot preview"
t_eq "$t_rc" 0
t_eq "$(ran)" "dispatch
status
health
energy|--every
protolab
preview"

t_case "tehasbot status writes the snapshot; it is not the script that prints"
reset
t_run "$bot status"
t_eq "$(ran)" "status"

t_case "tehasbot alone and help print the usage; an unknown command is exit 64"
reset
t_run "$bot"
t_eq "$t_rc" 0
t_contains "$t_out" "tehasbot energy [--every]"
t_run "$bot help"
t_eq "$t_rc" 0
for name in work dev index update labels summary bogus "''"; do
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
names=$("$T_ROOT/bin/tehas" help | sed -n 's/^  tehas \([a-z]*\).*/\1/p' | grep -v -x help | tr '\n' ' ')
t_eq "$names" "dispatch work dev index update labels status "
for name in $names; do
  t_run "$T_ROOT/bin/tehas help $name"
  t_eq "$t_rc" 0
  t_contains "$t_out" "tehas $name"
done

t_case "every command in the usage of tehasbot has an executable script"
names=$("$T_ROOT/bin/tehasbot" help | sed -n 's/^  tehasbot \([a-z]*\).*/\1/p' | grep -v -x help | tr '\n' ' ')
t_eq "$names" "dispatch status health energy protolab preview "
missing=""
for name in $names; do
  [ -x "$T_ROOT/libexec/$name" ] || missing="$missing $name"
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
t_eq "$(grep -L 'bin/tehasbot ' "$T_ROOT"/host/systemd/*.service)" ""

t_done
