#!/usr/bin/env bash
# Tests for libexec/with-settings: the order of the check, the load and the run
# of a console verb, and the refusals of each, against a recording console.
# shellcheck disable=SC2016  # stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

tree=$T/tree
mkdir -p "$tree/libexec" "$tree/calls"
ln -s "$T_ROOT/lib" "$tree/lib"
cp "$T_ROOT/libexec/with-settings" "$tree/libexec/with-settings"
run=$tree/libexec/with-settings

# A console that records its words and what it was given of the settings.
# CHECK is the status of its check, NEEDS the settings its verb cannot run without.
cat >"$tree/libexec/console" <<'CONSOLE'
#!/usr/bin/env bash
if [ "$1" = check ]; then
  echo "$*" >>"$T/calls/console"
  exit "${CHECK:-0}"
fi
if [ "$1" = needs ]; then
  echo "${NEEDS:-}"
  exit 0
fi
echo "$* repo=${TEHAS_REPO:-} home=${TEHAS_HOME:-}" >>"$T/calls/console"
exit "${RC:-0}"
CONSOLE
chmod +x "$tree/libexec/console"

reset() {
  rm -f "$T"/calls/*
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
}
repo=acme/widget

t_case "the check comes first, then the console runs with the settings and every word"
reset
t_run "$run tehas limit set --reason 'a b'"
t_eq "$t_rc" 0
t_eq "$(t_calls console)" "check tehas limit set --reason a b
tehas limit set --reason a b repo=$repo home=$T/home/tehas"

t_case "a setting the verb needs and the instance lacks is 78, and the verb does not run"
reset
T_ENV=(NEEDS="TEHAS_REPO TEHAS_REVIEW_REPO")
t_run "$run tehas limit"
t_eq "$t_rc" 78
t_contains "$t_out" "tehas: TEHAS_REVIEW_REPO is not set (see "
t_eq "$(t_calls console)" "check tehas limit"

t_case "a setting the verb needs and the instance has lets it run"
reset
T_ENV=(NEEDS="TEHAS_REPO TEHAS_CONTROL_ISSUE")
t_run "$run tehas limit"
t_eq "$t_rc" 0

t_case "the status is the console's"
reset
T_ENV=(RC=9)
t_run "$run tehasbot health"
t_eq "$t_rc" 9

t_case "a check that refuses is its own status and the settings are not read"
reset
t_instance 'echo "settings were read" >&2; exit 9'
T_ENV=(CHECK=64)
t_run "$run tehas limit show"
t_eq "$t_rc" 64
t_lacks "$t_out" "settings were read"
t_eq "$(t_calls console)" "check tehas limit show"

t_case "settings that do not load are 78 and the verb never runs"
reset
t_no_instance
t_run "$run tehas limit"
t_eq "$t_rc" 78
t_contains "$t_out" "no instance.env"
t_eq "$(t_calls console)" "check tehas limit"

t_case "malformed settings are 78 too"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_ANALYTICS=sometimes
t_run "$run tehas analytics report"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_ANALYTICS"
t_eq "$(t_calls console)" "check tehas analytics report"

t_case "TEHAS_CONSOLE is the console, though the load clears the name"
reset
cp "$tree/libexec/console" "$T/other"
sed -i 's/^echo "\$\* repo/echo "other $* repo/' "$T/other"
T_ENV=(TEHAS_CONSOLE="$T/other")
t_run "$run tehas limit"
t_eq "$t_rc" 0
t_contains "$(t_calls console)" "other tehas limit repo=$repo"

t_case "no console is 78, names the command and the fix, and reads nothing"
reset
t_instance 'echo "settings were read" >&2; exit 9'
mv "$tree/libexec/console" "$T/held"
t_run "$run tehas limit show"
t_eq "$t_rc" 78
t_eq "$t_out" "tehas limit: no console at $tree/libexec/console: run install.sh"
mv "$T/held" "$tree/libexec/console"

t_done
