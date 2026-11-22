#!/usr/bin/env bash
# Tests for lib/config.sh: how instance settings are found, loaded, defaulted
# and validated. config_defaults_test.sh holds the second half of the cases.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/config_lib.sh
. "$(dirname "$0")/config_lib.sh" || exit 1

t_case "a minimal instance loads and derives the checkout path"
t_instance "${T_MINIMAL[@]}"
t_run "$load && echo \"\$TEHAS_WORK_REPO|\$TEHAS_HOST_ADDR|\$TEHAS_WORKSTREAMS\""
t_eq "$t_rc" 0
t_eq "$t_out" "$T/home/tehas/repos/widget|127.0.0.1|1"

t_case "settings are exported to child processes"
t_run "$load && bash -c 'echo \$TEHAS_REPO \$PI_ENSEMBLE_SUBAGENT_MODEL \$PI_ENSEMBLE_MAX_WORKSTREAMS'"
t_eq "$t_out" "acme/widget test-model 1"

t_case "a gate gets thirty minutes unless the instance says otherwise, and pi-rukas is told in milliseconds"
t_run "$load && bash -c 'echo \$TEHAS_VERIFY_TIMEOUT \$PI_ENSEMBLE_VERIFY_TIMEOUT_MS \$PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS \$PI_ENSEMBLE_VERIFY_SUITE_TIMEOUT_MS'"
t_eq "$t_out" "1800 1800000 1800000 1800000"
t_instance "${T_MINIMAL[@]}" TEHAS_VERIFY_TIMEOUT=600
t_run "$load && bash -c 'echo \$TEHAS_VERIFY_TIMEOUT \$PI_ENSEMBLE_VERIFY_TIMEOUT_MS \$PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS \$PI_ENSEMBLE_VERIFY_SUITE_TIMEOUT_MS'"
t_eq "$t_out" "600 600000 600000 600000"
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
# The operator's copy and the manual's row are the config's number, and a line
# that leaves the setting empty takes the default rather than leave it unset.
t_contains "$(grep '^#TEHAS_CHILD_TIMEOUT=' "$T_ROOT/instances/example/optional.env")" "$child_max"
t_contains "$(grep '^| .TEHAS_CHILD_TIMEOUT.' "$T_ROOT/host/docs/tools.md")" "| \`$child_max\`"
t_instance "${T_MINIMAL[@]}" TEHAS_CHILD_TIMEOUT=
t_run "$load; echo rc=\$? \$TEHAS_CHILD_TIMEOUT"
t_eq "$t_out" "rc=0 $child_max"
t_instance "${T_MINIMAL[@]}"

t_case "a cycle's tree gets 100 commits of history unless the instance says otherwise, 0 is all of it"
t_run "$load && echo \$TEHAS_CLONE_DEPTH"
t_eq "$t_out" 100
t_contains "$(grep '^#TEHAS_CLONE_DEPTH=' "$T_ROOT/instances/example/optional.env")" "=100"
t_contains "$(grep '^| .TEHAS_CLONE_DEPTH.' "$T_ROOT/host/docs/tools.md")" "| \`100\`"
t_instance "${T_MINIMAL[@]}" TEHAS_CLONE_DEPTH=0
t_run "$load && echo \$TEHAS_CLONE_DEPTH"
t_eq "$t_out" 0
for bad in -1 01 full 1000000000; do
  t_instance "${T_MINIMAL[@]}" "TEHAS_CLONE_DEPTH=$bad"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "TEHAS_CLONE_DEPTH is not valid: expected a whole number of 0 or more"
done
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
# The ceiling tehas_load accepts, and the grace apply/turn.rs adds to it.
t_eq "$plan_max" "7200"
t_contains "$(cat "$T_ROOT/console/dispatch/src/apply/turn.rs")" "self.host.plan_timeout + 900"
t_contains "$(cat "$T_ROOT/host/docs/host.md")" "may be at most $plan_max, 2 hours"
t_contains "$(grep 'at most' "$T_ROOT/instances/example/optional.env")" "at most $plan_max"
# Read as tehas reads it: the ceiling gets a unit bound of the ceiling plus grace.
t_instance "${T_MINIMAL[@]}" TEHAS_PLAN_TIMEOUT="$plan_max"
t_run "$load; echo rc=\$? \$((TEHAS_PLAN_TIMEOUT + 900))"
t_eq "$t_out" "rc=0 $((plan_max + 900))"
t_instance "${T_MINIMAL[@]}"

t_case "a scratch directory is an absolute path"
t_instance "${T_MINIMAL[@]}" TEHAS_SCRATCH_DIR=scratch
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_SCRATCH_DIR is not valid"
t_instance "${T_MINIMAL[@]}" TEHAS_SCRATCH_DIR=/mnt/scratch
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=0"

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
# Past the largest number bash compares: refused rather than wrapped.
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
t_contains "$(cat "$T_ROOT/console/dispatch/src/poll.rs")" "const HANG: u64 = 1800;"

