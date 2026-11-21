#!/usr/bin/env bash
# Tests for libexec/update, the bash that asks the console whether it answers
# and then execs it. What the console's update does is tested in Rust, in
# console/tests/update*.rs.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_console || t_bad "no console: $t_console_why"
T_APP_WORLD=0
update="$T_ROOT/libexec/update"
t_stub gh
checkout=$T/home/tehas/platform

# A platform checkout and an install.sh that records a run, to show the
# refused update touched neither.
mkdir -p "$checkout"
printf '#!/usr/bin/env bash\necho install >>"%s/install"\n' "$T/calls" >"$checkout/install.sh"
chmod +x "$checkout/install.sh"
git init -q --initial-branch=main "$checkout"
git -C "$checkout" config user.name test
git -C "$checkout" config user.email test@example.com
git -C "$checkout" add -A
git -C "$checkout" commit -qm first
head=$(git -C "$checkout" rev-parse HEAD)

t_case "no console stops it with 78, and one that does not answer with 1 and where to look"
t_instance "${T_MINIMAL[@]}"
T_ENV_ALWAYS=(TEHAS_CONSOLE="$T/no-console" GH_TOKEN=ghs_test)
t_run "$update"
t_eq "$t_rc" 78
t_contains "$t_out" "tehas update: no console at $T/no-console: run install.sh"
t_stub broken-console 'exit 2'
T_ENV_ALWAYS=(TEHAS_CONSOLE="$T/bin/broken-console" GH_TOKEN=ghs_test)
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "the console at $T/bin/broken-console does not answer (exit 2)"
t_contains "$t_out" "troubleshooting"
# The status decides, not the word: a console that says 1 and fails is not asked
# to run the update.
t_stub broken-console 'echo 1; exit 3'
rm -f "$T/calls/broken-console"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "the console at $T/bin/broken-console does not answer (exit 3)"
t_eq "$(t_calls broken-console)" "update api"
t_eq "$(git -C "$checkout" rev-parse HEAD)" "$head"
t_eq "$(t_calls install)" ""

t_done
