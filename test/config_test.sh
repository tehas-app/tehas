#!/usr/bin/env bash
# Tests for lib/config.sh: how instance settings are found, loaded, defaulted
# and validated.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

load='. "$T_ROOT/lib/config.sh"; tehas_load'
# The ceiling as lib/config.sh writes it, so a test never hand-copies it.
plan_max=$(sed -n 's/^[[:space:]]*local TEHAS_PLAN_TIMEOUT_MAX=//p' "$T_ROOT/lib/config.sh")
# The bound of one child, likewise read from the config rather than copied.
child_max=$(sed -n 's/^[[:space:]]*local TEHAS_CHILD_TIMEOUT_MAX=//p' "$T_ROOT/lib/config.sh")

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

t_case "a child's bound is two hours unless the instance says otherwise"
t_run "$load && echo \$TEHAS_CHILD_TIMEOUT"
t_eq "$t_out" "$child_max"
t_instance "${T_MINIMAL[@]}" TEHAS_CHILD_TIMEOUT=10800
t_run "$load && echo \$TEHAS_CHILD_TIMEOUT"
t_eq "$t_out" "10800"
# The copy an operator copies and the row of the manual are the same number as
# the config's, and a line that names the setting and leaves it empty takes the
# default rather than leaving it unset for the arithmetic of tehas work.
t_contains "$(grep '^#TEHAS_CHILD_TIMEOUT=' "$T_ROOT/instances/example/optional.env")" "$child_max"
t_contains "$(grep '^| .TEHAS_CHILD_TIMEOUT.' "$T_ROOT/host/docs/tools.md")" "| \`$child_max\`"
t_instance "${T_MINIMAL[@]}" TEHAS_CHILD_TIMEOUT=
t_run "$load; echo rc=\$? \$TEHAS_CHILD_TIMEOUT"
t_eq "$t_out" "rc=0 $child_max"
t_instance "${T_MINIMAL[@]}"

t_case "a child bound longer than the cycle's own limit is refused"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_TIMEOUT=21600 TEHAS_CHILD_TIMEOUT=3601
t_run "$load; echo rc=\$?"
t_eq "$t_out" "rc=0"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_TIMEOUT=3600 TEHAS_CHILD_TIMEOUT=3601
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_CHILD_TIMEOUT (3601) is more than TEHAS_WORK_TIMEOUT (3600): a child that outlives its cycle is one tehas stopped too late"
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
t_contains "$(grep 'at most' "$T_ROOT/instances/example/optional.env")" "at most $plan_max"
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
t_contains "$t_out" "TEHAS_PLAN_TIMEOUT is not valid: expected at most 9 digits"
t_instance "${T_MINIMAL[@]}" TEHAS_PLAN_TIMEOUT=10000000
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_PLAN_TIMEOUT (10000000) is more than $plan_max"
t_instance "${T_MINIMAL[@]}"

t_case "the poll's unit has a finite limit, twice the lock's age at which a poll warns"
t_eq "$(sed -n 's/^TimeoutStartSec=//p' "$T_ROOT/host/systemd/tehas-dispatch.service")" "1h"
t_contains "$(cat "$T_ROOT/libexec/dispatch")" "bound=1800"

# A poll that exits non-zero leaves its unit failed, and a timer armed only
# from the unit's inactivity does not fire again from that state: on 2026-10-20
# one failed poll stopped the queue for 15 minutes. The two arms are pinned
# together so neither is dropped alone.
t_case "the poll's timer is armed from the last end and from the last start"
t_eq "$(sed -n 's/^OnBootSec=//p' "$T_ROOT/host/systemd/tehas-dispatch.timer")" "2min"
t_eq "$(sed -n 's/^OnUnitInactiveSec=//p' "$T_ROOT/host/systemd/tehas-dispatch.timer")" "2min"
t_eq "$(sed -n 's/^OnUnitActiveSec=//p' "$T_ROOT/host/systemd/tehas-dispatch.timer")" "2min"

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

t_case "a zip is not unpacked unless the instance says so, and only where it gives images or files"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1
T_ENV=(TEHAS_WORK_ZIPS=1)
t_run "$load && echo \"\$TEHAS_WORK_ZIPS\""
t_eq "$t_out" "0"
T_ENV=()
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1 TEHAS_WORK_ZIPS=1
t_run "$load && echo \"\$TEHAS_WORK_ZIPS\""
t_eq "$t_out" "1"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_FILES=1 TEHAS_WORK_ZIPS=1
t_run "$load && echo \"\$TEHAS_WORK_ZIPS\""
t_eq "$t_out" "1"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_ZIPS=1
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "tehas: TEHAS_WORK_ZIPS is 1 and neither TEHAS_WORK_IMAGES nor TEHAS_WORK_FILES is"