# A timer armed only from inactivity does not fire after a failed poll (one
# stopped the queue on 2026-10-20), so the two arms are pinned together.
t_case "the poll's timer is armed from the last end and from the last start"
t_eq "$(sed -n 's/^OnBootSec=//p' "$T_ROOT/host/systemd/tehas-dispatch.timer")" "2min"
t_eq "$(sed -n 's/^OnUnitInactiveSec=//p' "$T_ROOT/host/systemd/tehas-dispatch.timer")" "2min"
t_eq "$(sed -n 's/^OnUnitActiveSec=//p' "$T_ROOT/host/systemd/tehas-dispatch.timer")" "2min"

t_case "one cycle at a time may run several workstreams, and pi-rukas is told the same number"
t_instance "${T_MINIMAL[@]}" TEHAS_WORKSTREAMS=4 TEHAS_MAX_CYCLES=1
t_run "$load && echo \"\$TEHAS_WORKSTREAMS \$TEHAS_MAX_CYCLES \$PI_ENSEMBLE_MAX_WORKSTREAMS\""
t_eq "$t_rc" 0
t_eq "$t_out" "4 1 4"

t_case "several cycles and several workstreams are refused: one axis of parallelism"
t_instance "${T_MINIMAL[@]}" TEHAS_WORKSTREAMS=2 TEHAS_MAX_CYCLES=3
t_run "$load"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_WORKSTREAMS (2) and TEHAS_MAX_CYCLES (3) are both above 1: choose one"

t_case "an instance line for pi-rukas's workstreams is replaced by tehas' value"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_MAX_WORKSTREAMS=4
t_run "$load && echo \"\$TEHAS_WORKSTREAMS \$PI_ENSEMBLE_MAX_WORKSTREAMS\""
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
t_instance TEHAS_CONTROL_ISSUE=1 TEHAS_GITHUB_APP_ID=1
T_ENV=(TEHAS_REPO=other/thing TEHAS_SITE_INDEXING=1 PI_ENSEMBLE_DEBUG=1)
t_run "$load && echo \"[\${TEHAS_REPO:-}] [\$TEHAS_SITE_INDEXING] [\${PI_ENSEMBLE_DEBUG:-}]\""
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

t_case "an instance that still sets the attachment kinds that left core loads, and each is dropped"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1 TEHAS_WORK_DESCRIBE=1 TEHAS_WORK_FILES=1 TEHAS_WORK_ZIPS=1 TEHAS_WORK_COMMENTS=1
t_run "$load; echo rc=\$? \$TEHAS_WORK_IMAGES [\${TEHAS_WORK_FILES-unset}]"
t_contains "$t_out" "rc=0 1 [unset]"
t_contains "$t_out" "TEHAS_WORK_ZIPS is no longer read"

t_case "the test hooks survive from the environment"
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_WORK_CMD=fake-work TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index
  TEHAS_CYCLE_START_CMD=fake-start TEHAS_CYCLE_START_WAIT=2 TEHAS_PREFLIGHT_WAIT=3 TEHAS_STOP_WAIT=4 TEHAS_FETCH_WAIT=5
  TEHAS_CYCLE_RETRY_WAIT=6 TEHAS_CYCLE_STOP_GH_WAIT=7 TEHAS_CONSOLE_WAIT=8 TEHAS_CYCLE_STOP_CMD=fake-stop
  TEHAS_TIDY_CMD=fake-tidy TEHAS_TIDY_WAIT=9 TEHAS_ANALYTICS_WAIT=1 TEHAS_BUILD_TIMEOUT=600)
t_run "$load && echo \"\$TEHAS_WORK_CMD \$TEHAS_STATUS_CMD \$TEHAS_INDEX_CMD \$TEHAS_CYCLE_START_CMD \$TEHAS_CYCLE_START_WAIT \$TEHAS_PREFLIGHT_WAIT \$TEHAS_STOP_WAIT \$TEHAS_FETCH_WAIT \$TEHAS_CYCLE_RETRY_WAIT \$TEHAS_CYCLE_STOP_GH_WAIT \$TEHAS_CONSOLE_WAIT \$TEHAS_CYCLE_STOP_CMD \$TEHAS_TIDY_CMD \$TEHAS_TIDY_WAIT \$TEHAS_ANALYTICS_WAIT \$TEHAS_BUILD_TIMEOUT\""
t_eq "$t_out" "fake-work fake-status fake-index fake-start 2 3 4 5 6 7 8 fake-stop fake-tidy 9 1 600"
T_ENV=()

