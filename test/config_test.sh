#!/usr/bin/env bash
# Tests for lib/config.sh: how instance settings are found, loaded, defaulted
# and validated.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

load='. "$T_ROOT/lib/config.sh"; tehas_load'
# The ceiling as lib/config.sh writes it, so a test never hand-copies it.
plan_max=$(sed -n 's/^[[:space:]]*local TEHAS_PLAN_TIMEOUT_MAX=//p' "$T_ROOT/lib/config.sh")

t_case "a minimal instance loads and derives the checkout path"
t_instance "${T_MINIMAL[@]}"
t_run "$load && echo \"\$TEHAS_WORK_REPO|\$TEHAS_HOST_ADDR|\$TEHAS_CONCURRENCY\""
t_eq "$t_rc" 0
t_eq "$t_out" "$T/home/tehas/repos/widget|127.0.0.1|1"

t_case "settings are exported to child processes"
t_run "$load && bash -c 'echo \$TEHAS_REPO \$PI_ENSEMBLE_SUBAGENT_MODEL \$PI_ENSEMBLE_MAX_WORKSTREAMS'"
t_eq "$t_out" "acme/widget test-model 1"

t_case "a gate gets thirty minutes unless the instance says otherwise, and pi-rukas is told in milliseconds"
t_run "$load && bash -c 'echo \$TEHAS_VERIFY_TIMEOUT \$PI_ENSEMBLE_VERIFY_TIMEOUT_MS \$PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS'"
t_eq "$t_out" "1800 1800000 1800000"
t_instance "${T_MINIMAL[@]}" TEHAS_VERIFY_TIMEOUT=600
t_run "$load && bash -c 'echo \$TEHAS_VERIFY_TIMEOUT \$PI_ENSEMBLE_VERIFY_TIMEOUT_MS \$PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS'"
t_eq "$t_out" "600 600000 600000"
t_instance "${T_MINIMAL[@]}"

t_case "a workaround is named with the release that fixes it, and only a setting the instance has"
t_run "$load && echo \"[\$TEHAS_RUKAS_WORKAROUNDS]\""
t_eq "$t_out" "[]"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_COMMENT_RETENTION=0 TEHAS_WORK_IMAGES=0 \
  "TEHAS_RUKAS_WORKAROUNDS='PI_ENSEMBLE_COMMENT_RETENTION@0.12.80 TEHAS_WORK_IMAGES@0.12.80'"
t_run "$load && echo \"\$TEHAS_RUKAS_WORKAROUNDS\""
t_eq "$t_out" "PI_ENSEMBLE_COMMENT_RETENTION@0.12.80 TEHAS_WORK_IMAGES@0.12.80"
t_instance "${T_MINIMAL[@]}" TEHAS_RUKAS_WORKAROUNDS=PI_ENSEMBLE_COMMENT_RETENTION@0.12.80
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_RUKAS_WORKAROUNDS names PI_ENSEMBLE_COMMENT_RETENTION, which is not set: remove its entry"
t_instance "${T_MINIMAL[@]}" TEHAS_RUKAS_WORKAROUNDS=TEHAS_RUKAS_WORKAROUNDS@1.2
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_instance "${T_MINIMAL[@]}"

t_case "a cycle may go an hour without a line in its logs unless the instance says otherwise"
t_run "$load && echo \$TEHAS_WORK_STALL"
t_eq "$t_out" "3600"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_STALL=2400
t_run "$load && echo \$TEHAS_WORK_STALL"
t_eq "$t_out" "2400"

t_case "a stall limit that is not longer than a gate's run is refused"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_STALL=1800
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_WORK_STALL (1800) is not longer than TEHAS_VERIFY_TIMEOUT (1800)"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_STALL=1000 TEHAS_VERIFY_TIMEOUT=600
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=0"
t_instance "${T_MINIMAL[@]}"

t_case "a cycle's time limit that the unit's own limit would cut is refused"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_TIMEOUT=25201
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_WORK_TIMEOUT (25201) is more than 25200: tehas-cycle@.service stops a cycle after 8 hours"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_TIMEOUT=25200
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=0"
t_instance "${T_MINIMAL[@]}"

t_case "the cycle unit's limit is the longest time limit accepted plus an hour"
t_eq "$(sed -n 's/^RuntimeMaxSec=//p' "$T_ROOT/host/systemd/tehas-cycle@.service")" "8h"

t_case "the plan unit's limit is the longest turn's limit accepted plus the grace"
# The pair that has to fit together: the ceiling tehas_load accepts, and the
# grace lib/planning.sh adds to make the unit's RuntimeMaxSec.
t_eq "$plan_max" "7200"
t_contains "$(cat "$T_ROOT/lib/planning.sh")" "TEHAS_PLAN_TIMEOUT + 900"
t_contains "$(cat "$T_ROOT/host/docs/host.md")" "may be at most $plan_max, 2 hours"
t_contains "$(grep 'at most' "$T_ROOT/instances/example/instance.env")" "at most $plan_max"
# Read from the config the same way tehas does: the largest value accepted, at
# the ceiling, gets a unit bound that is the ceiling plus the grace.
t_instance "${T_MINIMAL[@]}" TEHAS_PLAN_TIMEOUT="$plan_max"
t_run "$load; echo rc=\$? \$((TEHAS_PLAN_TIMEOUT + 900))"
t_eq "$t_out" "rc=0 $((plan_max + 900))"
t_instance "${T_MINIMAL[@]}"

t_case "a planning turn's time limit over the ceiling is refused"
t_instance "${T_MINIMAL[@]}" TEHAS_PLAN_TIMEOUT=$((plan_max + 1))
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_PLAN_TIMEOUT ($((plan_max + 1))) is more than $plan_max: a planning turn may take at most two hours"
t_instance "${T_MINIMAL[@]}" TEHAS_PLAN_TIMEOUT="$plan_max"
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=0"
t_instance "${T_MINIMAL[@]}"

t_case "a planning turn's time limit too long for the arithmetic is refused"
# Past the largest number bash compares, and past any count the ceiling
# allows: the value test has to refuse it rather than wrap.
t_instance "${T_MINIMAL[@]}" TEHAS_PLAN_TIMEOUT=9223372036854775808
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_PLAN_TIMEOUT (9223372036854775808) is more than $plan_max"
t_instance "${T_MINIMAL[@]}" TEHAS_PLAN_TIMEOUT=10000000
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_PLAN_TIMEOUT (10000000) is more than $plan_max"
t_instance "${T_MINIMAL[@]}"

t_case "the poll's unit has a finite limit, twice the lock's age at which a poll warns"
t_eq "$(sed -n 's/^TimeoutStartSec=//p' "$T_ROOT/host/systemd/tehas-dispatch.service")" "1h"
t_contains "$(cat "$T_ROOT/libexec/dispatch")" "bound=1800"