t_case "the test hooks survive from the environment"
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_WORK_CMD=fake-work TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index
  TEHAS_CYCLE_START_CMD=fake-start TEHAS_CYCLE_START_WAIT=2 TEHAS_PREFLIGHT_WAIT=3 TEHAS_STOP_WAIT=4 TEHAS_FETCH_WAIT=5
  TEHAS_CYCLE_RETRY_WAIT=6 TEHAS_CYCLE_STOP_GH_WAIT=7 TEHAS_CONSOLE_WAIT=8 TEHAS_CYCLE_STOP_CMD=fake-stop
  TEHAS_TIDY_CMD=fake-tidy TEHAS_TIDY_WAIT=9 TEHAS_ANALYTICS_WAIT=1 TEHAS_BUILD_TIMEOUT=600)
t_run "$load && echo \"\$TEHAS_WORK_CMD \$TEHAS_STATUS_CMD \$TEHAS_INDEX_CMD \$TEHAS_CYCLE_START_CMD \$TEHAS_CYCLE_START_WAIT \$TEHAS_PREFLIGHT_WAIT \$TEHAS_STOP_WAIT \$TEHAS_FETCH_WAIT \$TEHAS_CYCLE_RETRY_WAIT \$TEHAS_CYCLE_STOP_GH_WAIT \$TEHAS_CONSOLE_WAIT \$TEHAS_CYCLE_STOP_CMD \$TEHAS_TIDY_CMD \$TEHAS_TIDY_WAIT \$TEHAS_ANALYTICS_WAIT \$TEHAS_BUILD_TIMEOUT\""
t_eq "$t_out" "fake-work fake-status fake-index fake-start 2 3 4 5 6 7 8 fake-stop fake-tidy 9 1 600"
T_ENV=()

t_case "the words a cycle hands down survive from the environment, and nothing else of its kind"
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_CYCLE_FROM=unit TEHAS_CYCLE_ATTEMPT=3 TEHAS_CYCLE_OTHER=x)
t_run "$load && echo \"\$TEHAS_CYCLE_FROM \$TEHAS_CYCLE_ATTEMPT \${TEHAS_CYCLE_OTHER:-gone}\""
t_eq "$t_out" "unit 3 gone"
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
  "TEHAS_BRANCH=two words" "TEHAS_INDEXING=yes" "TEHAS_WORK_IMAGES=yes" "TEHAS_WORK_IMAGES=2" "TEHAS_WORK_ZIPS=yes" "TEHAS_WORK_ZIPS=2" "TEHAS_WORK_DESCRIBE=yes" "TEHAS_WORK_FILES=yes" "TEHAS_WORK_FILES=2" "TEHAS_WORK_COMMENTS=yes" "TEHAS_TOP=yes" "TEHAS_TOP=2" "TEHAS_ANALYTICS_SRC=http://plain.example/ea.js" \
  "TEHAS_ANALYTICS_SRC=https://a.example/x.js\" onload=\"x" "TEHAS_ANALYTICS_SITE=two words" \
  "TEHAS_ANALYTICS_COOKIE=yes" "TEHAS_THEME=neon" "TEHAS_THEME=../pixel" "TEHAS_LAYOUT=game" "TEHAS_LAYOUT=../business" "TEHAS_PROTOLAB_URL=https://lab.example/" "TEHAS_PROTOLAB_URL=lab.example" \
  "TEHAS_PLAN_MAX_TURNS=0" "TEHAS_PLAN_TIMEOUT=1h" "TEHAS_VERIFY_TIMEOUT=30m" "TEHAS_VERIFY_TIMEOUT=0" \
  "TEHAS_CHILD_TIMEOUT=30m" "TEHAS_CHILD_TIMEOUT=0" "TEHAS_CHILD_TIMEOUT=-1" "TEHAS_CHILD_TIMEOUT= \t" \
  "TEHAS_BUILD_TIMEOUT=30m" "TEHAS_BUILD_TIMEOUT=0" \
  "TEHAS_DOMAIN=https://example.org" "TEHAS_DOMAIN=example.org/" "TEHAS_DOMAIN=Example.org" \
  "TEHAS_DOMAIN=localhost" "TEHAS_DOMAIN=-a.example.org" "TEHAS_PREVIEWS=yes" \
  "TEHAS_PREVIEW_MAX=0" "TEHAS_PREVIEW_MAX=100" "TEHAS_PREVIEW_MIN_FREE=lots" "TEHAS_CYCLE_MIN_FREE=lots" "TEHAS_CLEAN_MIN_FREE=lots" \
  "TEHAS_IMPROVE_REPO=not a repo" "TEHAS_IMPROVE_REPO=acme" "TEHAS_IMPROVE_REPO=acme/factory/more" \
  "TEHAS_IMPROVE_RUKAS_REPO=not a repo" "TEHAS_IMPROVE_RUKAS_REPO=acme" "TEHAS_IMPROVE_RUKAS_SKIP=yes" \
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