t_case "the poll's word to tehasbot status survives from the environment"
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_FROM_POLL=1)
t_run "$load && echo \"\$TEHAS_FROM_POLL\""
t_eq "$t_out" "1"
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

t_case "tehas_require_instance_settings names an unset setting and exits 78"
t_instance TEHAS_CONTROL_ISSUE=1 TEHAS_GITHUB_APP_ID=1
t_run "$load && tehas_require_instance_settings; echo not-reached"
t_eq "$t_rc" 78
t_contains "$t_out" "tehas: TEHAS_REPO is not set (see $T/instance/instance.env)"
t_lacks "$t_out" "not-reached"

t_case "tehas_require_instance_settings rejects an empty setting"
t_instance "${T_MINIMAL[@]}" TEHAS_ALLOWED_LABELERS=
t_run "$load && tehas_require_instance_settings"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_ALLOWED_LABELERS is not set"

t_case "the instance settings every command requires are one list"
t_instance "${T_MINIMAL[@]}"
t_run "$load && tehas_require_instance_settings && echo ok"
t_eq "$t_rc" 0
t_contains "$t_out" "ok"

# tehas_load hands over the console's list as _tehas_required, the names
# `console settings required` answers; a copy in lib/config.sh fails this case.
t_case "the list tehas_require_instance_settings checks is the console's"
t_instance "${T_MINIMAL[@]}"
t_run "$load && printf '%s\n' \"\${_tehas_required[@]}\""
instance_settings=$t_out
t_run "\"$T_CONSOLE\" settings required"
t_eq "$t_out" "$instance_settings"

read -ra required_names <<<"$instance_settings"
for missing in "${required_names[@]}"; do
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
  t_case "tehas_require_instance_settings rejects $bad"
  # Written quoted so a space or semicolon reaches the variable intact.
  printf '%s\n' "${T_MINIMAL[@]}" "${bad%%=*}='${bad#*=}'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load && tehas_require_instance_settings"
  t_eq "$t_rc" 78
  t_contains "$t_out" "tehas: ${bad%%=*} is not valid"
done

for bad in "TEHAS_WORKSTREAMS=auto" "TEHAS_WORKSTREAMS=0" "TEHAS_MAX_CYCLES=0" "TEHAS_MAX_CYCLES=two" "TEHAS_MAX_AWAITING=-1" "TEHAS_WORK_TIMEOUT=soon" "TEHAS_WORK_STALL=0" "TEHAS_WORK_STALL=1h" "TEHAS_HOST_ADDR=not an address" \
  "TEHAS_BRANCH=two words" "TEHAS_SITE_INDEXING=yes" "TEHAS_WORK_IMAGES=yes" "TEHAS_WORK_IMAGES=2" "TEHAS_TOP=yes" "TEHAS_TOP=2" "TEHAS_SITE_ANALYTICS_SRC=http://plain.example/ea.js" \
  "TEHAS_SITE_ANALYTICS_SRC=https://a.example/x.js\" onload=\"x" "TEHAS_SITE_ANALYTICS_NAME=two words" \
  "TEHAS_SITE_ANALYTICS_COOKIE=yes" "TEHAS_DESIGN=paper" "TEHAS_DESIGN=Pixel" "TEHAS_PROTOLAB_URL=https://lab.example/" "TEHAS_PROTOLAB_URL=lab.example" \
  "TEHAS_PLAN_MAX_TURNS=0" "TEHAS_PLAN_TIMEOUT=1h" "TEHAS_VERIFY_TIMEOUT=30m" "TEHAS_VERIFY_TIMEOUT=0" \
  "TEHAS_CHILD_TIMEOUT=30m" "TEHAS_CHILD_TIMEOUT=0" "TEHAS_CHILD_TIMEOUT=-1" "TEHAS_CHILD_TIMEOUT= \t" \
  "TEHAS_BUILD_TIMEOUT=30m" "TEHAS_BUILD_TIMEOUT=0" \
  "TEHAS_DOMAIN=https://example.org" "TEHAS_DOMAIN=example.org/" "TEHAS_DOMAIN=Example.org" \
  "TEHAS_DOMAIN=localhost" "TEHAS_DOMAIN=-a.example.org" "TEHAS_PREVIEWS=yes" "TEHAS_EGRESS=yes" "TEHAS_EGRESS_HOSTS=*.example.org" \
  "TEHAS_PREVIEW_MAX=0" "TEHAS_PREVIEW_MAX=100" "TEHAS_PREVIEW_MIN_FREE=lots" "TEHAS_CYCLE_MIN_FREE=lots" "TEHAS_CLEAN_MIN_FREE=lots" \
  "TEHAS_ASSIGNEE=acme-app[bot]" "TEHAS_ASSIGNEE=alice bob" \
  "TEHAS_RUKAS_WORKAROUNDS=PI_ENSEMBLE_X" "TEHAS_RUKAS_WORKAROUNDS=PI_ENSEMBLE_X@soon" "TEHAS_RUKAS_WORKAROUNDS=PATH@1.2" \
  "TEHAS_RUKAS_WORKAROUNDS=PI_ENSEMBLE_X@1.2  TEHAS_TOP@1.3" "TEHAS_RUKAS_WORKAROUNDS=PI_ENSEMBLE_X@1" \
  "TEHAS_BRANCH_CHECK=verify&x=1" "TEHAS_BRANCH_CHECK=two  spaces" "TEHAS_BRANCH_CHECK= verify" "TEHAS_BRANCH_CHECK=a\"b"; do
  t_case "loading rejects $bad"
  printf '%s\n' "${T_MINIMAL[@]}" "${bad%%=*}='${bad#*=}'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  # The suffix belongs to `auto` alone, and that count is TEHAS_WORKSTREAMS.
  [ "${bad%%=*}" = TEHAS_WORKSTREAMS ] || t_lacks "$t_out" "auto sizing is removed"
  t_contains "$t_out" "tehas: ${bad%%=*} is not valid"