t_case "one cycle at a time may run several workstreams, and pi-rukas is told the same number"
t_instance "${T_MINIMAL[@]}" TEHAS_CONCURRENCY=4 TEHAS_TICKET_CONCURRENCY=1
t_run "$load && echo \"\$TEHAS_CONCURRENCY \$TEHAS_TICKET_CONCURRENCY \$PI_ENSEMBLE_MAX_WORKSTREAMS\""
t_eq "$t_rc" 0
t_eq "$t_out" "4 1 4"

t_case "several cycles and several workstreams are refused: one axis of parallelism"
t_instance "${T_MINIMAL[@]}" TEHAS_CONCURRENCY=2 TEHAS_TICKET_CONCURRENCY=3
t_run "$load"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_CONCURRENCY (2) and TEHAS_TICKET_CONCURRENCY (3) are both above 1: choose one"

t_case "an instance line for pi-rukas's workstreams is replaced by tehas' value"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_MAX_WORKSTREAMS=4
t_run "$load && echo \"\$TEHAS_CONCURRENCY \$PI_ENSEMBLE_MAX_WORKSTREAMS\""
t_eq "$t_out" "1 1"

t_case "the sandbox safety switches are always on"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_NO_DOCKER_SOCKET=0 PI_ENSEMBLE_NO_SSH=
t_run "$load && echo \"\$PI_ENSEMBLE_NO_DOCKER_SOCKET \$PI_ENSEMBLE_NO_SSH\""
t_eq "$t_out" "1 1"

t_case "instance.env beats the environment"
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_REPO=other/thing)
t_run "$load && echo \"\$TEHAS_REPO\""
t_eq "$t_out" "acme/widget"

t_case "a setting only in the environment is not used"
t_instance TEHAS_CONTROL_ISSUE=1
T_ENV=(TEHAS_REPO=other/thing TEHAS_INDEXING=1 PI_ENSEMBLE_DEBUG=1)
t_run "$load && echo \"[\${TEHAS_REPO:-}] [\$TEHAS_INDEXING] [\${PI_ENSEMBLE_DEBUG:-}]\""
t_eq "$t_out" "[] [0] []"

t_case "a cycle is given no images unless the instance says so"
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_WORK_IMAGES=1)
t_run "$load && echo \"\$TEHAS_WORK_IMAGES\""
t_eq "$t_out" "0"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1
t_run "$load && echo \"\$TEHAS_WORK_IMAGES\""
t_eq "$t_out" "1"
T_ENV=()

t_case "images are not described unless the instance says so, and only where it gives images"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1
t_run "$load && echo \"\$TEHAS_WORK_DESCRIBE\""
t_eq "$t_out" "0"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1 TEHAS_WORK_DESCRIBE=1
t_run "$load && echo \"\$TEHAS_WORK_DESCRIBE\""
t_eq "$t_out" "1"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_DESCRIBE=1
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "tehas: TEHAS_WORK_DESCRIBE is 1 and TEHAS_WORK_IMAGES is not"

t_case "comments are not read unless the instance says so, and only where it gives images or files"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1
t_run "$load && echo \"\$TEHAS_WORK_COMMENTS\""
t_eq "$t_out" "0"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_FILES=1 TEHAS_WORK_COMMENTS=1
t_run "$load && echo \"\$TEHAS_WORK_COMMENTS\""
t_eq "$t_out" "1"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_COMMENTS=1
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "tehas: TEHAS_WORK_COMMENTS is 1 and neither TEHAS_WORK_IMAGES nor TEHAS_WORK_FILES is"

t_case "a cycle is given no text files unless the instance says so, and they need no images"
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_WORK_FILES=1)
t_run "$load && echo \"\$TEHAS_WORK_FILES\""
t_eq "$t_out" "0"
T_ENV=()
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_FILES=1
t_run "$load && echo \"\$TEHAS_WORK_FILES \$TEHAS_WORK_IMAGES\""
t_eq "$t_out" "1 0"

t_case "the test hooks survive from the environment"
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_WORK_CMD=fake-work TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index
  TEHAS_CYCLE_START_CMD=fake-start TEHAS_CYCLE_START_WAIT=2 TEHAS_PREFLIGHT_WAIT=3 TEHAS_STOP_WAIT=4 TEHAS_FETCH_WAIT=5
  TEHAS_CYCLE_RETRY_WAIT=6 TEHAS_CYCLE_STOP_GH_WAIT=7 TEHAS_CONSOLE_WAIT=8 TEHAS_CYCLE_STOP_CMD=fake-stop
  TEHAS_TIDY_CMD=fake-tidy TEHAS_TIDY_WAIT=9 TEHAS_ANALYTICS_WAIT=1)
t_run "$load && echo \"\$TEHAS_WORK_CMD \$TEHAS_STATUS_CMD \$TEHAS_INDEX_CMD \$TEHAS_CYCLE_START_CMD \$TEHAS_CYCLE_START_WAIT \$TEHAS_PREFLIGHT_WAIT \$TEHAS_STOP_WAIT \$TEHAS_FETCH_WAIT \$TEHAS_CYCLE_RETRY_WAIT \$TEHAS_CYCLE_STOP_GH_WAIT \$TEHAS_CONSOLE_WAIT \$TEHAS_CYCLE_STOP_CMD \$TEHAS_TIDY_CMD \$TEHAS_TIDY_WAIT \$TEHAS_ANALYTICS_WAIT\""
t_eq "$t_out" "fake-work fake-status fake-index fake-start 2 3 4 5 6 7 8 fake-stop fake-tidy 9 1"
T_ENV=()

t_case "a missing instance.env returns 78 and names the directory"
rm -f "$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "tehas: no instance.env in $T/instance"

t_case "a failed load does not end the shell that sourced it"
t_run "$load; echo still-here"
t_contains "$t_out" "still-here"

t_case "the instance-dir pointer file is honoured"
t_instance "${T_MINIMAL[@]}"
mkdir -p "$T/home/.config/tehas"
echo "$T/instance" >"$T/home/.config/tehas/instance-dir"
saved=$TEHAS_INSTANCE_DIR
unset TEHAS_INSTANCE_DIR
t_run "$load && echo \"\$TEHAS_REPO \$TEHAS_INSTANCE_DIR\""
t_eq "$t_out" "acme/widget $T/instance"
rm -f "$T/home/.config/tehas/instance-dir"

t_case "without a pointer the default directory is ~/.config/tehas"
t_run "$load; echo rc=\$?"
t_contains "$t_out" "no instance.env in $T/home/.config/tehas"
export TEHAS_INSTANCE_DIR=$saved

t_case "tehas_require names an unset setting and exits 78"
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$load && tehas_require TEHAS_CONTROL_ISSUE TEHAS_REPO; echo not-reached"
t_eq "$t_rc" 78
t_contains "$t_out" "tehas: TEHAS_REPO is not set (see $T/instance/instance.env)"
t_lacks "$t_out" "not-reached"

t_case "tehas_require rejects an empty setting"
t_instance "${T_MINIMAL[@]}" TEHAS_ALLOWED_LABELERS=
t_run "$load && tehas_require TEHAS_ALLOWED_LABELERS"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_ALLOWED_LABELERS is not set"