t_case "findings about pi-rukas are filed unless the instance skips them, and never both"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_IMPROVE_RUKAS_SKIP]\""
t_eq "$t_out" "[0]"
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory TEHAS_IMPROVE_RUKAS_SKIP=1
t_run "$load && echo \"[\$TEHAS_IMPROVE_RUKAS_SKIP]\""
t_eq "$t_out" "[1]"
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory TEHAS_IMPROVE_RUKAS_REPO=acme/rukas \
  TEHAS_IMPROVE_RUKAS_SKIP=1
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_IMPROVE_RUKAS_SKIP is 1 and TEHAS_IMPROVE_RUKAS_REPO is set"

t_case "the loop and the review never file in the product repository, whatever its case"
for v in TEHAS_IMPROVE_REPO=acme/widget TEHAS_IMPROVE_REPO=Acme/Widget \
  "TEHAS_IMPROVE_REPO=acme/backlog TEHAS_IMPROVE_RUKAS_REPO=acme/widget" \
  TEHAS_REVIEW_REPO=acme/widget TEHAS_REVIEW_REPO=Acme/Widget; do
  # shellcheck disable=SC2086  # $v is one or two settings
  printf '%s\n' "${T_MINIMAL[@]}" $v >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "the improvement loop files in acme/widget, the product repository"
done
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/backlog >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_IMPROVE_REPO]\""
t_eq "$t_out" "[acme/backlog]"

t_case "the loop's limit of open issues is 10 unless the instance says otherwise"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_IMPROVE_MAX_OPEN]\""
t_eq "$t_out" "[10]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_IMPROVE_MAX_OPEN=30 >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_IMPROVE_MAX_OPEN]\""
t_eq "$t_out" "[30]"

t_case "the millwright is off unless the instance names its account, may retry twice and answer three planning questions by default"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && set -u && echo \"[\$TEHAS_MILLWRIGHT_LOGIN][\$TEHAS_MILLWRIGHT_MAX_RETRIES][\$TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS]\""
t_eq "$t_out" "[][2][3]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_MILLWRIGHT_LOGIN=tehas-fixer TEHAS_MILLWRIGHT_MAX_RETRIES=3 TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS=1 >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_MILLWRIGHT_LOGIN][\$TEHAS_MILLWRIGHT_MAX_RETRIES][\$TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS]\""
t_eq "$t_out" "[tehas-fixer][3][1]"

t_case "refreshes are off unless the instance turns them on, two of one pull request and every 10 minutes by default"
t_instance "${T_MINIMAL[@]}"
t_run "$load && set -u && echo \"[\$TEHAS_REFRESH_CONFLICTS][\$TEHAS_REFRESH_MAX][\$TEHAS_REFRESH_EVERY][\$TEHAS_REFRESH_SLOTS]\""
t_eq "$t_out" "[0][2][10][1]"
t_instance "${T_MINIMAL[@]}" TEHAS_REFRESH_CONFLICTS=1 TEHAS_REFRESH_MAX=5 TEHAS_REFRESH_EVERY=1440 TEHAS_REFRESH_SLOTS=8
t_run "$load && echo \"[\$TEHAS_REFRESH_CONFLICTS][\$TEHAS_REFRESH_MAX][\$TEHAS_REFRESH_EVERY][\$TEHAS_REFRESH_SLOTS]\""
t_eq "$t_out" "[1][5][1440][8]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_REFRESH_SLOTS=0 >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_REFRESH_SLOTS]\""
t_eq "$t_out" "[0]"

t_case "the review loop is off unless the instance turns it on"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && set -u && echo \"[\$TEHAS_REVIEW]\""
t_eq "$t_out" "[0]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_REVIEW=1 >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_REVIEW]\""
t_eq "$t_out" "[1]"