done

t_case "outsource is off unless the instance turns it on, and the bounds are the defaults"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && set -u && echo \"[\$TEHAS_OUTSOURCE][\$TEHAS_OUTSOURCE_REPOS]\""
t_eq "$t_out" "[0][]"
t_run "$load && echo \"[\$TEHAS_OUTSOURCE_TIMEOUT][\$TEHAS_OUTSOURCE_GRACE][\$TEHAS_OUTSOURCE_RETRIES][\$TEHAS_OUTSOURCE_REST][\$TEHAS_OUTSOURCE_JOIN_TTL]\""
t_eq "$t_out" "[600][120][2][1800][3600]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_OUTSOURCE=on "TEHAS_OUTSOURCE_REPOS='acme/widget other/widget'" >"$TEHAS_INSTANCE_DIR/instance.env"
T_ENV=(TEHAS_TEST_FEATURES=outsource)
t_run "$load && echo \"[\$TEHAS_OUTSOURCE][\$TEHAS_OUTSOURCE_REPOS]\""
t_eq "$t_out" "[1][acme/widget other/widget]"
T_ENV=()

t_case "outsource is still being built: its switch set on stops the load with 78"
for value in on 1; do
  printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_OUTSOURCE=$value" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "tehas: TEHAS_OUTSOURCE is on and outsource is still being built in this release"
done

for bad in "TEHAS_OUTSOURCE=yes" "TEHAS_OUTSOURCE=On" "TEHAS_OUTSOURCE=2" \
  "TEHAS_OUTSOURCE_TIMEOUT=soon" "TEHAS_OUTSOURCE_TIMEOUT=0" "TEHAS_OUTSOURCE_TIMEOUT=10m" \
  "TEHAS_OUTSOURCE_GRACE=-1" "TEHAS_OUTSOURCE_RETRIES=two" "TEHAS_OUTSOURCE_RETRIES=0" \
  "TEHAS_OUTSOURCE_REST=1h" "TEHAS_OUTSOURCE_JOIN_TTL=0" \
  "TEHAS_OUTSOURCE_REPOS=acme" "TEHAS_OUTSOURCE_REPOS=acme/widget  other/widget" \
  "TEHAS_OUTSOURCE_REPOS= acme/widget" "TEHAS_OUTSOURCE_REPOS=acme/widget " \
  "TEHAS_OUTSOURCE_REPOS=acme/widget;rm"; do
  t_case "loading rejects $bad"
  printf '%s\n' "${T_MINIMAL[@]}" "${bad%%=*}='${bad#*=}'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "tehas: ${bad%%=*} is not valid"
done

t_case "a command fails closed on a bad TEHAS_OUTSOURCE before it calls GitHub"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_OUTSOURCE=maybe >"$TEHAS_INSTANCE_DIR/instance.env"
t_stub gh
for cmd in "bin/tehasbot cycle 7" "bin/tehasbot dispatch"; do
  t_run "TEHAS_CONSOLE=\"\$TEHAS_SETTINGS_CMD\" \"\$T_ROOT/\"$cmd"
  t_eq "$t_rc" 78
  t_contains "$t_out" "TEHAS_OUTSOURCE is not valid"
  t_eq "$(t_calls gh)" ""
done

t_case "the owner is optional: unset it is empty, set it is the login"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && set -u && echo \"[\$TEHAS_ASSIGNEE]\""
t_eq "$t_out" "[]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_ASSIGNEE=alice >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_ASSIGNEE]\""
t_eq "$t_out" "[alice]"