t_case "the instance settings every command requires are one list"
t_instance "${T_MINIMAL[@]}"
t_run "$load && tehas_require_instance_settings && echo ok"
t_eq "$t_rc" 0
t_contains "$t_out" "ok"

for missing in TEHAS_REPO TEHAS_CONTROL_ISSUE TEHAS_ALLOWED_LABELERS \
  PI_ENSEMBLE_SUBAGENT_MODEL PI_ENSEMBLE_SUBAGENT_PROVIDER; do
  t_case "tehas_require_instance_settings stops an instance without $missing"
  t_instance "${T_MINIMAL[@]}"
  grep -v "^$missing=" "$TEHAS_INSTANCE_DIR/instance.env" >"$T/instance.env"
  mv "$T/instance.env" "$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load && tehas_require_instance_settings; echo not-reached"
  t_eq "$t_rc" 78
  t_contains "$t_out" "$missing is not set"
done

for bad in "TEHAS_REPO=widget" "TEHAS_REPO=a/b/c" "TEHAS_CONTROL_ISSUE=abc" "TEHAS_CONTROL_ISSUE=0" \
  "TEHAS_ALLOWED_LABELERS=alice;rm" "TEHAS_ALLOWED_LABELERS=alice  bob" "TEHAS_ALLOWED_LABELERS=alice " \
  "TEHAS_ALLOWED_LABELERS= alice" "PI_ENSEMBLE_SUBAGENT_MODEL=two words"; do
  t_case "tehas_require rejects $bad"
  # Written quoted so a space or semicolon reaches the variable intact.
  printf '%s\n' "${T_MINIMAL[@]}" "${bad%%=*}='${bad#*=}'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load && tehas_require ${bad%%=*}"
  t_eq "$t_rc" 78
  t_contains "$t_out" "tehas: ${bad%%=*} is not valid"
done

for bad in "TEHAS_CONCURRENCY=0" "TEHAS_TICKET_CONCURRENCY=0" "TEHAS_TICKET_CONCURRENCY=two" "TEHAS_MAX_AWAITING=-1" "TEHAS_WORK_TIMEOUT=soon" "TEHAS_WORK_STALL=0" "TEHAS_WORK_STALL=1h" "TEHAS_HOST_ADDR=not an address" \
  "TEHAS_BRANCH=two words" "TEHAS_INDEXING=yes" "TEHAS_WORK_IMAGES=yes" "TEHAS_WORK_IMAGES=2" "TEHAS_WORK_DESCRIBE=yes" "TEHAS_WORK_FILES=yes" "TEHAS_WORK_FILES=2" "TEHAS_WORK_COMMENTS=yes" "TEHAS_TOP=yes" "TEHAS_TOP=2" "TEHAS_ANALYTICS_SRC=http://plain.example/ea.js" \
  "TEHAS_ANALYTICS_SRC=https://a.example/x.js\" onload=\"x" "TEHAS_ANALYTICS_SITE=two words" \
  "TEHAS_ANALYTICS_COOKIE=yes" "TEHAS_THEME=neon" "TEHAS_THEME=../pixel" "TEHAS_LAYOUT=game" "TEHAS_LAYOUT=../business" "TEHAS_PROTOLAB_URL=https://lab.example/" "TEHAS_PROTOLAB_URL=lab.example" \
  "TEHAS_PLAN_MAX_TURNS=0" "TEHAS_PLAN_TIMEOUT=1h" "TEHAS_VERIFY_TIMEOUT=30m" "TEHAS_VERIFY_TIMEOUT=0" \
  "TEHAS_DOMAIN=https://example.org" "TEHAS_DOMAIN=example.org/" "TEHAS_DOMAIN=Example.org" \
  "TEHAS_DOMAIN=localhost" "TEHAS_DOMAIN=-a.example.org" "TEHAS_PREVIEWS=yes" \
  "TEHAS_PREVIEW_MAX=0" "TEHAS_PREVIEW_MAX=100" "TEHAS_PREVIEW_MIN_FREE=lots" \
  "TEHAS_IMPROVE_REPO=not a repo" "TEHAS_IMPROVE_REPO=acme" "TEHAS_IMPROVE_REPO=acme/factory/more" \
  "TEHAS_IMPROVE_RUKAS_REPO=not a repo" "TEHAS_IMPROVE_RUKAS_REPO=acme" \
  "TEHAS_IMPROVE_MAX_OPEN=0" "TEHAS_IMPROVE_MAX_OPEN=many" \
  "TEHAS_RUKAS_WORKAROUNDS=PI_ENSEMBLE_X" "TEHAS_RUKAS_WORKAROUNDS=PI_ENSEMBLE_X@soon" "TEHAS_RUKAS_WORKAROUNDS=PATH@1.2" \
  "TEHAS_RUKAS_WORKAROUNDS=PI_ENSEMBLE_X@1.2  TEHAS_TOP@1.3" "TEHAS_RUKAS_WORKAROUNDS=PI_ENSEMBLE_X@1" \
  "TEHAS_BRANCH_CHECK=verify&x=1" "TEHAS_BRANCH_CHECK=two  spaces" "TEHAS_BRANCH_CHECK= verify" "TEHAS_BRANCH_CHECK=a\"b" \
  "TEHAS_LOW_GATE=gate" "TEHAS_LOW_GATE=./x/gate" "TEHAS_LOW_GATE=/x/cheap power" "TEHAS_LOW_GATE=/x/gate " "TEHAS_LOW_GATE=~/x/gate" "TEHAS_LOW_GATE=/x/gate\$x"; do
  t_case "loading rejects $bad"
  printf '%s\n' "${T_MINIMAL[@]}" "${bad%%=*}='${bad#*=}'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "tehas: ${bad%%=*} is not valid"
done

t_case "the improvement loop is off unless the instance names a repository"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && set -u && echo \"[\$TEHAS_IMPROVE_REPO]\""
t_eq "$t_out" "[]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_IMPROVE_REPO]\""
t_eq "$t_out" "[acme/factory]"

t_case "a repository for pi-rukas findings is optional, and needs the loop's own"
t_run "$load && set -u && echo \"[\$TEHAS_IMPROVE_RUKAS_REPO]\""
t_eq "$t_out" "[]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory TEHAS_IMPROVE_RUKAS_REPO=acme/rukas \
  >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_IMPROVE_RUKAS_REPO]\""
t_eq "$t_out" "[acme/rukas]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_IMPROVE_RUKAS_REPO=acme/rukas >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_IMPROVE_REPO is not set and TEHAS_IMPROVE_RUKAS_REPO is"

t_case "the loop's limit of open issues is 10 unless the instance says otherwise"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_IMPROVE_MAX_OPEN]\""
t_eq "$t_out" "[10]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_IMPROVE_MAX_OPEN=30 >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_IMPROVE_MAX_OPEN]\""
t_eq "$t_out" "[30]"