for bad in "TEHAS_REFRESH_CONFLICTS=yes" "TEHAS_REFRESH_CONFLICTS=2" "TEHAS_REVIEW=yes" "TEHAS_REVIEW=2" "TEHAS_REFRESH_MAX=0" "TEHAS_REFRESH_MAX=-1" "TEHAS_REFRESH_MAX=many" \
  "TEHAS_REFRESH_EVERY=0" "TEHAS_REFRESH_EVERY=1441" "TEHAS_REFRESH_EVERY=010" "TEHAS_REFRESH_EVERY=10m" \
  "TEHAS_REFRESH_SLOTS=9" "TEHAS_REFRESH_SLOTS=-1" "TEHAS_REFRESH_SLOTS=01" "TEHAS_REFRESH_SLOTS=one"; do
  t_case "loading rejects $bad"
  printf '%s\n' "${T_MINIMAL[@]}" "${bad%%=*}='${bad#*=}'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
done

t_case "the slice holds four containers unless the instance says another number from 2 to 16, or auto"
t_instance "${T_MINIMAL[@]}"
t_run "$load && echo \"[\$TEHAS_SLICE_CONTAINERS]\""
t_eq "$t_out" "[4]"
for good_n in 2 6 16 auto; do
  t_instance "${T_MINIMAL[@]}" TEHAS_SLICE_CONTAINERS=$good_n
  t_run "$load && echo \"[\$TEHAS_SLICE_CONTAINERS]\""
  t_eq "$t_out" "[$good_n]"
done
# `auto` is the poll's word, refused by lib/slice.sh at every start
# (test/slice_test.sh and the Rust slice's own test). Anything else is no count.
for bad in 1 0 17 06 1.5 -4 99999999999999999999 AUTO; do
  t_case "loading rejects TEHAS_SLICE_CONTAINERS=$bad"
  t_instance "${T_MINIMAL[@]}" "TEHAS_SLICE_CONTAINERS=$bad"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
done

# The hold of rule 6: unset, the dispatcher never asks the measure, so nothing
# changes. Set, it is a whole number of at least 1.
t_case "the size hold is off unless the instance names a number of files"
t_instance "${T_MINIMAL[@]}"
t_run "$load && set -u && echo \"[\$TEHAS_SIZE_MAX_FILES]\""
t_eq "$t_out" "[]"
t_instance "${T_MINIMAL[@]}" TEHAS_SIZE_MAX_FILES=3
t_run "$load && echo \"[\$TEHAS_SIZE_MAX_FILES]\""
t_eq "$t_out" "[3]"

for bad in "TEHAS_SIZE_MAX_FILES=0" "TEHAS_SIZE_MAX_FILES=-1" "TEHAS_SIZE_MAX_FILES=x"; do
  t_case "loading rejects $bad"
  t_instance "${T_MINIMAL[@]}" "${bad%%=*}='${bad#*=}'"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "tehas: TEHAS_SIZE_MAX_FILES is not valid"
done

# Every count: a digit string no comparison can hold is refused by its length,
# not taken as passing the comparison that failed on it (the dispatcher compares
# TEHAS_SIZE_MAX_FILES with the files of a body, and `tehas limit` its cycles
# with what runs: a long number must stop the load, not reach either).
for name in TEHAS_WORK_TIMEOUT TEHAS_CONCURRENCY TEHAS_TICKET_CONCURRENCY TEHAS_VERIFY_TIMEOUT \
  TEHAS_CHILD_TIMEOUT TEHAS_WORK_STALL TEHAS_PLAN_TIMEOUT TEHAS_SIZE_MAX_FILES TEHAS_MAX_AWAITING \
  TEHAS_MILLWRIGHT_MAX_RETRIES; do
  t_case "loading rejects a $name of 20 digits"
  t_instance "${T_MINIMAL[@]}" "$name=99999999999999999999"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "$name is not valid: expected at most 9 digits"
  t_lacks "$t_out" "integer expression"
done
t_instance "${T_MINIMAL[@]}" TEHAS_SIZE_MAX_FILES=999999999
t_run "$load && echo \"[\$TEHAS_SIZE_MAX_FILES]\""
t_eq "$t_out" "[999999999]"

for bad in "TEHAS_MILLWRIGHT_LOGIN=two words" "TEHAS_MILLWRIGHT_LOGIN=-x" "TEHAS_MILLWRIGHT_MAX_RETRIES=0" "TEHAS_MILLWRIGHT_MAX_RETRIES=many" \
  "TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS=0" "TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS=-1" "TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS=lots"; do
  t_case "loading rejects $bad"
  t_instance "${T_MINIMAL[@]}" "${bad%%=*}='${bad#*=}'"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "tehas: ${bad%%=*} is not valid"
done