t_case "a setting of the improvement loop, gone, stops the load with 78 and names itself"
for v in TEHAS_IMPROVE_REPO=acme/backlog TEHAS_IMPROVE_RUKAS_REPO=acme/rukas \
  TEHAS_IMPROVE_RUKAS_SKIP=1 TEHAS_IMPROVE_MAX_OPEN=30; do
  printf '%s\n' "${T_MINIMAL[@]}" "$v" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "${v%%=*} is no longer read: the improvement loop is removed, so take the line out"
done

t_case "a blank setting of the gone loop or report, and a skip of 0, load as if left out"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO= TEHAS_IMPROVE_RUKAS_REPO= TEHAS_IMPROVE_RUKAS_SKIP=0 \
  TEHAS_IMPROVE_MAX_OPEN= TEHAS_CYCLE_REPORT= >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=0"
t_lacks "$t_out" "no longer read"

t_case "a setting of the review loop, gone, loads with one warning and is dropped"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_REVIEW=on TEHAS_REVIEW_REPO=acme/backlog >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load; echo rc=\$? \"[\${TEHAS_REVIEW-unset}][\${TEHAS_REVIEW_REPO-unset}]\""
t_contains "$t_out" "rc=0 [unset][unset]"
for name in TEHAS_REVIEW TEHAS_REVIEW_REPO; do
  t_contains "$t_out" "$name is no longer read: the review loop left core in 2.0 and returns as a plugin; remove it from the instance"
done

t_case "the millwright may retry twice and answer three planning questions by default"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && set -u && echo \"[\$TEHAS_MILLWRIGHT_MAX_RETRIES][\$TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS]\""
t_eq "$t_out" "[2][3]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_MILLWRIGHT_MAX_RETRIES=3 TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS=1 >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_MILLWRIGHT_MAX_RETRIES][\$TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS]\""
t_eq "$t_out" "[3][1]"

t_case "the millwright's feed is off unless the instance turns it on, two at once by default"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && set -u && echo \"[\$TEHAS_MILLWRIGHT_FEED][\$TEHAS_MILLWRIGHT_MAX_FEEDS]\""
t_eq "$t_out" "[0][2]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_MILLWRIGHT_FEED=1 TEHAS_MILLWRIGHT_MAX_FEEDS=4 >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_MILLWRIGHT_FEED][\$TEHAS_MILLWRIGHT_MAX_FEEDS]\""
t_eq "$t_out" "[1][4]"

t_case "the millwright's epics are off unless the instance turns them on"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && set -u && echo \"[\$TEHAS_MILLWRIGHT_EPICS]\""
t_eq "$t_out" "[0]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_MILLWRIGHT_EPICS=1 >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_MILLWRIGHT_EPICS]\""
t_eq "$t_out" "[1]"

t_case "two refreshes of one pull request and a lane of one by default"
t_instance "${T_MINIMAL[@]}"
t_run "$load && set -u && echo \"[\$TEHAS_REFRESH_MAX_PER_PR][\$TEHAS_REFRESH_SLOTS]\""
t_eq "$t_out" "[2][1]"
t_instance "${T_MINIMAL[@]}" TEHAS_REFRESH_MAX_PER_PR=5 TEHAS_REFRESH_SLOTS=8
t_run "$load && echo \"[\$TEHAS_REFRESH_MAX_PER_PR][\$TEHAS_REFRESH_SLOTS]\""
t_eq "$t_out" "[5][8]"

t_case "the settings of the timed refresh scan are gone: a value they took loads and is dropped, any other is refused"
for line in TEHAS_REFRESH_CONFLICTS=0 TEHAS_REFRESH_CONFLICTS=1 TEHAS_REFRESH_CONFLICTS= TEHAS_REFRESH_EVERY= \
  TEHAS_REFRESH_EVERY=10 TEHAS_REFRESH_EVERY=1440; do
  t_instance "${T_MINIMAL[@]}" "$line"
  t_run "$load && echo \"[\${TEHAS_REFRESH_CONFLICTS-unset}][\${TEHAS_REFRESH_EVERY-unset}]\""
  t_eq "$line $t_out" "$line [unset][unset]"
done
for line in TEHAS_REFRESH_CONFLICTS=yes TEHAS_REFRESH_CONFLICTS=2 TEHAS_REFRESH_EVERY=0 TEHAS_REFRESH_EVERY=1441 \
  TEHAS_REFRESH_EVERY=010 TEHAS_REFRESH_EVERY=10m TEHAS_REFRESH_EVERY=+10 TEHAS_REFRESH_EVERY=+1; do
  t_instance "${T_MINIMAL[@]}" "$line"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "${line%%=*} is no longer read: the timed refresh scan is removed and tehas refresh --pr N asks for the refresh of one pull request, so take the line out"
