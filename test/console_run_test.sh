#!/usr/bin/env bash
# Tests for `console run`, the way into every command under the settings: the
# order of the check, the load and the verb, and that bad settings stop each
# command with 78 before GitHub, Docker or git and before any write (rule 2).
# shellcheck disable=SC2016  # stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

t_console || t_bad "no console: $t_console_why"
t_dispatch || t_bad "no tehas-dispatch: $t_dispatch_why"
tree=$T/tree
mkdir -p "$tree/libexec" "$tree/bin"
ln -s "$T_ROOT/lib" "$tree/lib"
cp "$T_ROOT/bin/tehas" "$T_ROOT/bin/tehasbot" "$tree/bin/"
cp "$T_ROOT"/libexec/* "$tree/libexec/"
cp "$T_CONSOLE" "$tree/libexec/console"
cp "$T_DISPATCH" "$tree/libexec/tehas-dispatch"
tehas=$tree/bin/tehas
bot=$tree/bin/tehasbot
home=$T/home/tehas
for tool in gh docker git; do t_stub "$tool" 'exit 1'; done

reset() {
  rm -rf "$home"
  rm -f "$T"/calls/*
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
}
# Nothing outside was asked and nothing was written.
untouched() {
  t_eq "$(t_calls gh)$(t_calls docker)$(t_calls git)" ""
  t_eq "$(find "$home" -mindepth 1 2>/dev/null | wc -l)" 0
}

# Every command that went through a script or with-settings.
commands=(
  "$tehas state pack -" "$tehas limit" "$tehas pause" "$tehas resume" "$tehas api" "$tehas labels"
  "$tehas analytics report" "$tehas drain" "$tehas history" "$tehas stop 7" "$tehas mcp serve"
  "$tehas check-product" "$tehas init-product"
  "$bot health" "$bot status" "$bot top" "$bot mcp serve" "$bot protolab"
  "$bot preview" "$bot improve" "$bot refresh" "$bot review" "$bot clean" "$bot github-token"
)
for command in "${commands[@]}"; do
  name=${command#"$tree"/bin/}

  t_case "$name with no instance.env is 78, and calls and writes nothing"
  reset
  t_no_instance
  t_run "$command"
  t_eq "$t_rc" 78
  t_contains "$t_out" "no instance.env"
  untouched

  t_case "$name with a setting no rule allows is 78, and calls and writes nothing"
  reset
  t_instance "${T_MINIMAL[@]}" TEHAS_ANALYTICS=sometimes
  t_run "$command"
  t_eq "$t_rc" 78
  t_contains "$t_out" "TEHAS_ANALYTICS"
  untouched

  t_case "$name with an instance file that ends its shell is 78, and calls and writes nothing"
  reset
  t_instance "${T_MINIMAL[@]}" 'exit 3'
  t_run "$command"
  t_eq "$t_rc" 78
  t_contains "$t_out" "could not be read as shell"
  untouched
done

# What each command cannot run without, and the name it stops on.
needs=(
  "TEHAS_REPO|$tehas api" "TEHAS_REPO|$tehas labels" "TEHAS_REPO|$tehas history"
  "TEHAS_REPO|$tehas resume" "TEHAS_REPO|$bot status"
  "TEHAS_REPO|$tehas check-product" "TEHAS_REPO|$tehas init-product"
  "TEHAS_REPO|$bot protolab" "TEHAS_REPO|$bot preview" "TEHAS_REPO|$bot review" "TEHAS_REPO|$bot clean"
)
for row in "${needs[@]}"; do
  command=${row#*|}
  t_case "${command#"$tree"/bin/} with no ${row%%|*} is 78, and calls and writes nothing"
  reset
  t_instance TEHAS_CONTROL_ISSUE=1 TEHAS_ALLOWED_LABELERS=alice \
    PI_ENSEMBLE_SUBAGENT_MODEL=test-model PI_ENSEMBLE_SUBAGENT_PROVIDER=test-provider
  t_run "$command"
  t_eq "$t_rc" 78
  t_contains "$t_out" "${row%%|*} is not set (see "
  untouched
done

t_case "a loop that is on stops on the settings it needs, and one that is off asks for none"
reset
t_instance TEHAS_CONTROL_ISSUE=1 TEHAS_ALLOWED_LABELERS=alice TEHAS_IMPROVE_REPO=acme/notes
t_run "$bot improve"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_REPO is not set (see "
untouched
reset
t_instance TEHAS_CONTROL_ISSUE=1 TEHAS_ALLOWED_LABELERS=alice TEHAS_REFRESH_CONFLICTS=1
t_run "$bot refresh"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_REPO is not set (see "
untouched
reset
t_instance TEHAS_CONTROL_ISSUE=1 TEHAS_ALLOWED_LABELERS=alice
t_run "$bot improve"
t_eq "$t_rc" 0
t_eq "$t_out" "tehas improve: TEHAS_IMPROVE_REPO is not set, the improvement loop is off"
t_run "$bot refresh"
t_eq "$t_rc" 0
t_eq "$t_out" ""
untouched

t_case "the review's gate sits below the settings it needs"
reset
t_run "$bot review"
t_eq "$t_rc" 0
t_eq "$t_out" ""
untouched
t_instance TEHAS_CONTROL_ISSUE=1 TEHAS_ALLOWED_LABELERS=alice
t_run "$bot review"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_REPO is not set (see "

t_case "an instance with previews off is 78 and says so"
reset
t_run "$bot preview"
t_eq "$t_rc" 78
t_eq "$t_out" "tehas: TEHAS_PREVIEWS is not 1: this instance has no previews"
untouched

# A bad argument is 64 and a text 0, whatever the settings hold.
t_case "a check that refuses is its own status and the settings are not read"
reset
t_instance 'echo "settings were read" >&2; exit 9'
for bad in "$tehas limit show" "$tehas pause a b" "$tehas stop" "$tehas check-product --what" "$tehas init-product --what" \
  "$bot preview --what" "$bot improve x" "$bot clean --what" \
  "$bot github-token --print" "$bot review --what" "$bot refresh --what"; do
  t_run "$bad"
  t_eq "$t_rc" 64
  t_lacks "$t_out" "settings were read"
done
untouched

t_case "a check that was the whole answer ends 0, and neither the settings nor the verb are reached"
reset
t_instance 'echo "settings were read" >&2; exit 9'
for help in "$tehas stop 5 --help" "$bot clean -h" "$bot review --help" "$bot refresh --help"; do
  t_run "$help"
  t_eq "$t_rc" 0
  t_lacks "$t_out" "settings were read"
done
untouched

t_case "a word that is not text is refused by the check of its command, else as text"
reset
t_run "$tehas stop \$'\\xff'"
t_eq "$t_rc" 64
t_run "$tehas limit set \$'\\xff'"
t_eq "$t_rc" 2
t_contains "$t_out" "console: an argument is not UTF-8"
untouched

t_case "the status is the verb's"
reset
t_run "$tehas api --json"
t_eq "$t_rc" 0
t_run "$bot refresh --what"
t_eq "$t_rc" 64

# The two the shell hands in, and the tree the console was told it stands in.
recording() {
  printf '%s\n' '#!/usr/bin/env bash' 'umask >>"$T/calls/umask"' 'printf "%s\n" "$*" >>"$T/calls/console"' >"$1"
  chmod +x "$1"
}

t_case "tehas and tehasbot name their libexec, the uid and the bash, and every word"
reset
recording "$T/other"
T_ENV=(TEHAS_CONSOLE="$T/other")
t_run "$tehas limit set --reason 'a b' && $bot clean --dry-run && echo \"\$EUID \$BASH\""
t_eq "$t_rc" 0
say=${t_out##*$'\n'}
t_eq "$(t_calls console)" "run $tree/libexec ${say% *} ${say#* } tehas limit set --reason a b
run $tree/libexec ${say% *} ${say#* } tehasbot clean --dry-run"

t_case "the pages and the previews are written by a umask their server can read"
reset
recording "$T/other"
T_ENV=(TEHAS_CONSOLE="$T/other")
t_run "umask 077; $bot preview; $bot protolab; $bot clean"
t_eq "$(cat "$T/calls/umask")" "0022
0022
0077"

t_case "no console is 78, names the command and the fix, and reads nothing"
reset
t_instance 'echo "settings were read" >&2; exit 9'
mv "$tree/libexec/console" "$T/held"
t_run "$tehas limit show"
t_eq "$t_rc" 78
t_eq "$t_out" "tehas limit: no console at $tree/libexec/console: run install.sh"
t_run "$bot refresh"
t_eq "$t_rc" 78
t_eq "$t_out" "tehas: no console at $tree/libexec/console to load the settings: run install.sh"
mv "$T/held" "$tree/libexec/console"

t_done