t_case "the millwright is never one of the accounts that authorise work"
t_instance "${T_MINIMAL[@]}" TEHAS_MILLWRIGHT_LOGIN=alice
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_MILLWRIGHT_LOGIN (alice) is in TEHAS_ALLOWED_LABELERS"

t_case "the millwright may be a GitHub App, named by its login with or without [bot]"
for good in acme-millwright 'acme-millwright[bot]'; do
  t_instance "${T_MINIMAL[@]}" "TEHAS_MILLWRIGHT_LOGIN=$good"
  t_run "$load && echo \"[\$TEHAS_MILLWRIGHT_LOGIN]\""
  t_eq "$t_out" "[$good]"
done

for bad in 'acme-millwright[bot][bot]' '[bot]' 'acme-millwright ' 'acme-millwright[bot] acme-other' 'acme-millwright[BOT]' 'acme-millwright[bot]x'; do
  t_case "loading rejects a TEHAS_MILLWRIGHT_LOGIN of '$bad'"
  printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_MILLWRIGHT_LOGIN='$bad'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "tehas: TEHAS_MILLWRIGHT_LOGIN is not valid: expected one GitHub login"
done

t_case "an App's login is the same account as its name without [bot]: the millwright is still never an allowed labeller"
t_instance "${T_MINIMAL[@]}" 'TEHAS_MILLWRIGHT_LOGIN=alice[bot]'
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_MILLWRIGHT_LOGIN (alice[bot]) is in TEHAS_ALLOWED_LABELERS"

# The bot of a GitHub App as both forms name it: REST gives <slug>[bot],
# GraphQL <slug>.
app=(TEHAS_GITHUB_APP_ID=1 TEHAS_GITHUB_APP_INSTALLATION=2)
for pair in 'acme-app[bot] acme-app[bot]' 'acme-app[bot] acme-app' 'acme-app acme-app[bot]'; do
  t_case "the millwright is never the bot: bot ${pair% *}, millwright ${pair#* }"
  t_instance "${T_MINIMAL[@]}" "${app[@]}" "TEHAS_BOT_LOGIN=${pair% *}" "TEHAS_MILLWRIGHT_LOGIN=${pair#* }"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "TEHAS_MILLWRIGHT_LOGIN (${pair#* }) is in TEHAS_ALLOWED_LABELERS or is TEHAS_BOT_LOGIN"
done

t_case "a millwright with a name like the bot's is an account of its own"
t_instance "${T_MINIMAL[@]}" "${app[@]}" 'TEHAS_BOT_LOGIN=acme-app[bot]' 'TEHAS_MILLWRIGHT_LOGIN=acme-app-millwright[bot]'
t_run "$load && echo \"[\$TEHAS_MILLWRIGHT_LOGIN]\""
t_eq "$t_out" "[acme-app-millwright[bot]]"

t_case "an old fixer name stops the load with 78 and names the new one"
for old in TEHAS_FIXER_LOGIN=tehas-mill TEHAS_FIXER_MAX_RETRIES=3 TEHAS_FIXER_MAX_PLAN_ANSWERS=1 TEHAS_FIXER_LOGIN=; do
  t_case "loading rejects $old"
  printf '%s\n' "${T_MINIMAL[@]}" "$old" TEHAS_MILLWRIGHT_LOGIN=tehas-mill >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  name=${old%%=*}
  t_contains "$t_out" "$name is no longer read: rename it TEHAS_MILLWRIGHT_${name#TEHAS_FIXER_}"
done

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
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_LAYOUT=business TEHAS_THEME=beesknees26 >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_LAYOUT][\$TEHAS_THEME]\""
t_eq "$t_out" "[business][beesknees26]"

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

# What the poll reads for `auto` is not exported and not checked here: the
# loader clears what came in with it, and the poll defaults the three itself.
t_case "an inherited auto count is cleared and stays cleared in a child"
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

# The disk hold of the dispatcher (libexec/dispatch): ten gibibytes unless the
# instance says otherwise, the same shape of setting as the preview's.
t_case "cycles are held below 10g of free disk unless the instance says otherwise"
t_instance "${T_MINIMAL[@]}"
t_run "$load && echo \"\$TEHAS_CYCLE_MIN_FREE \$TEHAS_CYCLE_MIN_FREE_MB\""
t_eq "$t_out" "10g 10240"
t_instance "${T_MINIMAL[@]}" TEHAS_CYCLE_MIN_FREE=512m
t_run "$load && echo \"\$TEHAS_CYCLE_MIN_FREE \$TEHAS_CYCLE_MIN_FREE_MB\""
t_eq "$t_out" "512m 512"
t_instance "${T_MINIMAL[@]}"