done
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_REFRESH_SLOTS=0 >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_REFRESH_SLOTS]\""
t_eq "$t_out" "[0]"

t_case "the easy merge is gone: its setting loads blank or 0 and is refused otherwise"
for line in "TEHAS_EASY_MERGE=" "TEHAS_EASY_MERGE=0"; do
  printf '%s\n' "${T_MINIMAL[@]}" "$line" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load && echo \"[\${TEHAS_EASY_MERGE-unset}]\""
  t_eq "$t_out" "[unset]"
done
for bad in "TEHAS_EASY_MERGE=1" "TEHAS_EASY_MERGE=yes"; do
  t_case "loading refuses $bad"
  printf '%s\n' "${T_MINIMAL[@]}" "$bad" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "tehas: TEHAS_EASY_MERGE is no longer read: the easy merge is removed"
done

t_case "the overlap hold is gone: its setting is refused unless blank or 0, and says what holds an issue instead"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\${TEHAS_OVERLAP_HOLD-unset}]\""
t_eq "$t_out" "[unset]"
for v in '' 0; do
  printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_OVERLAP_HOLD=$v" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load && echo \"[\${TEHAS_OVERLAP_HOLD-unset}]\""
  t_eq "$t_out" "[unset]"
done
for v in 1 yes; do
  printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_OVERLAP_HOLD=$v" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "tehas: TEHAS_OVERLAP_HOLD is no longer read: a host holds an issue out of the start order with tehas order set --only, so take the line out"
done

t_case "read-only is off unless the instance turns it on, and a value that is not 0 or 1 stops the load"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && set -u && echo \"[\$TEHAS_READ_ONLY]\""
t_eq "$t_out" "[0]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_READ_ONLY=1 >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_READ_ONLY]\""
t_eq "$t_out" "[1]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_READ_ONLY=yes >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "tehas: TEHAS_READ_ONLY is not valid: expected on or off"

t_case "the egress limit is off and its hosts empty unless the instance says otherwise"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && set -u && echo \"[\$TEHAS_EGRESS] [\$TEHAS_EGRESS_HOSTS]\""
t_eq "$t_out" "[0] []"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_EGRESS=1 'TEHAS_EGRESS_HOSTS="pypi.org m.example:8443"' >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && set -u && echo \"[\$TEHAS_EGRESS] [\$TEHAS_EGRESS_HOSTS]\""
t_eq "$t_out" "[1] [pypi.org m.example:8443]"

for bad in "TEHAS_REFRESH_MAX_PER_PR=0" "TEHAS_REFRESH_MAX_PER_PR=-1" "TEHAS_REFRESH_MAX_PER_PR=many" \
  "TEHAS_REFRESH_SLOTS=9" "TEHAS_REFRESH_SLOTS=-1" "TEHAS_REFRESH_SLOTS=01" "TEHAS_REFRESH_SLOTS=one"; do
  t_case "loading rejects $bad"
  printf '%s\n' "${T_MINIMAL[@]}" "${bad%%=*}='${bad#*=}'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
done

t_case "the slice holds four containers unless the instance says another number from 2 to 16"
t_instance "${T_MINIMAL[@]}"
t_run "$load && echo \"[\$TEHAS_SLICE_CONTAINERS]\""
t_eq "$t_out" "[4]"
for good_n in 2 6 16; do
  t_instance "${T_MINIMAL[@]}" TEHAS_SLICE_CONTAINERS=$good_n
  t_run "$load && echo \"[\$TEHAS_SLICE_CONTAINERS]\""
  t_eq "$t_out" "[$good_n]"
done
# `auto` is the poll's word, but the cycle refuses it (tehas_core::slice) and
# parks, so the loader refuses it until the cycle can use the poll's count.
for bad in 1 0 17 06 1.5 -4 auto AUTO 99999999999999999999; do
  t_case "loading rejects TEHAS_SLICE_CONTAINERS=$bad"
  t_instance "${T_MINIMAL[@]}" "TEHAS_SLICE_CONTAINERS=$bad"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  # The suffix is for `auto` alone: it is not said of a number out of range.
  if [ "$bad" = auto ]; then
    t_contains "$t_out" "TEHAS_SLICE_CONTAINERS is not valid: expected a whole number from 2 to 16: auto sizing is removed"
  else
    t_contains "$t_out" "TEHAS_SLICE_CONTAINERS is not valid: expected a whole number from 2 to 16"
    t_lacks "$t_out" "auto sizing is removed"
  fi
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
# so a long number stops the load rather than reach the dispatcher or tehas limit.
for name in TEHAS_WORK_TIMEOUT TEHAS_WORKSTREAMS TEHAS_MAX_CYCLES TEHAS_VERIFY_TIMEOUT \
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