t_case "the fixer is off unless the instance names its account, may retry twice and answer three planning questions by default"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && set -u && echo \"[\$TEHAS_FIXER_LOGIN][\$TEHAS_FIXER_MAX_RETRIES][\$TEHAS_FIXER_MAX_PLAN_ANSWERS]\""
t_eq "$t_out" "[][2][3]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_FIXER_LOGIN=tehas-fixer TEHAS_FIXER_MAX_RETRIES=3 TEHAS_FIXER_MAX_PLAN_ANSWERS=1 >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_FIXER_LOGIN][\$TEHAS_FIXER_MAX_RETRIES][\$TEHAS_FIXER_MAX_PLAN_ANSWERS]\""
t_eq "$t_out" "[tehas-fixer][3][1]"

t_case "refreshes are off unless the instance turns them on, two of one pull request by default"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && set -u && echo \"[\$TEHAS_REFRESH_CONFLICTS][\$TEHAS_REFRESH_MAX]\""
t_eq "$t_out" "[0][2]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_REFRESH_CONFLICTS=1 TEHAS_REFRESH_MAX=5 >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_REFRESH_CONFLICTS][\$TEHAS_REFRESH_MAX]\""
t_eq "$t_out" "[1][5]"

for bad in "TEHAS_REFRESH_CONFLICTS=yes" "TEHAS_REFRESH_CONFLICTS=2" "TEHAS_REFRESH_MAX=0" "TEHAS_REFRESH_MAX=-1" "TEHAS_REFRESH_MAX=many"; do
  t_case "loading rejects $bad"
  printf '%s\n' "${T_MINIMAL[@]}" "${bad%%=*}='${bad#*=}'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
done

for bad in "TEHAS_FIXER_LOGIN=two words" "TEHAS_FIXER_LOGIN=-x" "TEHAS_FIXER_MAX_RETRIES=0" "TEHAS_FIXER_MAX_RETRIES=many" \
  "TEHAS_FIXER_MAX_PLAN_ANSWERS=0" "TEHAS_FIXER_MAX_PLAN_ANSWERS=-1" "TEHAS_FIXER_MAX_PLAN_ANSWERS=lots"; do
  t_case "loading rejects $bad"
  printf '%s\n' "${T_MINIMAL[@]}" "${bad%%=*}='${bad#*=}'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "tehas: ${bad%%=*} is not valid"
done

t_case "the fixer is never one of the accounts that authorise work"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_FIXER_LOGIN=alice >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_FIXER_LOGIN (alice) is in TEHAS_ALLOWED_LABELERS"

t_case "analytics is off unless the instance names a script and a site"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_ANALYTICS_SRC][\$TEHAS_ANALYTICS_SITE][\$TEHAS_ANALYTICS_COOKIE]\""
t_eq "$t_out" "[][][0]"

t_case "analytics settings are loaded as written"
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ANALYTICS_SRC=https://stats.example/ea.js" "TEHAS_ANALYTICS_SITE=widget" \
  "TEHAS_ANALYTICS_COOKIE=1" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_ANALYTICS_SRC][\$TEHAS_ANALYTICS_SITE][\$TEHAS_ANALYTICS_COOKIE]\""
t_eq "$t_out" "[https://stats.example/ea.js][widget][1]"

t_case "the pages are pixel unless the instance names another theme"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_THEME]\""
t_eq "$t_out" "[pixel]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_THEME=paper >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_THEME]\""
t_eq "$t_out" "[paper]"

t_case "the front page is the floor unless the instance names the business layout"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_LAYOUT]\""
t_eq "$t_out" "[floor]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_LAYOUT=business TEHAS_THEME=beesknees >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_LAYOUT][\$TEHAS_THEME]\""
t_eq "$t_out" "[business][beesknees]"

t_case "every layout the settings accept has a page"
layouts=$(sed -n '/TEHAS_LAYOUT)/{n;p}' "$T_ROOT/lib/config.sh" | sed 's/^ *case "\$v" in \(.*\)) ;; \*).*/\1/' | tr -d ' ' | tr '|' ' ')
t_contains "$layouts" floor
t_contains "$layouts" business
for l in $layouts; do
  case "$l" in floor) page=index.html ;; *) page=$l.html ;; esac
  [ -f "$T_ROOT/host/fun/$page" ] || t_bad "layout $l has no host/fun/$page"
done

t_case "every theme the settings accept ships, and every theme that ships is accepted"
t_eq "$(sed -n '/TEHAS_THEME)/{n;p}' "$T_ROOT/lib/config.sh" | sed 's/^ *case "\$v" in \(.*\)) ;; \*).*/\1/' | tr -d ' ' | tr '|' '\n' | sort | xargs)" \
  "$(for f in "$T_ROOT"/host/fun/theme-*.css; do f=${f##*/theme-}; echo "${f%.css}"; done | sort | xargs)"

t_case "an analytics script without a site is refused"
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ANALYTICS_SRC=https://stats.example/ea.js" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_ANALYTICS_SITE is not set"

t_case "the factory's own analytics are on unless the instance says off"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_ANALYTICS]\""
t_eq "$t_out" "[on]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_ANALYTICS=off >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_ANALYTICS]\""
t_eq "$t_out" "[off]"

t_case "TEHAS_ANALYTICS from the environment is dropped, as every setting is"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
T_ENV=(TEHAS_ANALYTICS=off)
t_run "$load && echo \"[\$TEHAS_ANALYTICS]\""
t_eq "$t_out" "[on]"
T_ENV=()

for bad in TEHAS_ANALYTICS=1 TEHAS_ANALYTICS=0 TEHAS_ANALYTICS=On TEHAS_ANALYTICS=yes "TEHAS_ANALYTICS=on off"; do
  t_case "loading rejects $bad, and says what it takes"
  printf '%s\n' "${T_MINIMAL[@]}" "${bad%%=*}='${bad#*=}'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "tehas: TEHAS_ANALYTICS is not valid: expected on or off"
done

t_case "the switch of the factory's analytics and the visitor analytics' cookie are checked apart"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_ANALYTICS=off TEHAS_ANALYTICS_COOKIE=1 >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_ANALYTICS][\$TEHAS_ANALYTICS_COOKIE]\""
t_eq "$t_out" "[off][1]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_ANALYTICS_COOKIE=off >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load; echo rc=\$?"
t_contains "$t_out" "TEHAS_ANALYTICS_COOKIE is not valid: expected 0 or 1"

t_case "a command fails closed on a bad TEHAS_ANALYTICS before it calls GitHub"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_ANALYTICS=maybe >"$TEHAS_INSTANCE_DIR/instance.env"
t_stub gh
for cmd in "cycle 7" dispatch; do
  t_run "\"\$T_ROOT/libexec/\"$cmd"
  t_eq "$t_rc" 78
  t_contains "$t_out" "TEHAS_ANALYTICS is not valid"
  t_eq "$(t_calls gh)" ""
done

t_case "several labellers separated by single spaces are accepted"
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ALLOWED_LABELERS='alice bob-2 carol'" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && tehas_require TEHAS_ALLOWED_LABELERS && echo \"[\$TEHAS_ALLOWED_LABELERS]\""
t_eq "$t_out" "[alice bob-2 carol]"