# The example instance names the default, as it names the preview's.
t_case "the example instance names the minimum the loader defaults to"
t_contains "$(grep '^#TEHAS_CYCLE_MIN_FREE=' "$T_ROOT/instances/example/optional.env")" "=10g"

# The disk hold of the cleaner (libexec/clean): thirty gibibytes unless the
# instance says otherwise, the same shape of setting as the other two. It sits
# above the dispatcher's 10g above, so the cleaner frees space before the poll
# stops starting cycles.
t_case "the cleaner frees space below 30g of free disk unless the instance says otherwise"
t_instance "${T_MINIMAL[@]}"
t_run "$load && echo \"\$TEHAS_CLEAN_MIN_FREE \$TEHAS_CLEAN_MIN_FREE_MB\""
t_eq "$t_out" "30g 30720"
t_instance "${T_MINIMAL[@]}" TEHAS_CLEAN_MIN_FREE=512m
t_run "$load && echo \"\$TEHAS_CLEAN_MIN_FREE \$TEHAS_CLEAN_MIN_FREE_MB\""
t_eq "$t_out" "512m 512"
t_instance "${T_MINIMAL[@]}"

# The example instance names the default, and names it above the poll's hold.
t_case "the example instance names the minimum the cleaner defaults to"
t_contains "$(grep '^#TEHAS_CLEAN_MIN_FREE=' "$T_ROOT/instances/example/optional.env")" "=30g"

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
t_contains "$(grep '^#TEHAS_PLAN_TIMEOUT=' "$T_ROOT/instances/example/optional.env")" "$plan_max"

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

# The wait for the update lock is read before tehas_load by install.sh and
# tehas update, because tehas_load unsets every TEHAS_* it does not know and does
# not name this one: the value the early read took is what the command then uses.
# Two halves, because the loader sourcing instance.env is what would turn the
# knob into a setting, and the two orders of reading it answer differently.
t_case "the environment's update lock wait is used, and the loader drops the name"
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_UPDATE_LOCK_WAIT=3)
t_run '. "$T_ROOT/lib/config.sh"; . "$T_ROOT/lib/update-steps.sh"; a=$(tehas_update_lock_wait); tehas_load >/dev/null 2>&1; b=$(tehas_update_lock_wait); printf "before=%s after=%s\n" "$a" "$b"'
t_eq "$t_out" "before=3 after=10"
T_ENV=()
# tehas_load sources instance.env, so a line there reaches the environment, but
# only after the two commands have read the wait: the instance does not set it.
t_case "instance.env does not set the update lock's wait"
t_instance "${T_MINIMAL[@]}" TEHAS_UPDATE_LOCK_WAIT=3
t_run '. "$T_ROOT/lib/config.sh"; . "$T_ROOT/lib/update-steps.sh"; a=$(tehas_update_lock_wait); tehas_load >/dev/null 2>&1; b=${TEHAS_UPDATE_LOCK_WAIT:-}; printf "read=%s instance_line=%s\n" "$a" "$b"'
t_eq "$t_out" "read=10 instance_line=3"

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

for bad in two 0 -1 1.5 99999999999999999999; do
  t_case "a concurrency that is no number is refused, not added to: $bad"
  t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=$bad
  t_run "$load"
  t_eq "$t_rc" 78
  t_contains "$t_out" "TEHAS_TICKET_CONCURRENCY is not valid"
done

# `auto` is a value the dispatcher answers, not one the loader works out: it
# reaches the environment as written, and nothing here runs nproc for it.
t_case "auto for both counts is kept as written, and no nproc is asked for it"
rm -f "$T/bin/nproc"
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=auto TEHAS_SLICE_CONTAINERS=auto
t_run "$load && echo \"\$TEHAS_TICKET_CONCURRENCY \$TEHAS_SLICE_CONTAINERS\""
t_eq "$t_rc" 0
t_eq "$t_out" "auto auto"
t_eq "$(t_calls nproc)" ""
t_stub nproc 'echo 8'

t_case "auto with the lane off holds a review cap of three, the floor the setting has"
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=auto
t_run "$load && echo \"\$TEHAS_MAX_AWAITING\""
t_eq "$t_out" "3"
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=auto TEHAS_MAX_AWAITING=9
t_run "$load && echo \"\$TEHAS_MAX_AWAITING\""
t_eq "$t_out" "9"

t_case "auto is refused with several workstreams: one axis of parallelism"
t_instance "${T_MINIMAL[@]}" TEHAS_CONCURRENCY=2 TEHAS_TICKET_CONCURRENCY=auto
t_run "$load"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_CONCURRENCY (2) and TEHAS_TICKET_CONCURRENCY (auto) are both above 1"