for bad in "TEHAS_MILLWRIGHT_MAX_RETRIES=0" "TEHAS_MILLWRIGHT_MAX_RETRIES=many" \
  "TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS=0" "TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS=-1" "TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS=lots" \
  "TEHAS_MILLWRIGHT_FEED=yes" "TEHAS_MILLWRIGHT_FEED=2" "TEHAS_MILLWRIGHT_MAX_FEEDS=0" "TEHAS_MILLWRIGHT_MAX_FEEDS=many" \
  "TEHAS_MILLWRIGHT_EPICS=yes" "TEHAS_MILLWRIGHT_EPICS=2"; do
  t_case "loading rejects $bad"
  t_instance "${T_MINIMAL[@]}" "${bad%%=*}='${bad#*=}'"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "tehas: ${bad%%=*} is not valid"
done

t_case "the app's login file gives the bot and the millwright their login, and the old app settings stop the load"
mkdir -p "$T/home/tehas/state/github"
printf '%s\n' 'new-app[bot]' >"$T/home/tehas/state/github/login"
t_instance "${T_MINIMAL[@]}"
t_run "$load; echo \"[\$TEHAS_BOT_LOGIN][\$TEHAS_MILLWRIGHT_LOGIN]\""
t_eq "$t_out" "[new-app[bot]][new-app[bot]]"
for line in TEHAS_GITHUB_APP_INSTALLATION=2 'TEHAS_BOT_LOGIN=old[bot]' 'TEHAS_MILLWRIGHT_LOGIN=mw[bot]'; do
  t_instance "${T_MINIMAL[@]}" "$line"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "tehas: ${line%%=*} is no longer read: "
  t_contains "$t_out" "rc=78"
done
rm -rf "$T/home/tehas/state/github"

t_case "GH_CYCLE_TOKEN and TEHAS_CYCLE_LOGIN stop the load, and so does GH_TOKEN in the factory's credential file"
refusal="is no longer used: the factory acts as a GitHub App. Run tehas setup --new-app or give --app-id and --app-key-file."
for line in GH_CYCLE_TOKEN=x TEHAS_CYCLE_LOGIN=acme-cycle; do
  t_instance "${T_MINIMAL[@]}" "$line"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "tehas: ${line%%=*} $refusal"
  t_contains "$t_out" "rc=78"
done
t_instance "${T_MINIMAL[@]}"
mkdir -p "$T/home/.config/tehas"
echo GH_TOKEN=t0 >"$T/home/.config/tehas/env"
t_run "$load; echo rc=\$?"
t_contains "$t_out" "tehas: GH_TOKEN $refusal"
t_contains "$t_out" "rc=78"
rm -rf "$T/home/.config/tehas"
T_ENV=(GH_TOKEN=other)
t_run "$load; echo rc=\$?"
t_eq "$t_out" "rc=0"
T_ENV=()

t_case "an instance without TEHAS_GITHUB_APP_ID is refused and the line says where it comes from"
t_instance "${T_MINIMAL[@]:1}"
t_run "$load; echo rc=\$?"
t_contains "$t_out" "tehas: TEHAS_GITHUB_APP_ID is not set: run tehas setup"
t_contains "$t_out" "rc=78"
t_instance "${T_MINIMAL[@]}"

t_case "an old fixer name stops the load with 78 and names the new one"
for old in TEHAS_FIXER_LOGIN=tehas-mill TEHAS_FIXER_MAX_RETRIES=3 TEHAS_FIXER_MAX_PLAN_ANSWERS=1 TEHAS_FIXER_LOGIN=; do
  t_case "loading rejects $old"
  printf '%s\n' "${T_MINIMAL[@]}" "$old" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  name=${old%%=*}
  if [ "$name" = TEHAS_FIXER_LOGIN ]; then
    t_contains "$t_out" "$name is no longer read: the millwright is the GitHub App's login"
  else
    t_contains "$t_out" "$name is no longer read: rename it TEHAS_MILLWRIGHT_${name#TEHAS_FIXER_}"
  fi
done

t_case "analytics is off unless the instance names a script and a site"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_SITE_ANALYTICS_SRC][\$TEHAS_SITE_ANALYTICS_NAME][\$TEHAS_SITE_ANALYTICS_COOKIE]\""
t_eq "$t_out" "[][][0]"

t_case "analytics settings are loaded as written"
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_SITE_ANALYTICS_SRC=https://stats.example/ea.js" "TEHAS_SITE_ANALYTICS_NAME=widget" \
  "TEHAS_SITE_ANALYTICS_COOKIE=1" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_SITE_ANALYTICS_SRC][\$TEHAS_SITE_ANALYTICS_NAME][\$TEHAS_SITE_ANALYTICS_COOKIE]\""
t_eq "$t_out" "[https://stats.example/ea.js][widget][1]"