# The budget of an earlier version. The limit around the containers is now the
# slice on the host, so an instance that still sets a budget is refused, not
# run with a limit it did not ask for.
t_case "TEHAS_WORK_CPUS set is refused and names the slice file"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_CPUS=3
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "tehas: TEHAS_WORK_CPUS is set and this version of tehas has no CPU budget to set: the limit around all containers is /etc/systemd/system/tehas.slice (see $T/instance/instance.env)"
t_eq "$(grep -c '^tehas:' <<<"$t_out")" 1

t_case "TEHAS_WORK_MEMORY set is refused and names the slice file"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_MEMORY=8g
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "tehas: TEHAS_WORK_MEMORY is set and this version of tehas has no memory budget to set: the limit around all containers is /etc/systemd/system/tehas.slice (see $T/instance/instance.env)"
t_eq "$(grep -c '^tehas:' <<<"$t_out")" 1

t_case "a budget set to nothing is still a budget line, and is refused"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_MEMORY=
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_WORK_MEMORY is set"

t_case "TEHAS_CYCLE_CPUS and TEHAS_CYCLE_MEMORY are not exported"
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_CYCLE_CPUS=64 TEHAS_CYCLE_MEMORY=64g)
t_run "$load && echo \"[\${TEHAS_CYCLE_CPUS:-}][\${TEHAS_CYCLE_MEMORY:-}]\" && bash -c 'echo \"[\${TEHAS_CYCLE_CPUS:-}][\${TEHAS_CYCLE_MEMORY:-}]\"'"
t_eq "$t_out" "[][]
[][]"
T_ENV=()

t_case "_tehas_host_mem_mb is not defined"
t_run "$load; declare -F _tehas_host_mem_mb; echo rc=\$?"
t_eq "$t_out" "rc=1"

t_case "an instance that asks for database clones is refused"
t_instance "${T_MINIMAL[@]}" TEHAS_CLONES=1
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "tehas: TEHAS_CLONES is set and this version of tehas has no database clones"

t_case "previews and clones written as off still load"
t_instance "${T_MINIMAL[@]}" TEHAS_PREVIEWS=0 TEHAS_CLONES=0
t_run "$load; echo rc=\$?"
t_eq "$t_out" "rc=0"

t_case "previews without a domain are refused"
t_instance "${T_MINIMAL[@]}" TEHAS_PREVIEWS=1
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "tehas: TEHAS_PREVIEWS is 1 and TEHAS_DOMAIN is not set"

t_case "previews with a domain load, with the defaults"
t_instance "${T_MINIMAL[@]}" TEHAS_PREVIEWS=1 TEHAS_DOMAIN=example.org
t_run "$load && echo \"\$TEHAS_PREVIEWS \$TEHAS_PREVIEW_MAX \$TEHAS_PREVIEW_MIN_FREE \$TEHAS_PREVIEW_MIN_FREE_MB\""
t_eq "$t_out" "1 5 5g 5120"

t_case "without a domain there are no previews and no derived address"
t_instance "${T_MINIMAL[@]}"
t_run "$load && echo \"[\$TEHAS_DOMAIN] \$TEHAS_PREVIEWS [\${TEHAS_PROTOLAB_URL:-}]\""
t_eq "$t_out" "[] 0 []"

t_case "a domain gives the Protolab its address, and a written one is kept"
t_instance "${T_MINIMAL[@]}" TEHAS_DOMAIN=example.org
t_run "$load && echo \"\$TEHAS_PROTOLAB_URL\""
t_eq "$t_out" "https://lab.example.org"
t_instance "${T_MINIMAL[@]}" TEHAS_DOMAIN=example.org TEHAS_PROTOLAB_URL=https://proto.example.net
t_run "$load && echo \"\$TEHAS_PROTOLAB_URL\""
t_eq "$t_out" "https://proto.example.net"

t_case "tehas_edge_name makes a name from the domain, and nothing else"
t_instance "${T_MINIMAL[@]}" TEHAS_DOMAIN=example.org
t_run "$load && tehas_edge_name uat 12 && tehas_edge_name lab 7 && tehas_edge_name uat 999999999"
t_eq "$t_out" "uat-12.example.org
lab-7.example.org
uat-999999999.example.org"
# A number is one to nine digits and does not start with a zero.
for bad in "www 1" "uat 0" "uat 01" "uat x" "uat" "lab 1x" "lab -1" "uat 1234567890" "lab 1000000000" "uat 12345678901234567890"; do
  t_run "$load; tehas_edge_name $bad; echo rc=\$?"
  t_eq "$t_out" "rc=1"
done
t_instance "${T_MINIMAL[@]}"
t_run "$load; tehas_edge_name uat 12; echo rc=\$?"
t_eq "$t_out" "rc=1"

t_case "an explicit host address is kept"
t_instance "${T_MINIMAL[@]}" TEHAS_HOST_ADDR=192.0.2.7
t_run "$load && echo \"\$TEHAS_HOST_ADDR\""
t_eq "$t_out" "192.0.2.7"

t_case "the worked example takes its private addresses from a file on the host"
t_stub nproc 'echo 12'
TEHAS_INSTANCE_DIR=$T_ROOT/instances/tehas-tehas
t_run "$load && echo \"[\$TEHAS_HOST_ADDR] [\$PI_ENSEMBLE_HOST_ALIASES]\""
t_eq "$t_out" "[127.0.0.1] []"
mkdir -p "$T/home/.config/tehas"
printf '%s\n' "TEHAS_HOST_ADDR=192.0.2.7" "PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.8" >"$T/home/.config/tehas/private.env"
t_run "$load && echo \"[\$TEHAS_HOST_ADDR] [\$PI_ENSEMBLE_HOST_ALIASES]\""
t_eq "$t_out" "[192.0.2.7] [models:192.0.2.8]"
rm -rf "$T/home/.config"
rm -f "$T/bin/nproc"

t_case "loading needs neither git nor the network nor a checkout"
t_instance "${T_MINIMAL[@]}"
mkdir -p "$T/nogit"
for tool in bash awk nproc id head cat tr env; do ln -sf "$(command -v "$tool")" "$T/nogit/$tool"; done
t_out=$(cd "$T" && env -i PATH="$T/nogit" HOME="$T/home" TEHAS_HOME="$T/home/tehas" T_ROOT="$T_ROOT" \
  TEHAS_INSTANCE_DIR="$TEHAS_INSTANCE_DIR" bash -c "$load && echo loaded" 2>&1)
t_eq "$t_out" "loaded"

t_case "loading writes nothing"
before=$(find "$T/home" "$T/instance" | sort)
t_run "$load"
after=$(find "$T/home" "$T/instance" | sort)
t_eq "$after" "$before"

# --- tehas_branch -----------------------------------------------------------
git init -q -b main "$T/upstream"
git -C "$T/upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m first
git clone -q "$T/upstream" "$T/home/tehas/repos/widget"

t_case "tehas_branch reads the default branch from origin/HEAD"
t_instance "${T_MINIMAL[@]}"
t_run "$load && tehas_branch"
t_eq "$t_rc" 0
t_eq "$t_out" "main"