# The three the poll reads for `auto` are not the loader's: nothing is exported
# and nothing is checked, and the poll holds the rule and the default of each
# with the arithmetic (console/dispatch/src/slots.rs).
t_case "what the instance writes for the auto settings reaches the environment as written"
t_instance "${T_MINIMAL[@]}" TEHAS_CONTAINER_MEMORY=16g TEHAS_CYCLE_CPUS=8 TEHAS_CYCLE_CEILING=6
t_run "$load && echo \"\$TEHAS_CONTAINER_MEMORY \$TEHAS_CYCLE_CPUS \$TEHAS_CYCLE_CEILING\""
t_eq "$t_out" "16g 8 6"

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
  PI_ENSEMBLE_PROVIDER_PLANNER=p PI_ENSEMBLE_MODEL_LENS_SECURITY= PI_ENSEMBLE_MODEL_=m PI_ENSEMBLE_PROVIDER_=p; do
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

# The budget of the idle polls goes to `store poll-idle --budget` as it stands,
# so the store is what reads it; the shape is checked here so an instance that
# writes a word the store cannot use is told at the load and not in a health
# file that says nothing. The store reads the three words in this order, so an
# instance that writes them the other way round would bound the wrong figure.
t_case "the budget of an idle poll takes the default the health check warns at"
t_instance "${T_MINIMAL[@]}"
t_run "$load && set -u && echo \"[\$TEHAS_POLL_BUDGET]\""
t_eq "$t_rc" 0
t_eq "$t_out" "[cpu=5,wall=30,writes=0]"

t_case "the budget of an idle poll can be set in the instance"
t_instance "${T_MINIMAL[@]}" "TEHAS_POLL_BUDGET=cpu=3,wall=20"
t_run "$load && echo \"[\$TEHAS_POLL_BUDGET]\""
t_eq "$t_rc" 0
t_eq "$t_out" "[cpu=3,wall=20]"

for good in "cpu=5" "wall=30" "writes=0" "cpu=0,wall=0,writes=0" "cpu=999999999" \
  "cpu=0" "wall=0" "cpu=5,wall=0" "wall=30,writes=0"; do
  t_case "loading takes a TEHAS_POLL_BUDGET of '$good'"
  t_instance "${T_MINIMAL[@]}" "TEHAS_POLL_BUDGET=$good"
  t_run "$load && echo ok"
  t_eq "$t_out" "ok"
done

# The store's own grammar for a figure of the budget (verbs::count, and the
# same one the store's --budget reads), and no order but the store's: a figure
# with a sign, a fraction, an exponent or ten digits is not one, no name is
# said twice, and a figure of digits is 0 or begins 1-9 -- the store refuses
# 05, and health gives the value to it as it stands, so a leading 0 here would
# take the warning out of the health file in silence. The whole reason the load
# checks this setting: see lib/config.sh.
for bad in "cpu=" "cpu" "cpu=1.5" "cpu=-1" "cpu=1e3" "cpu=1000000000" \
  "cpu=1,cpu=2" "wall=1,cpu=1" "writes=1,cpu=2" "last=1" "cpu=1 wall=2" " cpu=1" "cpu=1 wall=2 writes=0" \
  "cpu=05" "wall=030" "writes=00" "cpu=5,wall=030,writes=0"; do
  t_case "loading rejects a TEHAS_POLL_BUDGET of '$bad'"
  printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_POLL_BUDGET='$bad'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load"
  t_eq "$t_rc" 78
  t_contains "$t_out" "TEHAS_POLL_BUDGET is not valid: expected cpu=<s>,wall=<s>,writes=<n>"
done
t_instance "${T_MINIMAL[@]}"

# The comment in the example instance copies the default by hand.
t_case "the example instance documents the budget the health check warns at"
t_contains "$(grep ^#TEHAS_POLL_BUDGET= "$T_ROOT/instances/example/optional.env")" "=cpu=5,wall=30,writes=0"

# The two bounds of the measure tehas work makes of an approved body. Each is
# asked of head -c and head -n, so a sign, a fraction or an exponent is no
# bound; a body of many short lines is held by the bound on lines alone and a
# body of one huge line by the bound on bytes alone, so both are settings.
t_case "the bounds of the measure of a body take the defaults"
t_instance "${T_MINIMAL[@]}"
t_run "$load && set -u && echo \"[\$TEHAS_SIZE_MAX_BYTES] [\$TEHAS_SIZE_MAX_LINES]\""
t_eq "$t_rc" 0
t_eq "$t_out" "[131072] [1000]"