t_case "the whole site is beesknees26 unless the instance says pixel"
for pair in "|beesknees26" "TEHAS_DESIGN=|beesknees26" "TEHAS_DESIGN=beesknees26|beesknees26" "TEHAS_DESIGN=pixel|pixel"; do
  t_instance "${T_MINIMAL[@]}" "${pair%%|*}"
  t_run "$load; echo rc=\$? \"[\$TEHAS_DESIGN]\""
  t_eq "$t_out" "rc=0 [${pair#*|}]"
done
t_instance "${T_MINIMAL[@]}"

t_case "a design this version does not draw stops every command"
t_instance "${T_MINIMAL[@]}" TEHAS_DESIGN=paper
t_run "$load; echo rc=\$?"
t_contains "$t_out" "tehas: TEHAS_DESIGN is not valid: expected beesknees26 or pixel"
t_contains "$t_out" "rc=78"
t_instance "${T_MINIMAL[@]}"

t_case "the default driver is classic unless the instance says lean"
for pair in "|classic" "TEHAS_DRIVER=|classic" "TEHAS_DRIVER=classic|classic"; do
  t_instance "${T_MINIMAL[@]}" "${pair%%|*}"
  t_run "$load; echo rc=\$? \"[\$TEHAS_DRIVER]\""
  t_eq "$t_out" "rc=0 [${pair#*|}]"
done
# lean is still being built: a test build turns it on.
t_instance "${T_MINIMAL[@]}" TEHAS_DRIVER=lean TEHAS_LEAN=on
saved_env=(${T_ENV[@]+"${T_ENV[@]}"})
T_ENV=(${T_ENV[@]+"${T_ENV[@]}"} TEHAS_TEST_FEATURES=lean)
t_run "$load; echo rc=\$? \"[\$TEHAS_DRIVER]\""
t_eq "$t_out" "rc=0 [lean]"
T_ENV=(${saved_env[@]+"${saved_env[@]}"})
t_instance "${T_MINIMAL[@]}"

t_case "lean is still being built: its switch and a lean default stop every command"
t_instance "${T_MINIMAL[@]}" TEHAS_LEAN=on
t_run "$load; echo rc=\$?"
t_contains "$t_out" "tehas: TEHAS_LEAN is on and lean is still being built in this release: set it to off or take the line out"
t_contains "$t_out" "rc=78"
t_instance "${T_MINIMAL[@]}" TEHAS_DRIVER=lean
t_run "$load; echo rc=\$?"
t_contains "$t_out" "tehas: TEHAS_DRIVER=lean needs the feature lean, and lean is still being built in this release; set TEHAS_DRIVER to classic or take the line out"
t_contains "$t_out" "rc=78"
t_instance "${T_MINIMAL[@]}"

t_case "a driver this version does not know stops every command"
t_instance "${T_MINIMAL[@]}" TEHAS_DRIVER=work-lean
t_run "$load; echo rc=\$?"
t_contains "$t_out" "tehas: TEHAS_DRIVER is not valid: expected classic or lean"
t_contains "$t_out" "rc=78"
t_instance "${T_MINIMAL[@]}"

t_case "a retired theme or layout line stops every command, with its reason"
for pair in "TEHAS_THEME=pixel|TEHAS_DESIGN draws the whole site, beesknees26 by default or pixel, so take the line out" \
  "TEHAS_THEME=neon|TEHAS_DESIGN draws the whole site, beesknees26 by default or pixel, so take the line out" \
  "TEHAS_LAYOUT=floor|the front page follows TEHAS_DESIGN, the business page for beesknees26 and the factory floor for pixel, so take the line out"; do
  line=${pair%%|*}
  why=${pair#*|}
  t_instance "${T_MINIMAL[@]}" "$line"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "tehas: ${line%%=*} is no longer read: $why (see $T/instance/instance.env)"
  t_contains "$t_out" "rc=78"
done
t_instance "${T_MINIMAL[@]}"

t_case "a theme or layout line that asks for the default design loads, and is gone after it"
for line in "TEHAS_THEME=" "TEHAS_THEME=beesknees26" "TEHAS_LAYOUT=" "TEHAS_LAYOUT=business"; do
  t_instance "${T_MINIMAL[@]}" "$line"
  t_run "$load; echo rc=\$? \"[\${${line%%=*}:-}]\""
  t_eq "$t_out" "rc=0 []"
done
t_instance "${T_MINIMAL[@]}"

t_case "each frontend has its page in the tree"
for page in index.html business.html; do
  [ -f "$T_ROOT/host/fun/$page" ] || t_bad "host/fun/$page is missing"
done

t_case "an analytics script without a site is refused"
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_SITE_ANALYTICS_SRC=https://stats.example/ea.js" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_SITE_ANALYTICS_NAME is not set"

t_done