t_case "TEHAS_BRANCH wins over the checkout"
t_instance "${T_MINIMAL[@]}" TEHAS_BRANCH=release
t_run "$load && tehas_branch"
t_eq "$t_out" "release"

t_case "TEHAS_BRANCH_CHECK: unset is none, a name with spaces is kept"
t_instance "${T_MINIMAL[@]}"
t_run "$load && echo \"[\$TEHAS_BRANCH_CHECK]\""
t_eq "$t_out" "[]"
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_BRANCH_CHECK='build / verify (linux)'" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_BRANCH_CHECK]\""
t_eq "$t_out" "[build / verify (linux)]"

t_case "TEHAS_LOW_GATE: unset is none, an absolute path is kept"
t_instance "${T_MINIMAL[@]}"
t_run "$load && set -u && echo \"[\$TEHAS_LOW_GATE]\""
t_eq "$t_out" "[]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_LOW_GATE=/x/gate >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_LOW_GATE]\""
t_eq "$t_out" "[/x/gate]"

for bad in "gate" "bin/gate" "/x/cheap power" "/x/gate " "//gate" "/x/gate/" "/." "/.." "/x//gate" "/x/./gate" "/x/../gate" "/x/.."; do
  t_case "loading rejects a TEHAS_LOW_GATE of '$bad'"
  printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_LOW_GATE='$bad'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load"
  t_eq "$t_rc" 78
  t_contains "$t_out" "TEHAS_LOW_GATE is not valid: expected an absolute path of /, letters, digits and ._-, with no empty or dot segment"
done
# The gate runs as the tehas user in every poll, and a merged pull request
# writes the trees under the host's tehas home and the instance directory: a
# setting in the same pull request would otherwise name a program the factory
# put in place. It is refused by every command, before GitHub is asked anything.
for inside in "$T/home/tehas/bin/gate" "$T/home/tehas/libexec/gate" "$T/home/tehas/repos/widget/gate" "$T/instance/gate" "$T/home/tehas"; do
  t_case "loading rejects a TEHAS_LOW_GATE a pull request could put there"
  printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_LOW_GATE=$inside" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load"
  t_eq "$t_rc" 78
  t_contains "$t_out" "TEHAS_LOW_GATE is not valid: expected a command outside"
done
t_case "TEHAS_LOW_GATE: a program of the person's own, outside those trees, is kept"
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_LOW_GATE=$T/bin/cheap-power" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_LOW_GATE]\""
t_eq "$t_rc" 0
t_eq "$t_out" "[$T/bin/cheap-power]"
t_case "TEHAS_LOW_GATE_WAIT: 60 seconds unless the instance says otherwise"
t_instance "${T_MINIMAL[@]}"
t_run "$load && set -u && echo \"[\$TEHAS_LOW_GATE_WAIT]\""
t_eq "$t_out" "[60]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_LOW_GATE=/x/gate TEHAS_LOW_GATE_WAIT=5 >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_LOW_GATE_WAIT]\""
t_eq "$t_out" "[5]"
for bad in "soon" "0" "-5" "1.5" "60 "; do
  t_case "loading rejects a TEHAS_LOW_GATE_WAIT of '$bad'"
  printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_LOW_GATE_WAIT='$bad'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load"
  t_eq "$t_rc" 78
  t_contains "$t_out" "TEHAS_LOW_GATE_WAIT is not valid: expected a positive whole number"
done
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_LOW_GATE_WAIT=600 >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_LOW_GATE_WAIT]\""
t_eq "$t_out" "[600]"
# The poll waits for the gate while it holds state/dispatch.lock, so every later
# poll is held up by these seconds too: a mistyped huge value must stop loading,
# not wait for a day. A digit string no comparison can hold is refused by its
# length, with no complaint from the comparison itself.
for bad in "601" "9999" "99999999999999999999"; do
  t_case "loading rejects a TEHAS_LOW_GATE_WAIT of '$bad'"
  printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_LOW_GATE_WAIT='$bad'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load"
  t_eq "$t_rc" 78
  t_contains "$t_out" "TEHAS_LOW_GATE_WAIT is not valid: expected at most 600 seconds"
  t_lacks "$t_out" "integer expression"
done
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"

t_case "a checkout without origin/HEAD fails with both remedies"
t_instance "${T_MINIMAL[@]}"
git -C "$T/home/tehas/repos/widget" remote set-head origin -d
t_run "$load && b=\$(tehas_branch); echo \"rc=\$? b=[\$b]\""
t_contains "$t_out" "rc=78 b=[]"
t_contains "$t_out" "TEHAS_BRANCH"
t_contains "$t_out" "install.sh"

t_case "the cycle's workspace is under the tehas tree and is not a setting"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_DIR=/somewhere/else
t_run "$load && echo \"\$TEHAS_WORK_DIR\""
t_eq "$t_rc" 0
t_eq "$t_out" "$T/home/tehas/work/widget"

t_case "the planning limits have defaults"
t_instance "${T_MINIMAL[@]}"
t_run '. "$T_ROOT/lib/config.sh"; tehas_load; echo "$TEHAS_PLAN_MAX_TURNS $TEHAS_PLAN_TIMEOUT"'
t_eq "$t_out" "20 $plan_max"

# The two hand-copied defaults, the header of the command and the commented
# example an operator copies, are pinned against the config.
t_contains "$(grep 'TEHAS_PLAN_TIMEOUT  seconds' "$T_ROOT/libexec/plan")" "[$plan_max, the ceiling"
t_contains "$(grep 'TEHAS_PLAN_TIMEOUT        seconds' "$T_ROOT/libexec/dispatch")" "[$plan_max]"
t_contains "$(grep '^#TEHAS_PLAN_TIMEOUT=' "$T_ROOT/instances/example/instance.env")" "$plan_max"

t_case "a planning limit set empty takes its default"
t_instance "${T_MINIMAL[@]}" TEHAS_PLAN_TIMEOUT= TEHAS_WORK_TIMEOUT= TEHAS_VERIFY_TIMEOUT=
t_run "$load; echo rc=\$? \$TEHAS_PLAN_TIMEOUT \$TEHAS_WORK_TIMEOUT \$TEHAS_VERIFY_TIMEOUT"
t_eq "$t_out" "rc=0 $plan_max 21600 1800"
t_instance "${T_MINIMAL[@]}"

t_case "the planning limits can be set in the instance"
t_instance "${T_MINIMAL[@]}" TEHAS_PLAN_MAX_TURNS=5 TEHAS_PLAN_TIMEOUT=600
t_run '. "$T_ROOT/lib/config.sh"; tehas_load; echo "$TEHAS_PLAN_MAX_TURNS $TEHAS_PLAN_TIMEOUT"'
t_eq "$t_out" "5 600"

t_case "the planning test hook is kept from the environment, a setting of that name is not"
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_PLAN_CMD=fake-plan TEHAS_PLAN_START_CMD=fake-plan-start TEHAS_PLAN_MAX_TURNS=99)
t_run '. "$T_ROOT/lib/config.sh"; tehas_load; echo "$TEHAS_PLAN_CMD $TEHAS_PLAN_START_CMD $TEHAS_PLAN_MAX_TURNS"'
t_eq "$t_out" "fake-plan fake-plan-start 20"
T_ENV=()