t_case "the bounds of the measure can both be set in the instance"
t_instance "${T_MINIMAL[@]}" "TEHAS_SIZE_MAX_BYTES=4096" "TEHAS_SIZE_MAX_LINES=50"
t_run "$load && echo \"[\$TEHAS_SIZE_MAX_BYTES] [\$TEHAS_SIZE_MAX_LINES]\""
t_eq "$t_rc" 0
t_eq "$t_out" "[4096] [50]"

for bad in "soon" "0" "-1" "1.5" "1000 " "1e3"; do
  t_case "loading rejects a TEHAS_SIZE_MAX_BYTES of '$bad'"
  printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_SIZE_MAX_BYTES='$bad'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load"
  t_eq "$t_rc" 78
  t_contains "$t_out" "TEHAS_SIZE_MAX_BYTES is not valid: expected a whole number of at least 1"
  t_case "loading rejects a TEHAS_SIZE_MAX_LINES of '$bad'"
  printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_SIZE_MAX_LINES='$bad'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load"
  t_eq "$t_rc" 78
  t_contains "$t_out" "TEHAS_SIZE_MAX_LINES is not valid: expected a whole number of at least 1"
done

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

t_case "a TEHAS_CLEAN_DIRS with .git among other names is refused whatever IFS is"
t_run ". \"$T_ROOT/lib/config.sh\"; IFS=x; _tehas_valid TEHAS_CLEAN_DIRS 'target .git'; echo rc=\$?"
t_contains "$t_out" "refuses .git: that is not build output"
t_contains "$t_out" "rc=1"

# The default the example instance copies by hand is pinned against the
# config's own list, so the two cannot drift apart.
t_case "the example instance documents the defaults the cleaner runs with"
t_instance "${T_MINIMAL[@]}"
t_run "$load && echo \"\$TEHAS_CLEAN_DIRS\""
default_dirs=$t_out
t_eq "$t_rc" 0
t_contains "$(grep '^#TEHAS_CLEAN_AFTER=' "$T_ROOT/instances/example/optional.env")" "=24"
t_contains "$(grep '^#TEHAS_CLEAN_DIRS=' "$T_ROOT/instances/example/optional.env")" "$default_dirs"
t_instance "${T_MINIMAL[@]}"

t_case "the instance's PI_RUKAS_FOLLOW wins over the environment's"
commit40=$(printf 'a%.0s' {1..40})
t_instance "${T_MINIMAL[@]}" "PI_RUKAS_FOLLOW=commit:$commit40"
T_ENV=(PI_RUKAS_FOLLOW=release)
t_run "$load && echo \$PI_RUKAS_FOLLOW"
t_eq "$t_rc" 0
t_eq "$t_out" "commit:$commit40"
T_ENV=()

t_case "each form of PI_RUKAS_FOLLOW loads"
for good in release release:0.14 release:2 branch:main branch:feature/one "commit:$commit40"; do
  t_instance "${T_MINIMAL[@]}" "PI_RUKAS_FOLLOW=$good"
  t_run "$load && echo ok"
  t_eq "$t_out" "ok"
done

t_case "a PI_RUKAS_FOLLOW of any other shape is 78"
for bad in main latest release: release:0.14.1 release:v0 "commit:${commit40:0:39}" "commit:$(printf 'A%.0s' {1..40})" commit: branch: "branch:two words" ""; do
  t_instance "${T_MINIMAL[@]}" "PI_RUKAS_FOLLOW='$bad'"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "PI_RUKAS_FOLLOW is not valid"
done
t_instance "${T_MINIMAL[@]}"

# Every command loads the settings, so a load starts no program but nproc:
# with nothing else on PATH it still loads.
t_case "loading the settings runs no program but nproc"
t_instance "${T_MINIMAL[@]}" "TEHAS_CYCLE_MIN_FREE=2g" "PI_ENSEMBLE_MODEL_OPS=m"
mkdir -p "$T/only"
printf '#!/bin/bash\necho 4\n' >"$T/only/nproc"
chmod +x "$T/only/nproc"
t_run "PATH=\$T/only; $load; echo \"rc=\$? \$TEHAS_CYCLE_MIN_FREE_MB \$TEHAS_TICKET_CONCURRENCY \$PI_ENSEMBLE_IMAGE\""
t_eq "$t_out" "rc=0 2048 3 tehas/pi-rukas:uid$EUID"
t_instance "${T_MINIMAL[@]}"

t_done
