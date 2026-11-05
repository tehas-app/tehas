#!/usr/bin/env bash
# Tests for bin/tehas and bin/tehasbot, and the console's front behind them:
# which script each command runs, what is passed on, the texts, and what a
# host with no console gets. They run against a scratch copy of the two files
# with recording scripts and the real console in libexec/, and once against the
# real tree. Which verb of the console a command is, and whether it runs under
# the settings, is pinned beside the source, in console/src/front.rs.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

t_console || t_bad "no console: $t_console_why"

# A scratch tree: the two commands, the console, and a libexec/ whose scripts
# record their name and arguments and exit with $RC. The names of the scripts
# that once only exec'd the console are there too, so a run of one is seen.
tree=$T/tree
mkdir -p "$tree/bin" "$tree/libexec" "$tree/lib"
cp "$T_ROOT/bin/tehas" "$T_ROOT/bin/tehasbot" "$tree/bin/"
cp "$T_CONSOLE" "$tree/libexec/console"
# The settings library is there to be read, so that not reading it means something.
cp "$T_ROOT/lib/config.sh" "$tree/lib/config.sh"
# One line per call: the name, then each argument after a "|".
for name in cycle work index setup update dispatch plan dev improve review check-product github-token; do
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
t_run "$tehas work 7 --restart && $tehas index && $tehas update --check"
t_eq "$t_rc" 0
t_eq "$(ran)" "work|7|--restart
index
update|--check"

t_case "the commands the console runs itself run no script of libexec"
reset
for words in "tehas dispatch --dry-run" "tehas plan 7 --out /tmp/o" "tehas check-product --sandbox" \
  "tehasbot dispatch" "tehasbot improve" "tehasbot review" "tehasbot github-token --check"; do
  t_run "$tree/bin/$words"
done
t_eq "$(ran)" ""

t_case "an argument with a space, or empty, reaches the script as one argument"
reset
t_run "$tehas work 'a b' '' c"
t_eq "$(ran)" "work|a b||c"

t_case "the exit status is the script's"
reset
T_ENV=(RC=3)
t_run "$tehas index"
t_eq "$t_rc" 3
t_run "$bot cycle 7"
t_eq "$t_rc" 3

t_case "tehas alone, help, -h and --help print the usage and run nothing"
reset
for arg in "" help -h --help "help help"; do
  t_run "$tehas $arg 2>/dev/null"
  t_eq "$t_rc" 0
  t_contains "$t_out" "tehas work <issue> [--restart]"
  t_contains "$t_out" "tehas help [command]"
done
t_eq "$(ran)" ""

t_case "tehas help <command> prints the header of that command's script"
reset
t_run "$tehas help work"
t_eq "$t_rc" 0
t_eq "$t_out" "The text of work.

Usage: work"
t_eq "$(ran)" ""

t_case "tehas help <verb> prints the console's text of it"
reset
t_run "$tehas help limit"
t_eq "$t_rc" 0
t_eq "$t_out" "$("$T_CONSOLE" usage limit)"

t_case "tehas <command> --help and -h print its text and run nothing"
reset
for name in dispatch work plan dev index setup update labels status disk state api limit drain top analytics pause resume stop; do
  for flag in --help -h; do
    t_run "$tehas $name $flag 2>/dev/null"
    t_eq "$t_rc" 0
    [ -n "$t_out" ] || t_bad "tehas $name $flag printed nothing"
  done
done
t_eq "$(ran)" ""

t_case "--help with another argument is the script's to judge"
reset
t_run "$tehas work --help 7"
t_eq "$(ran)" "work|--help|7"

t_case "an unknown command is exit 64 on standard error and runs nothing"
reset
for arg in bogus --dry-run 7 "''" "help bogus" "help review"; do
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
for name in health energy protolab preview improve summary console cycle ../bin/tehasbot; do
  t_run "$tehas $name"
  t_eq "$t_rc" 64
done
t_eq "$(ran)" ""

t_case "tehasbot runs the script of cycle with the remaining arguments"
reset
t_run "$bot cycle 7"
t_eq "$t_rc" 0
t_eq "$(ran)" "cycle|7"