t_case "the index run's test hooks are kept from the environment"
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_INDEX_START_CMD=fake-index-start TEHAS_INDEX_LOCK_WAIT=1)
t_run '. "$T_ROOT/lib/config.sh"; tehas_load; echo "$TEHAS_INDEX_START_CMD $TEHAS_INDEX_LOCK_WAIT"'
t_eq "$t_out" "fake-index-start 1"
T_ENV=()

# The host's CPU count is read with nproc; a stub makes the default the same
# on every machine.
t_stub nproc 'echo 8'
t_case "the ticket ceiling defaults to the host's CPUs minus one"
t_instance "${T_MINIMAL[@]}"
t_run "$load && echo \"\$TEHAS_TICKET_CONCURRENCY\""
t_eq "$t_rc" 0
t_eq "$t_out" "7"
t_stub nproc 'echo 1'
t_run "$load && echo \"\$TEHAS_TICKET_CONCURRENCY\""
t_eq "$t_out" "1"
t_stub nproc 'echo 8'

t_case "workstreams alone are refused where the ticket ceiling's default is above 1, accepted where it is 1"
t_instance "${T_MINIMAL[@]}" TEHAS_CONCURRENCY=4
t_run "$load"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_CONCURRENCY (4) and TEHAS_TICKET_CONCURRENCY (7) are both above 1"
t_stub nproc 'echo 2'
t_run "$load && echo \"\$TEHAS_CONCURRENCY \$TEHAS_TICKET_CONCURRENCY\""
t_eq "$t_rc" 0
t_eq "$t_out" "4 1"
t_stub nproc 'echo 8'
t_instance "${T_MINIMAL[@]}"

t_case "the review cap leaves room for every cycle and two pull requests by default"
t_run "$load && echo \"\$TEHAS_MAX_AWAITING\""
t_eq "$t_out" "9"

t_case "a ceiling above the host's CPUs is accepted"
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=20
t_run "$load && echo \"\$TEHAS_TICKET_CONCURRENCY \$TEHAS_MAX_AWAITING\""
t_eq "$t_rc" 0
t_eq "$t_out" "20 22"

t_case "the review cap's default follows the cycles side by side"
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=4
t_run "$load && echo \"\$TEHAS_MAX_AWAITING\""
t_eq "$t_out" "6"

t_case "a review cap that is set is kept"
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=4 TEHAS_MAX_AWAITING=3
t_run "$load && echo \"\$TEHAS_MAX_AWAITING\""
t_eq "$t_out" "3"

t_case "a concurrency that is no number is refused, not added to"
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=two
t_run "$load"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_TICKET_CONCURRENCY is not valid"
rm -f "$T/bin/nproc"

t_case "the reserve defaults to 2g"
t_instance "${T_MINIMAL[@]}"
t_run "$load && bash -c 'echo \$TEHAS_CYCLE_RESERVE'"
t_eq "$t_rc" 0
t_eq "$t_out" "2g"

t_case "a reserve the instance sets is kept as written, and one in the environment is not used"
t_instance "${T_MINIMAL[@]}" TEHAS_CYCLE_RESERVE=768m
T_ENV=(TEHAS_CYCLE_RESERVE=64g)
t_run "$load && echo \"\$TEHAS_CYCLE_RESERVE\""
t_eq "$t_out" "768m"
t_instance "${T_MINIMAL[@]}"
t_run "$load && echo \"\$TEHAS_CYCLE_RESERVE\""
t_eq "$t_out" "2g"
T_ENV=()

# 02g and 1000000000g are sizes to Docker but not to lib/slice.sh, which reads
# the reserve at every start: what loads must be what the slice library reads.
for bad in lots 2gb 0 5m 6000k -1g 1.5g 02g 1000000000g; do
  t_case "a reserve that is not a size, or under 6m, is refused: $bad"
  t_instance "${T_MINIMAL[@]}" "TEHAS_CYCLE_RESERVE=$bad"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "tehas: TEHAS_CYCLE_RESERVE is not valid: expected a size of at least 6m, such as 512m or 2g"
done

t_case "a reserve of 6m is the smallest that loads"
t_instance "${T_MINIMAL[@]}" TEHAS_CYCLE_RESERVE=6m
t_run "$load && echo \"\$TEHAS_CYCLE_RESERVE\""
t_eq "$t_out" "6m"

t_case "TEHAS_CGROUP_ROOT in the environment is kept"
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_CGROUP_ROOT=/somewhere/cgroup)
t_run "$load && echo \"\$TEHAS_CGROUP_ROOT\""
t_eq "$t_out" "/somewhere/cgroup"
T_ENV=()

t_case "loading sets no cgroup root of its own"
t_run "$load && echo \"[\${TEHAS_CGROUP_ROOT:-}]\""
t_eq "$t_out" "[]"

t_case "the host section is published only when the instance asks for it"
t_instance "${T_MINIMAL[@]}"
t_run "$load && echo \"\$TEHAS_TOP\""
t_eq "$t_out" "0"
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=1
t_run "$load && echo \"\$TEHAS_TOP\""
t_eq "$t_out" "1"
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=0
t_run "$load && echo \"\$TEHAS_TOP\""
t_eq "$t_out" "0"
# A line in the environment is not the instance asking for it.
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_TOP=1)
t_run "$load && echo \"\$TEHAS_TOP\""
t_eq "$t_out" "0"
T_ENV=()

t_case "the energy switches in ~/.config/tehas/env survive env.sh"
t_instance "${T_MINIMAL[@]}"
mkdir -p "$T/home/.config/tehas" "$T/home/tehas/lib"
cp "$T_ROOT/lib/config.sh" "$T/home/tehas/lib/"
printf '%s\n' GH_TOKEN=t0 TEHAS_ENERGY=off TEHAS_ENERGY_URL=http://mesh.example:8086/v1/cluster \
  >"$T/home/.config/tehas/env"
t_run '. "$T_ROOT/host/env.sh"; echo "[$GH_TOKEN] [${TEHAS_ENERGY:-}] [${TEHAS_ENERGY_URL:-}] [$TEHAS_REPO]"'
t_eq "$t_out" "[t0] [off] [http://mesh.example:8086/v1/cluster] [acme/widget]"
rm -rf "$T/home/.config/tehas" "$T/home/tehas/lib"

t_case "without a role model every role is left to the subagent model"
t_instance "${T_MINIMAL[@]}"
t_run "$load && env | grep -E '^PI_ENSEMBLE_(MODEL|PROVIDER)_' ; tehas_models"
t_eq "$t_out" "test-provider test-model"

t_case "a role model is passed on, with the subagent provider unless it names its own"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_MODEL_CODE_REVIEW_SPECIALIST=small-model \
  PI_ENSEMBLE_MODEL_DEVELOPER=big-model PI_ENSEMBLE_PROVIDER_DEVELOPER=other-provider
t_run "$load && bash -c 'env | grep -E \"^PI_ENSEMBLE_(MODEL|PROVIDER)_\" | LC_ALL=C sort'"
t_eq "$t_rc" 0
t_eq "$t_out" "PI_ENSEMBLE_MODEL_CODE_REVIEW_SPECIALIST=small-model
PI_ENSEMBLE_MODEL_DEVELOPER=big-model
PI_ENSEMBLE_PROVIDER_CODE_REVIEW_SPECIALIST=test-provider
PI_ENSEMBLE_PROVIDER_DEVELOPER=other-provider"

t_case "tehas_models names each provider and model once, the subagent default first"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_MODEL_EXPLORE=test-model \
  PI_ENSEMBLE_MODEL_DEVELOPER=big-model PI_ENSEMBLE_PROVIDER_DEVELOPER=other-provider \
  PI_ENSEMBLE_MODEL_OPS=big-model PI_ENSEMBLE_PROVIDER_OPS=other-provider
t_run "$load && tehas_models"
t_eq "$t_out" "test-provider test-model
other-provider big-model"

t_case "an empty role model is not set and is not passed on"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_MODEL_DEVELOPER=
t_run "$load && echo \"[\$(env | grep '^PI_ENSEMBLE_MODEL_')]\""
t_eq "$t_rc" 0
t_eq "$t_out" "[]"

for bad in PI_ENSEMBLE_MODEL_REVIEWER=m PI_ENSEMBLE_MODEL_PROJECT_MANAGER=m PI_ENSEMBLE_MODEL_developer=m \
  PI_ENSEMBLE_PROVIDER_PLANNER=p PI_ENSEMBLE_MODEL_LENS_SECURITY=; do
  t_case "a role pi-rukas does not know is refused: ${bad%%=*}"
  t_instance "${T_MINIMAL[@]}" "$bad"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "tehas: ${bad%%=*} names no role pi-rukas gives a model to; the roles are DEVELOPER OPS EXPLORE ADVERSARIAL_DEVELOPER CODE_REVIEW_SPECIALIST"
done

t_case "a role provider without its model is refused"
for line in PI_ENSEMBLE_PROVIDER_OPS=other "PI_ENSEMBLE_PROVIDER_OPS=other PI_ENSEMBLE_MODEL_OPS="; do
  # shellcheck disable=SC2086  # one setting per word
  t_instance "${T_MINIMAL[@]}" $line
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "PI_ENSEMBLE_PROVIDER_OPS is set and PI_ENSEMBLE_MODEL_OPS is not"
done

t_case "a role model of two words is refused"
printf '%s\n' "${T_MINIMAL[@]}" "PI_ENSEMBLE_MODEL_EXPLORE='two words'" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "PI_ENSEMBLE_MODEL_EXPLORE is not valid: expected one word"
t_instance "${T_MINIMAL[@]}"

t_case "a role model in the environment is dropped, like every setting"
t_run "PI_ENSEMBLE_MODEL_NOPE=x PI_ENSEMBLE_MODEL_DEVELOPER=x; export PI_ENSEMBLE_MODEL_NOPE PI_ENSEMBLE_MODEL_DEVELOPER; $load && echo \"[\${PI_ENSEMBLE_MODEL_DEVELOPER:-}]\""
t_eq "$t_rc" 0
t_eq "$t_out" "[]"

t_case "the MCP server's settings come from the host's environment and are kept"
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_MCP=1 TEHAS_MCP_KEYS="abcdefghijklmnopqrstuvwxyz012345 second")
t_run "$load && echo \"\$TEHAS_MCP [\$TEHAS_MCP_KEYS]\""
t_eq "$t_out" "1 [abcdefghijklmnopqrstuvwxyz012345 second]"
T_ENV=()

t_case "keys for the MCP server in the instance directory are refused"
t_instance "${T_MINIMAL[@]}" TEHAS_MCP_KEYS=abcdefghijklmnopqrstuvwxyz012345
t_run "$load"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_MCP_KEYS"
t_lacks "$t_out" "abcdefghijklmnopqrstuvwxyz012345"

# The cleaner's two settings: the hours of age a thing must have before it goes,
# and the directory names that count as build output.
t_case "the cleaner removes a day of build output unless the instance says otherwise"
t_instance "${T_MINIMAL[@]}"
t_run "$load && set -u && echo \"[\$TEHAS_CLEAN_AFTER] [\$TEHAS_CLEAN_DIRS]\""
t_eq "$t_rc" 0
t_eq "$t_out" "[24] [target node_modules __pycache__ .venv .pytest_cache .mypy_cache .next .turbo .gradle]"

t_case "the cleaner's age and its names can both be set in the instance"
t_instance "${T_MINIMAL[@]}" "TEHAS_CLEAN_AFTER=6" "TEHAS_CLEAN_DIRS='target dist'"
t_run "$load && echo \"[\$TEHAS_CLEAN_AFTER] [\$TEHAS_CLEAN_DIRS]\""
t_eq "$t_rc" 0
t_eq "$t_out" "[6] [target dist]"

# The cleaner's two settings. The hours are asked of docker's `until` and of
# find's `-newermt`, so a sign, a fraction or an exponent is no number of
# hours; an empty value takes the default, as every other setting left empty
# does.
for bad in "soon" "0" "-5" "1.5" "24 " "1e3" "123456"; do
  t_case "loading rejects a TEHAS_CLEAN_AFTER of '$bad'"
  printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_CLEAN_AFTER='$bad'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load"
  t_eq "$t_rc" 78
  t_contains "$t_out" "TEHAS_CLEAN_AFTER is not valid: expected a whole number of hours of at least 1"
done

# The names go to find -name, so a path, a bare dot, and the family of the two
# dot-names a workspace keeps are all refused: they are not build output. The
# family, not just the two names spelled, because a name close to one of them is
# a name someone meant as one of them, and the cleaner's walk stops at a dot-name
# it cannot name here.
for bad in "." ".." ".git" ".pi" "a/target" "/target" "target;rm -rf" "target *"; do
  t_case "loading rejects a TEHAS_CLEAN_DIRS of '$bad'"
  printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_CLEAN_DIRS='$bad'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load"
  t_eq "$t_rc" 78
  t_contains "$t_out" "TEHAS_CLEAN_DIRS is not valid"
done

# The default the example instance copies by hand is pinned against the
# config's own list, so the two cannot drift apart.
t_case "the example instance documents the defaults the cleaner runs with"
t_instance "${T_MINIMAL[@]}"
t_run "$load && echo \"\$TEHAS_CLEAN_DIRS\""
default_dirs=$t_out
t_eq "$t_rc" 0
t_contains "$(grep '^#TEHAS_CLEAN_AFTER=' "$T_ROOT/instances/example/instance.env")" "=24"
t_contains "$(grep '^#TEHAS_CLEAN_DIRS=' "$T_ROOT/instances/example/instance.env")" "$default_dirs"
t_instance "${T_MINIMAL[@]}"

t_done