t_case "tehasbot alone and help print the usage; an unknown command is exit 64"
reset
for arg in "" help -h --help; do
  t_run "$bot $arg"
  t_eq "$t_rc" 0
  t_contains "$t_out" "tehasbot energy [--every]"
  t_contains "$t_out" "tehasbot top [--every]"
done
for name in work plan dev index update labels summary bogus "''"; do
  t_run "$bot $name 2>/dev/null"
  t_eq "$t_rc" 64
  t_eq "$t_out" ""
done
t_run "$bot bogus"
t_contains "$t_out" "tehasbot: unknown command: bogus"
t_eq "$(ran)" ""

t_case "neither command reads the instance settings to choose"
reset
t_instance 'echo "settings were read" >&2; exit 9'
t_run "$tehas index && $tehas help status && $bot cycle 7 && $bot help"
t_eq "$t_rc" 0
t_lacks "$t_out" "settings were read"

t_case "a console TEHAS_CONSOLE names is the one that runs"
reset
printf '%s\n' '#!/usr/bin/env bash' 'echo "other $*"' >"$T/other-console"
chmod +x "$T/other-console"
T_ENV=(TEHAS_CONSOLE="$T/other-console")
t_run "$tehas status"
t_eq "$t_out" "other front $tree/libexec $(id -u) $T_BASH tehas status"
t_run "$bot clean"
t_eq "$t_out" "other front $tree/libexec $(id -u) $T_BASH tehasbot clean"

t_case "with no console each command is 78 and says to install, and runs nothing"
reset
T_ENV=(TEHAS_CONSOLE="$T/no-console")
for words in "status" "help" "update --help" "work 7" "top --once"; do
  t_run "$tehas $words"
  t_eq "$t_rc" 78
  t_eq "$t_out" "tehas: no console at $T/no-console: run install.sh"
done
for words in "dispatch" "help" "protolab"; do
  t_run "$bot $words"
  t_eq "$t_rc" 78
  t_eq "$t_out" "tehasbot: no console at $T/no-console: run install.sh"
done
t_eq "$(ran)" ""

t_case "with no console tehas setup runs its script, which builds one"
reset
T_ENV=(TEHAS_CONSOLE="$T/no-console")
t_run "$tehas setup --repo acme/widget"
t_eq "$t_rc" 0
t_eq "$(ran)" "setup|--repo|acme/widget"

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
t_case "every command in the usage of tehas has a text that says how to use it"
reset
T_ENV=(TEHAS_CONSOLE="$T_CONSOLE")
names=$(TEHAS_CONSOLE=$T_CONSOLE "$T_ROOT/bin/tehas" help | sed -n 's/^  tehas \([a-z-]*\).*/\1/p' | grep -v -x help | tr '\n' ' ')
t_eq "$names" "dispatch work plan dev index setup update labels panel check-product init-product status api disk limit drain history export state top analytics mcp pause resume stop "
for name in $names; do
  t_run "$T_ROOT/bin/tehas help $name"
  t_eq "$t_rc" 0
  t_contains "$t_out" "tehas $name"
done

t_case "the script tehasbot runs is executable"
t_eq "$([ -x "$T_ROOT/libexec/cycle" ] && echo yes)" "yes"

t_case "bin holds the two commands and nothing else"
t_eq "$(cd "$T_ROOT/bin" && find . -type f | LC_ALL=C sort | tr '\n' ' ')" "./tehas ./tehasbot "

t_case "a cycle's unit runs tehasbot cycle from the cycle's own copy of the code"
# Which command every other unit names is pinned in console/src/front.rs.
t_eq "$(grep -L 'bin/tehasbot ' "$T_ROOT"/host/systemd/*.service)" "$T_ROOT/host/systemd/tehas-cycle@.service"
t_contains "$(grep -h -o 'libexec/tehas-cycle run %i' "$T_ROOT/host/systemd/tehas-cycle@.service")" "tehas-cycle run %i"

t_done
