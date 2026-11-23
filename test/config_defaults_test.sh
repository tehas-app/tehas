#!/usr/bin/env bash
# Tests for lib/config.sh, split from config_test.sh: the switches, each
# setting's default and bounds, and what a load leaves in the environment.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/config_lib.sh
. "$(dirname "$0")/config_lib.sh" || exit 1

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

t_case "TEHAS_ANALYTICS takes 1 and 0 as every switch does, and exports on and off"
for pair in 1:on 0:off; do
  printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ANALYTICS=${pair%%:*}" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load && echo \"[\$TEHAS_ANALYTICS]\""
  t_eq "$t_out" "[${pair#*:}]"
done

t_case "every switch takes on and off, and exports the form its readers compare with"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_SITE_INDEXING=on TEHAS_READ_ONLY=off TEHAS_TOP=1 >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_SITE_INDEXING][\$TEHAS_READ_ONLY][\$TEHAS_TOP]\""
t_eq "$t_out" "[1][0][1]"

for bad in TEHAS_ANALYTICS=On TEHAS_ANALYTICS=yes TEHAS_ANALYTICS=2 "TEHAS_ANALYTICS=on off"; do
  t_case "loading rejects $bad, and says what it takes"
  printf '%s\n' "${T_MINIMAL[@]}" "${bad%%=*}='${bad#*=}'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "tehas: TEHAS_ANALYTICS is not valid: expected on or off"
done

t_case "the switch of the factory's analytics and the visitor analytics' cookie are checked apart"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_ANALYTICS=off TEHAS_SITE_ANALYTICS_COOKIE=1 >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_ANALYTICS][\$TEHAS_SITE_ANALYTICS_COOKIE]\""
t_eq "$t_out" "[off][1]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_SITE_ANALYTICS_COOKIE=maybe >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load; echo rc=\$?"
t_contains "$t_out" "TEHAS_SITE_ANALYTICS_COOKIE is not valid: expected on or off"

t_case "a command fails closed on a bad TEHAS_ANALYTICS before it calls GitHub"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_ANALYTICS=maybe >"$TEHAS_INSTANCE_DIR/instance.env"
t_stub gh
for cmd in "bin/tehasbot cycle 7" "bin/tehasbot dispatch"; do
  t_run "TEHAS_CONSOLE=\"\$TEHAS_SETTINGS_CMD\" \"\$T_ROOT/\"$cmd"
  t_eq "$t_rc" 78
  t_contains "$t_out" "TEHAS_ANALYTICS is not valid"
  t_eq "$(t_calls gh)" ""
done

t_case "several labellers separated by single spaces are accepted"
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ALLOWED_LABELERS='alice bob-2 carol'" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && tehas_require_instance_settings && echo \"[\$TEHAS_ALLOWED_LABELERS]\""
t_eq "$t_out" "[alice bob-2 carol]"

# The budget of an earlier version: the slice on the host is the limit now, so a
# budget is refused, not run with a limit the instance did not ask for.
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

# A name of the family that came in with the environment is cleared.
t_case "an inherited sizing name is cleared and stays cleared in a child"
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

# An earlier version read a blank TEHAS_CLONES as no clones, so it still loads.
t_case "clones written as nothing still load, and the name is not exported"
t_instance "${T_MINIMAL[@]}" TEHAS_CLONES=
t_run "$load; echo rc=\$?; declare -x TEHAS_CLONES 2>&1 | grep -c TEHAS_CLONES"
t_eq "$t_out" "rc=0
0"

t_case "clones written any other way are refused"
t_instance "${T_MINIMAL[@]}" TEHAS_CLONES=00
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_CLONES is set and this version of tehas has no database clones"

# tehas_load drops every TEHAS_* name before instance.env is read, so a retired
# name comes only from the file, and the loop reads the names after the source.
t_case "TEHAS_CLONES in the environment alone is dropped, not refused"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
T_ENV=(TEHAS_CLONES=9)
t_run "$load; echo rc=\$?; echo \"[\${TEHAS_CLONES-unset}]\""
t_eq "$t_out" "rc=0
[unset]"
T_ENV=()

t_case "TEHAS_WORK_CPUS in the environment alone is dropped, not refused"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
T_ENV=(TEHAS_WORK_CPUS=4)
t_run "$load; echo rc=\$?; echo \"[\${TEHAS_WORK_CPUS-unset}]\""
t_eq "$t_out" "rc=0
[unset]"
T_ENV=()

t_case "TEHAS_DISPATCHER that asks for the bash poll is refused, and says what to do"
for v in bash perl; do
  t_instance "${T_MINIMAL[@]}" TEHAS_DISPATCHER=$v
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "tehas: TEHAS_DISPATCHER is set and this version of tehas has no bash poll: every poll is tehas-dispatch, so take the line out"
done

t_case "TEHAS_DISPATCHER=rust or a blank line stays loadable and reaches no child"
for line in TEHAS_DISPATCHER=rust TEHAS_DISPATCHER=; do
  t_instance "${T_MINIMAL[@]}" "$line"
  t_run "$load; echo rc=\$?; echo \"[\${TEHAS_DISPATCHER-unset}]\""
  t_eq "$t_out" "rc=0
[unset]"
done

t_case "of two retired lines the operator is told about the first of them"
t_instance "${T_MINIMAL[@]}" TEHAS_CLONES=1 TEHAS_WORK_CPUS=4
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "tehas: TEHAS_CLONES is set and this version of tehas has no database clones"
t_eq "$(grep -c '^tehas:' <<<"$t_out")" 1
t_instance "${T_MINIMAL[@]}" TEHAS_CLONES=0 TEHAS_WORK_CPUS=4
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "tehas: TEHAS_WORK_CPUS is set"

t_case "previews without a domain are refused"
t_instance "${T_MINIMAL[@]}" TEHAS_PREVIEWS=1
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "tehas: TEHAS_PREVIEWS is on and TEHAS_DOMAIN is not set"

t_case "previews with a domain load, with the defaults"
t_instance "${T_MINIMAL[@]}" TEHAS_PREVIEWS=1 TEHAS_DOMAIN=example.org
t_run "$load && echo \"\$TEHAS_PREVIEWS \$TEHAS_PREVIEW_MAX \$TEHAS_PREVIEW_MIN_FREE \$TEHAS_PREVIEW_MIN_FREE_MB\""
t_eq "$t_out" "1 5 5g 5120"

# The disk hold of the dispatcher, the same shape of setting as the preview's.
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

# The disk hold of the cleaner (libexec/clean), above the dispatcher's 10g, so
# the cleaner frees space before the poll stops starting cycles.
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

t_case "without a domain there are no previews"
t_instance "${T_MINIMAL[@]}"
t_run "$load && echo \"[\$TEHAS_DOMAIN] \$TEHAS_PREVIEWS\""
t_eq "$t_out" "[] 0"

t_case "an explicit host address is kept"
t_instance "${T_MINIMAL[@]}" TEHAS_HOST_ADDR=192.0.2.7
t_run "$load && echo \"\$TEHAS_HOST_ADDR\""
t_eq "$t_out" "192.0.2.7"

t_case "the worked example takes its private addresses from a file on the host"
t_stub nproc 'echo 12'
# A copy with the renames of api/renames-1.0.tsv: a running factory's instance.
cp -R "$T_ROOT/instances/tehas-tehas" "$T/worked"
while IFS=$'\t' read -r kind old new _; do
  [ "$kind" = setting ] && [ -n "$new" ] || continue
  sed -i "s/^$old=/$new=/" "$T/worked/instance.env"
done <"$T_ROOT/api/renames-1.0.tsv"
TEHAS_INSTANCE_DIR=$T/worked
mkdir -p "$T/home/.config/tehas"
echo TEHAS_GITHUB_APP_ID=1 >"$T/home/.config/tehas/private.env"
t_run "$load && echo \"[\$TEHAS_HOST_ADDR] [\$PI_ENSEMBLE_HOST_ALIASES]\""
t_eq "$t_out" "[127.0.0.1] []"
printf '%s\n' TEHAS_GITHUB_APP_ID=1 "TEHAS_HOST_ADDR=192.0.2.7" "PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.8" >"$T/home/.config/tehas/private.env"
t_run "$load && echo \"[\$TEHAS_HOST_ADDR] [\$PI_ENSEMBLE_HOST_ALIASES]\""
t_eq "$t_out" "[192.0.2.7] [models:192.0.2.8]"
rm -rf "$T/home/.config"
rm -f "$T/bin/nproc"

t_case "loading needs neither git nor the network nor a checkout"
t_instance "${T_MINIMAL[@]}"
mkdir -p "$T/nogit"
for tool in bash awk nproc id head cat tr env; do ln -sf "$(command -v "$tool")" "$T/nogit/$tool"; done
t_out=$(cd "$T" && env -i PATH="$T/nogit" HOME="$T/home" TEHAS_HOME="$T/home/tehas" T_ROOT="$T_ROOT" \
  TEHAS_INSTANCE_DIR="$TEHAS_INSTANCE_DIR" TEHAS_SETTINGS_CMD="$TEHAS_SETTINGS_CMD" bash -c "$load && echo loaded" 2>&1)
t_eq "$t_out" "loaded"
# The console needs neither git nor curl to open a socket, so where the host
# lets a test take the network away, the load runs again with none: a socket
# attempt fails the case. Where it does not, the half says so.
unshare=$(command -v unshare || true)
if [ -z "$unshare" ]; then
  t_skip "no unshare on this host: the load was not asked to do without the network"
elif ! "$unshare" -rn true 2>/dev/null; then
  t_skip "this host refuses unshare -rn: the load was not asked to do without the network"
else
  t_out=$("$unshare" -rn env -i PATH="$T/nogit" HOME="$T/home" TEHAS_HOME="$T/home/tehas" T_ROOT="$T_ROOT" \
    TEHAS_INSTANCE_DIR="$TEHAS_INSTANCE_DIR" TEHAS_SETTINGS_CMD="$TEHAS_SETTINGS_CMD" bash -c "cd '$T' && $load && echo loaded" 2>&1)
  t_eq "$t_out" "loaded"
fi

t_case "loading writes nothing"
# The working directory and TMPDIR are made fresh for the run and compared with
# home and the instance.
mkdir -p "$T/watch/cwd" "$T/watch/tmp"
before=$(find "$T/home" "$T/instance" "$T/watch" | sort)
T_ENV=(${T_ENV[@]+"${T_ENV[@]}"} "TMPDIR=$T/watch/tmp")
t_run "cd '$T/watch/cwd' && $load"
after=$(find "$T/home" "$T/instance" "$T/watch" | sort)
t_eq "$after" "$before"
T_ENV=()

t_case "loading the settings of outsource, gone, writes nothing"
# The warning goes to standard error: the load reaches only its own environment.
mkdir -p "$T/outsource/cwd" "$T/outsource/tmp"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_OUTSOURCE=1 TEHAS_OUTSOURCE_REPOS=acme/widget >"$TEHAS_INSTANCE_DIR/instance.env"
before=$(find "$T/home" "$TEHAS_INSTANCE_DIR" "$T/outsource" | sort)
T_ENV=(${T_ENV[@]+"${T_ENV[@]}"} "TMPDIR=$T/outsource/tmp")
t_run "cd '$T/outsource/cwd' && $load"
after=$(find "$T/home" "$TEHAS_INSTANCE_DIR" "$T/outsource" | sort)
t_eq "$after" "$before"
T_ENV=()

# --- A checkout -------------------------------------------------------------
git init -q -b main "$T/upstream"
git -C "$T/upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m first
git clone -q "$T/upstream" "$T/home/tehas/repos/widget"

t_case "TEHAS_BRANCH_CHECK: unset is none, a name with spaces is kept"
t_instance "${T_MINIMAL[@]}"
t_run "$load && echo \"[\$TEHAS_BRANCH_CHECK]\""
t_eq "$t_out" "[]"
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_BRANCH_CHECK='build / verify (linux)'" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_BRANCH_CHECK]\""
t_eq "$t_out" "[build / verify (linux)]"

t_case "the settings of the old queue order are refused unless blank: tehas order replaced them"
for name in TEHAS_PRIORITY_FIELD TEHAS_LOW_GATE TEHAS_LOW_GATE_WAIT; do
  printf '%s\n' "${T_MINIMAL[@]}" "$name=" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load && echo \"[\${$name-unset}]\""
  t_eq "$t_out" "[unset]"
  printf '%s\n' "${T_MINIMAL[@]}" "$name=7" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load; echo rc=\$?"
  t_contains "$t_out" "rc=78"
  t_contains "$t_out" "$name is no longer read: tehas order sets the order ready issues start in"
done

t_case "TEHAS_ISSUE_TYPES: unset stays unset, kind=Type pairs are kept, another kind stops the load"
t_instance "${T_MINIMAL[@]}"
t_run "$load && echo \"[\${TEHAS_ISSUE_TYPES-unset}]\""
t_eq "$t_out" "[unset]"
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ISSUE_TYPES='improvement=Improvement prototype=Prototype'" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_ISSUE_TYPES]\""
t_eq "$t_out" "[improvement=Improvement prototype=Prototype]"
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_ISSUE_TYPES=bug=Bug >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_ISSUE_TYPES is not valid: bug is no kind"

t_case "the cycle's workspace is under the tehas tree and is not a setting"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_DIR=/somewhere/else
t_run "$load && echo \"\$TEHAS_WORK_DIR\""
t_eq "$t_rc" 0
t_eq "$t_out" "$T/home/tehas/work/widget"

t_case "the planning limits have defaults"
t_instance "${T_MINIMAL[@]}"
t_run '. "$T_ROOT/lib/config.sh"; tehas_load; echo "$TEHAS_PLAN_MAX_TURNS $TEHAS_PLAN_TIMEOUT"'
t_eq "$t_out" "20 $plan_max"

# The command's header and the operator's example are pinned to the config.
t_contains "$(grep 'TEHAS_PLAN_TIMEOUT  seconds' "$T_ROOT/console/src/usage/plan.txt")" "[$plan_max, the ceiling"
t_contains "$(grep 'TEHAS_PLAN_TIMEOUT        seconds' "$T_ROOT/console/src/usage/dispatch.txt")" "[$plan_max]"
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

# install.sh and tehas update read the update lock's wait before tehas_load,
# which unsets every TEHAS_* it does not know, this one among them. Two halves,
# because sourcing instance.env is what would make the knob a setting.
t_case "the environment's update lock wait is used, and the loader drops the name"
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_UPDATE_LOCK_WAIT=3)
t_run '. "$T_ROOT/lib/config.sh"; a=${TEHAS_UPDATE_LOCK_WAIT:-unset}; tehas_load >/dev/null 2>&1; b=${TEHAS_UPDATE_LOCK_WAIT:-unset}; printf "before=%s after=%s\n" "$a" "$b"'
t_eq "$t_out" "before=3 after=unset"
T_ENV=()
# A line in instance.env is sourced only after the two commands read the wait.
t_case "instance.env does not set the update lock's wait"
t_instance "${T_MINIMAL[@]}" TEHAS_UPDATE_LOCK_WAIT=3
t_run '. "$T_ROOT/lib/config.sh"; a=${TEHAS_UPDATE_LOCK_WAIT:-unset}; tehas_load >/dev/null 2>&1; b=${TEHAS_UPDATE_LOCK_WAIT:-}; printf "read=%s instance_line=%s\n" "$a" "$b"'
t_eq "$t_out" "read=unset instance_line=3"

# A stub of nproc makes the default the same on every machine.
t_stub nproc 'echo 8'
t_case "the ticket ceiling defaults to the host's CPUs minus one"
t_instance "${T_MINIMAL[@]}"
t_run "$load && echo \"\$TEHAS_MAX_CYCLES\""
t_eq "$t_rc" 0
t_eq "$t_out" "7"
t_stub nproc 'echo 1'
t_run "$load && echo \"\$TEHAS_MAX_CYCLES\""
t_eq "$t_out" "1"
t_stub nproc 'echo 8'

t_case "workstreams alone are refused where the ticket ceiling's default is above 1, accepted where it is 1"
t_instance "${T_MINIMAL[@]}" TEHAS_WORKSTREAMS=4
t_run "$load"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_WORKSTREAMS (4) and TEHAS_MAX_CYCLES (7) are both above 1"
t_stub nproc 'echo 2'
t_run "$load && echo \"\$TEHAS_WORKSTREAMS \$TEHAS_MAX_CYCLES\""
t_eq "$t_rc" 0
t_eq "$t_out" "4 1"
t_stub nproc 'echo 8'
t_instance "${T_MINIMAL[@]}"

t_case "the review cap leaves room for every cycle and two pull requests by default"
t_run "$load && echo \"\$TEHAS_MAX_AWAITING\""
t_eq "$t_out" "9"

t_case "a ceiling above the host's CPUs is accepted"
t_instance "${T_MINIMAL[@]}" TEHAS_MAX_CYCLES=20
t_run "$load && echo \"\$TEHAS_MAX_CYCLES \$TEHAS_MAX_AWAITING\""
t_eq "$t_rc" 0
t_eq "$t_out" "20 22"

t_case "the review cap's default follows the cycles side by side"
t_instance "${T_MINIMAL[@]}" TEHAS_MAX_CYCLES=4
t_run "$load && echo \"\$TEHAS_MAX_AWAITING\""
t_eq "$t_out" "6"

t_case "a review cap that is set is kept"
t_instance "${T_MINIMAL[@]}" TEHAS_MAX_CYCLES=4 TEHAS_MAX_AWAITING=3
t_run "$load && echo \"\$TEHAS_MAX_AWAITING\""
t_eq "$t_out" "3"

for bad in two 0 -1 1.5 AUTO 99999999999999999999; do
  t_case "a concurrency that is no number is refused, not added to: $bad"
  t_instance "${T_MINIMAL[@]}" TEHAS_MAX_CYCLES=$bad
  t_run "$load"
  t_eq "$t_rc" 78
  t_contains "$t_out" "TEHAS_MAX_CYCLES is not valid"
  # AUTO is another word: only `auto` gets its reason named.
  [ "$bad" = AUTO ] || t_lacks "$t_out" "auto sizing is removed"
done

# `auto` is refused by its own case, with the words that say why.
t_case "a concurrency of auto is refused with the reason"
t_instance "${T_MINIMAL[@]}" TEHAS_MAX_CYCLES=auto
t_run "$load"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_MAX_CYCLES is not valid: expected a whole number: auto sizing is removed"

# Auto sizing is removed: refused for both counts, and the host is not read.
t_case "auto for either count is refused with 78"
rm -f "$T/bin/nproc"
t_instance "${T_MINIMAL[@]}" TEHAS_MAX_CYCLES=auto TEHAS_SLICE_CONTAINERS=auto
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_eq "$(grep -c '^tehas:' <<<"$t_out")" 1
t_contains "$t_out" "auto sizing is removed"
t_eq "$(t_calls nproc)" ""
t_stub nproc 'echo 8'

t_case "a review cap of three is the floor below the host's cycles"
t_instance "${T_MINIMAL[@]}" TEHAS_MAX_CYCLES=1
t_run "$load && echo \"\$TEHAS_MAX_AWAITING\""
t_eq "$t_out" "3"
t_instance "${T_MINIMAL[@]}" TEHAS_MAX_CYCLES=1 TEHAS_MAX_AWAITING=9
t_run "$load && echo \"\$TEHAS_MAX_AWAITING\""
t_eq "$t_out" "9"

# One axis of parallelism holds with numbers too.
t_case "several workstreams with several cycles are refused: one axis of parallelism"
t_instance "${T_MINIMAL[@]}" TEHAS_WORKSTREAMS=2 TEHAS_MAX_CYCLES=4
t_run "$load"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_WORKSTREAMS (2) and TEHAS_MAX_CYCLES (4) are both above 1"

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

# 02g and 1000000000g are sizes to Docker but not to tehas_core::slice.
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

t_case "the energy settings a 1.4 host left in ~/.config/tehas/env load and are not read"
t_instance "${T_MINIMAL[@]}"
mkdir -p "$T/home/.config/tehas" "$T/home/tehas/lib"
cp "$T_ROOT/lib/config.sh" "$T/home/tehas/lib/"
printf '%s\n' TEHAS_ENERGY=viiwork TEHAS_ENERGY_URL=mesh TEHAS_ENERGY_SOURCE_URL=x \
  >"$T/home/.config/tehas/env"
t_run '. "$T_ROOT/host/env.sh"; echo "[${TEHAS_ENERGY:-}] [${TEHAS_ENERGY_URL:-}] [$TEHAS_REPO]"'
t_eq "$t_out" "[] [] [acme/widget]"
rm -rf "$T/home/.config/tehas" "$T/home/tehas/lib"

t_case "without a role model every role is left to the subagent model"
t_instance "${T_MINIMAL[@]}"
t_run "$load && env | grep -E '^PI_ENSEMBLE_(MODEL|PROVIDER)_'"
t_eq "$t_out" ""

t_case "a role model is passed on, with the subagent provider unless it names its own"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_MODEL_CODE_REVIEW_SPECIALIST=small-model \
  PI_ENSEMBLE_MODEL_DEVELOPER=big-model PI_ENSEMBLE_PROVIDER_DEVELOPER=other-provider
t_run "$load && bash -c 'env | grep -E \"^PI_ENSEMBLE_(MODEL|PROVIDER)_\" | LC_ALL=C sort'"
t_eq "$t_rc" 0
t_eq "$t_out" "PI_ENSEMBLE_MODEL_CODE_REVIEW_SPECIALIST=small-model
PI_ENSEMBLE_MODEL_DEVELOPER=big-model
PI_ENSEMBLE_PROVIDER_CODE_REVIEW_SPECIALIST=test-provider
PI_ENSEMBLE_PROVIDER_DEVELOPER=other-provider"

t_case "loading the instance keeps the model keys and refuses one set in instance.env"
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_MODEL_KEY_MAIN=main-secret)
t_run "$load; echo \"\$TEHAS_MODEL_KEY_MAIN\""
t_eq "$t_out" main-secret
t_instance "${T_MINIMAL[@]}" TEHAS_MODEL_KEY_MAIN=in-the-repo
t_run "$load"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_MODEL_KEY_"
t_lacks "$t_out" "in-the-repo"
T_ENV=()

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
T_ENV=(TEHAS_MCP=on TEHAS_MCP_KEYS="abcdefghijklmnopqrstuvwxyz012345 second-key-of-32-characters-long" TEHAS_MCP_DASHBOARD_KEYS="zyxwvutsrqponmlkjihgfedcba012345")
t_run "$load && echo \"\$TEHAS_MCP [\$TEHAS_MCP_KEYS] [\$TEHAS_MCP_DASHBOARD_KEYS]\""
t_eq "$t_out" "1 [abcdefghijklmnopqrstuvwxyz012345 second-key-of-32-characters-long] [zyxwvutsrqponmlkjihgfedcba012345]"

t_case "an MCP key of the wrong shape stops the load, named by its place and never shown"
T_ENV=(TEHAS_MCP_KEYS="abcdefghijklmnopqrstuvwxyz012345 second")
t_run "$load"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_MCP_KEYS is not valid: key 2 is not at least 32 characters"
t_lacks "$t_out" "second"
T_ENV=()

t_case "keys for the MCP server in the instance directory are refused"
t_instance "${T_MINIMAL[@]}" TEHAS_MCP_KEYS=abcdefghijklmnopqrstuvwxyz012345
t_run "$load"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_MCP_KEYS"
t_lacks "$t_out" "abcdefghijklmnopqrstuvwxyz012345"

t_case "dashboard keys for the MCP server in the instance directory are refused too"
t_instance "${T_MINIMAL[@]}" TEHAS_MCP_DASHBOARD_KEYS=abcdefghijklmnopqrstuvwxyz012345
t_run "$load"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_MCP_DASHBOARD_KEYS"
t_lacks "$t_out" "abcdefghijklmnopqrstuvwxyz012345"

# The cleaner's two settings: the hours of age before a thing goes, and the
# directory names that count as build output.
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

# The budget of the idle polls goes to `store poll-idle --budget` as it stands;
# its shape is checked at the load, so a word the store cannot use is not lost in
# a health file, and the order is the store's, so no figure is bound wrongly.
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

# The store's own grammar for a figure (verbs::count), in the store's order: no
# sign, fraction, exponent or ten digits, no name twice, and 0 or a figure that
# begins 1-9, since the store refuses 05 and health would lose the warning.
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

# The two bounds of the measure of an approved body, asked of head -c and head
# -n, so a sign, a fraction or an exponent is no bound. Lines and bytes each hold
# a body the other does not, so both are settings.
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

# The hours go to docker's `until` and find's `-newermt`: a sign, a fraction or
# an exponent is no number of hours, and an empty value takes the default.
for bad in "soon" "0" "-5" "1.5" "24 " "1e3" "123456"; do
  t_case "loading rejects a TEHAS_CLEAN_AFTER of '$bad'"
  printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_CLEAN_AFTER='$bad'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load"
  t_eq "$t_rc" 78
  t_contains "$t_out" "TEHAS_CLEAN_AFTER is not valid: expected a whole number of hours of at least 1"
done

# The names go to find -name: a path, a bare dot, and the family of the two
# dot-names a workspace keeps are refused, since a name close to one of them was
# meant as one, and the cleaner's walk stops at a dot-name it cannot name.
for bad in "." ".." ".git" ".pi" "a/target" "/target" "target;rm -rf" "target *"; do
  t_case "loading rejects a TEHAS_CLEAN_DIRS of '$bad'"
  printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_CLEAN_DIRS='$bad'" >"$TEHAS_INSTANCE_DIR/instance.env"
  t_run "$load"
  t_eq "$t_rc" 78
  t_contains "$t_out" "TEHAS_CLEAN_DIRS is not valid"
done

t_case "a TEHAS_CLEAN_DIRS with .git among other names is refused whatever IFS is"
t_instance "${T_MINIMAL[@]}" "TEHAS_CLEAN_DIRS='target .git'"
t_run "IFS=x; $load; echo rc=\$?"
t_contains "$t_out" "TEHAS_CLEAN_DIRS is not valid: refuses .git: that is not build output"
t_contains "$t_out" "rc=78"
t_instance "${T_MINIMAL[@]}"

# The default the example instance copies by hand is pinned to the config's.
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

# Every command loads the settings, so with only nproc on PATH it still loads.
t_case "loading the settings runs no program but nproc"
t_instance "${T_MINIMAL[@]}" "TEHAS_CYCLE_MIN_FREE=2g" "PI_ENSEMBLE_MODEL_OPS=m"
mkdir -p "$T/only"
printf '#!/bin/bash\necho 4\n' >"$T/only/nproc"
chmod +x "$T/only/nproc"
t_run "PATH=\$T/only; $load; echo \"rc=\$? \$TEHAS_CYCLE_MIN_FREE_MB \$TEHAS_MAX_CYCLES \$PI_ENSEMBLE_IMAGE\""
t_eq "$t_out" "rc=0 2048 3 tehas/pi-rukas:uid$EUID"
t_instance "${T_MINIMAL[@]}"

# --- what the bash load left -----------------------------------------------------
# The two hashes are of the answers of the bash rules the console replaced; the
# rules and their tests are in console/core/src/settings now. A deliberate
# change to a rule or a default changes a hash: take the new one from the
# failure, and say in the pull request what changed.
t_case "every value of every setting gets the verdict and the reason bash gave it"
# shellcheck disable=SC2054  # a comma is part of a budget
values=('' ' ' 0 1 2 6 16 17 99 100 600 601 1440 1441 123456 999999999 1000000000 99999999999999999999 01 auto AUTO on off
  yes git release bash rust 'two words' 'alice  bob' 'alice bob-2' alice- -alice
  'alice[bot]' '[bot]' acme/widget a/b/c /a/b /a/./b /a/../b /a//b /a/ /h/tehas/gate /i/gate /i /w/gate a..b
  5g 5G 5m 6m 512m 6000k 6144k 1048576 1048576b 010g 08m 00 2gb 1.5g -1g 1000000000g 17179869184g
  example.org Example.org -a.org a-.org localhost a.b https://a.example/x.js https://a.example http://a.example/x.js
  https://a.example:80 https://lab.example:80/ 'a"b' "a'b" 'a$b' 'a*b' .git .GIT .pi .pi.x .pix . .. 'target .git' target
  PI_ENSEMBLE_X@1.2 TEHAS_Y@1.2.3.4 'TEHAS_Y@1.2 PI_ENSEMBLE_X@3.4' 'TEHAS_Y@1.2  TEHAS_Z@3.4' TEHAS_Y@1 PATH@1.2
  cpu=5 wall=0,writes=1 cpu=1,writes=2 wall=1,cpu=1 cpu=05 cpu=1000000000 release:0.14 release:0.14.1 branch:a/b branch:
  commit:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa commit:aaa main 'build / verify (linux)' verify\&x=1 $'tab\tbed'
  $'new\nline' $'word\n' é)
names=(PI_ENSEMBLE_MODEL_ PI_ENSEMBLE_PROVIDER_ PI_ENSEMBLE_SUBAGENT_MODEL PI_ENSEMBLE_SUBAGENT_PROVIDER PI_RUKAS_FOLLOW
  TEHAS_ALLOWED_LABELERS TEHAS_ANALYTICS TEHAS_SITE_ANALYTICS_COOKIE TEHAS_SITE_ANALYTICS_NAME TEHAS_SITE_ANALYTICS_SRC
  TEHAS_BRANCH TEHAS_BRANCH_CHECK TEHAS_BUILD_TIMEOUT TEHAS_CHILD_TIMEOUT TEHAS_CLEAN_AFTER TEHAS_CLEAN_DIRS
  TEHAS_CLEAN_MIN_FREE TEHAS_WORKSTREAMS TEHAS_CONTROL_ISSUE TEHAS_CYCLE_MIN_FREE
  TEHAS_CYCLE_RESERVE TEHAS_DESIGN TEHAS_DOMAIN TEHAS_GITHUB_APP_ID TEHAS_HOME
  TEHAS_HOST_ADDR
  TEHAS_SITE_INDEXING TEHAS_INSTANCE_DIR TEHAS_MAX_AWAITING
  TEHAS_MILLWRIGHT_EPICS TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS
  TEHAS_PLAN_MAX_TURNS TEHAS_PLAN_TIMEOUT TEHAS_POLL_BUDGET TEHAS_PREVIEWS TEHAS_PREVIEW_MAX
  TEHAS_PREVIEW_MIN_FREE TEHAS_READ_ONLY TEHAS_REFRESH_MAX_PER_PR
  TEHAS_REFRESH_SLOTS TEHAS_RELEASE_REPO TEHAS_REPO TEHAS_RUKAS_WORKAROUNDS
  TEHAS_SCRATCH_DIR TEHAS_SIZE_MAX_BYTES TEHAS_SIZE_MAX_FILES TEHAS_SIZE_MAX_LINES TEHAS_SLICE_CONTAINERS
  TEHAS_MAX_CYCLES TEHAS_TOP TEHAS_UPDATE_FROM TEHAS_VERIFY_TIMEOUT
  TEHAS_WORK_IMAGES TEHAS_WORK_REPO TEHAS_WORK_STALL TEHAS_WORK_TIMEOUT)
for name in "${names[@]}"; do
  for value in "${values[@]}"; do printf '%s\0%s\0' "$name" "$value"; done
done >"$T/pairs"
TEHAS_HOME=/h/tehas TEHAS_INSTANCE_DIR=/i/ TEHAS_WORK_REPO=/w "$T_CONSOLE" settings valid <"$T/pairs" >"$T/verb"
t_eq "$(sha256sum <"$T/verb" | cut -d' ' -f1)" "4651874a62d22bd2e3a898950972894f68fb8cb606f29aa62f5abca1cffbe26e"

t_stub nproc 'echo 8'
t_case "each kind of instance leaves the environment and the words bash left"
# One instance a line: its lines after T_MINIMAL, separated by |. It is shell, so
# a value may name what the source's environment holds.
: >"$T/loads"
while IFS='|' read -r -u 5 -a lines; do
  t_instance "${T_MINIMAL[@]}" "${lines[@]}"
  T_ENV=(TEHAS_REPO=other/thing TEHAS_SITE_INDEXING=1 PI_ENSEMBLE_DEBUG=1 TEHAS_WORK_CMD=fake EDITOR=ed)
  t_run "$load; echo rc=\$?; export -p"
  t_out=${t_out//$T/@T}
  # Bash 5.3 names the command it was parsing here, and 5.2 does not.
  t_out=$(sed -E "s/(unexpected end of file) from \`if' command on line [0-9]+/\1/" <<<"$t_out")
  # TMPDIR is left out only as t_run gave it, so a load that changed it shows.
  grep -vE '^declare -x (PATH|PWD|OLDPWD|SHLVL|_|T|T_ROOT|TEHAS_SETTINGS_CMD|TEHAS_TEST_STORE_ANSWERS)(=|$)|^declare -x TMPDIR="@T/tmp"$' <<<"${t_out//uid$EUID/uid@}" >>"$T/loads"
done 5<<'INSTANCES'

TEHAS_SITE_INDEXING=1
TEHAS_THEME=
TEHAS_PLAN_TIMEOUT=
TEHAS_CHILD_TIMEOUT=
TEHAS_WORK_DIR=/else
TEHAS_WORK_REPO=/else/repo
TEHAS_HOME=/else/home
TEHAS_INSTANCE_DIR=/else/instance
TEHAS_MAX_CYCLES=4
TEHAS_WORKSTREAMS=4|TEHAS_MAX_CYCLES=1
TEHAS_MAX_AWAITING=3
TEHAS_DOMAIN=example.org
TEHAS_VERIFY_TIMEOUT=600
PI_ENSEMBLE_VERIFY_TIMEOUT_MS=1
PI_ENSEMBLE_MAX_WORKSTREAMS=4
PI_ENSEMBLE_NO_SSH=0
PI_ENSEMBLE_IMAGE=mine
PI_ENSEMBLE_HOST_ALIASES=a:192.0.2.1
PI_RUKAS_FOLLOW=release
PI_RUKAS_FOLLOW=
PI_ENSEMBLE_MODEL_OPS=m
PI_ENSEMBLE_MODEL_OPS=m|PI_ENSEMBLE_PROVIDER_OPS=p
PI_ENSEMBLE_MODEL_OPS=
PI_ENSEMBLE_PROVIDER_OPS=p
PI_ENSEMBLE_MODEL_NOPE=m
PI_ENSEMBLE_MODEL_=m
PI_ENSEMBLE_MODEL_ZZ=a|PI_ENSEMBLE_MODEL_AA=b
PI_ENSEMBLE_PROVIDER_ZZ=a|PI_ENSEMBLE_MODEL_ZZ=b
TEHAS_FIXER_LOGIN=x
TEHAS_CLONES=0
TEHAS_CLONES=
TEHAS_CLONES=1
TEHAS_DISPATCHER=rust
TEHAS_DISPATCHER=bash
TEHAS_WORK_CPUS=3
TEHAS_WORK_MEMORY=8g
TEHAS_CLONES=1|TEHAS_WORK_CPUS=4
TEHAS_THEME=neon
TEHAS_THEME=neon|TEHAS_TOP=9
TEHAS_DESIGN=pixel
TEHAS_DESIGN=paper
TEHAS_TOP=9|TEHAS_BRANCH='a b'
TEHAS_CYCLE_MIN_FREE=010g
TEHAS_CYCLE_MIN_FREE=08m
TEHAS_CYCLE_MIN_FREE=0
TEHAS_CYCLE_RESERVE=5m
TEHAS_RUKAS_WORKAROUNDS=TEHAS_X@1.2
TEHAS_TOP=1|TEHAS_RUKAS_WORKAROUNDS=TEHAS_TOP@1.2
TEHAS_RUKAS_WORKAROUNDS=TEHAS_RUKAS_WORKAROUNDS@1.2
TEHAS_GITHUB_APP_ID=7
TEHAS_SITE_ANALYTICS_SRC=https://a.example/x.js
TEHAS_REVIEW=on|TEHAS_REVIEW_REPO=Acme/Widget
TEHAS_OUTSOURCE=off|TEHAS_OUTSOURCE_REPOS=|TEHAS_OUTSOURCE_TIMEOUT=600
TEHAS_WORK_DESCRIBE=1
TEHAS_WORK_COMMENTS=1
TEHAS_WORK_ZIPS=1
TEHAS_WORKSTREAMS=2|TEHAS_MAX_CYCLES=3
TEHAS_WORK_STALL=1800
TEHAS_WORK_TIMEOUT=3600|TEHAS_CHILD_TIMEOUT=3601
TEHAS_WORK_TIMEOUT=25201
TEHAS_PLAN_TIMEOUT=7201
TEHAS_PREVIEWS=1
TEHAS_MAX_CYCLES=two
TEHAS_VERIFY_TIMEOUT=soon
TEHAS_UPDATE_LOCK_WAIT=3
TEHAS_MODEL_KEY_X=sekret
export TEHAS_MODEL_KEY_X=sekret
TEHAS_MCP_KEYS=sekret
TEHAS_MCP_DASHBOARD_KEYS=sekret
EDITOR=vi
unset -v EDITOR
FOO='a b'
false
return 1
if
'unterminated
X=$'a\nb'
T_INSIDE=$TEHAS_INSTANCE_DIR/$HOME
INSTANCES
T_ENV=()
t_instance "${T_MINIMAL[@]}"
t_eq "$(sha256sum <"$T/loads" | cut -d' ' -f1)" "f291ccd9898ff38f621a6750a4d9ce65adfb081736a853e10e4f35f09d0e6d1d"
t_contains "$(cat "$T/loads")" "rc=78"

t_case "no instance.env keeps the environment as it was, as bash did"
rm -f "$TEHAS_INSTANCE_DIR/instance.env"
T_ENV=(TEHAS_REPO=other/thing)
t_run "$load; echo rc=\$? \$TEHAS_REPO"
T_ENV=()
t_contains "$t_out" "rc=78 other/thing"
t_instance "${T_MINIMAL[@]}"

t_case "a pointer's blanks are not part of the directory, and an empty pointer is no pointer"
mkdir -p "$T/home/.config/tehas"
echo "  $T/instance  " >"$T/home/.config/tehas/instance-dir"
saved=$TEHAS_INSTANCE_DIR
unset TEHAS_INSTANCE_DIR
t_run "$load && echo \"\$TEHAS_INSTANCE_DIR\""
t_eq "$t_out" "$T/instance"
: >"$T/home/.config/tehas/instance-dir"
t_run "$load; echo rc=\$?"
t_contains "$t_out" "no instance.env in $T/home/.config/tehas"
export TEHAS_INSTANCE_DIR=$saved
rm -rf "$T/home/.config"

t_case "a host's nproc that fails or says nothing leaves one cycle"
for body in 'exit 3' 'echo'; do
  t_stub nproc "$body"
  t_run "$load && echo \"\$TEHAS_MAX_CYCLES\""
  t_eq "$t_out" "1"
done
t_stub nproc 'echo 8'

t_case "a size arithmetic cannot read stops the load, where bash fell through with no word"
t_instance "${T_MINIMAL[@]}" TEHAS_CYCLE_MIN_FREE=09g
t_run "$load; echo rc=\$?"
t_contains "$t_out" "tehas: TEHAS_CYCLE_MIN_FREE is not valid: expected a size such as 512m or 5g (see $T/instance/instance.env)"
t_contains "$t_out" "rc=78"
t_instance "${T_MINIMAL[@]}"

t_case "what instance.env prints is not read as settings"
t_instance "${T_MINIMAL[@]}" "echo 'export TEHAS_REPO=evil/repo'" "TEHAS_TOP=1"
t_run "$load; echo rc=\$? \$TEHAS_TOP \$TEHAS_REPO"
t_contains "$t_out" "rc=0 1 acme/widget"
t_contains "$t_out" "export TEHAS_REPO=evil/repo"

t_case "an exit in instance.env is a refusal, and ends no shell"
t_instance "${T_MINIMAL[@]}" "exit 0"
t_run "$load; echo rc=\$? [\${TEHAS_REPO:-}]"
t_contains "$t_out" "could not be read as shell"
t_contains "$t_out" "rc=78 []"

t_case "instance.env moves no shell: a cd stays in the shell that read it"
t_instance "${T_MINIMAL[@]}" "cd /"
t_run "cd \"\$T\" && $load; echo rc=\$? \"\$PWD\""
t_eq "$t_out" "rc=0 $T"

t_case "instance.env sees the environment, and the name of a host's setting is gone from it"
t_instance "${T_MINIMAL[@]}" 'TEHAS_ASSIGNEE=${TEHAS_TOP:-seen-${HOME##*/}}'
T_ENV=(TEHAS_TOP=1)
t_run "$load && echo \"\$TEHAS_ASSIGNEE\""
T_ENV=()
t_eq "$t_out" "seen-home"
t_instance "${T_MINIMAL[@]}"

t_case "the shell that reads instance.env reads no stdin and runs no file BASH_ENV names"
t_instance "${T_MINIMAL[@]}" TEHAS_MAX_CYCLES=1 'read -r TEHAS_BRANCH || TEHAS_BRANCH=none'
printf 'echo ran >>"%s/bash-env"\n' "$T" >"$T/bash-env.sh"
t_run "export BASH_ENV=\$T/bash-env.sh; echo stdin | { $load; echo rc=\$? \"\$TEHAS_BRANCH\" \"\$BASH_ENV\"; }"
t_contains "$t_out" "rc=0 none $T/bash-env.sh"
[ ! -e "$T/bash-env" ] || t_bad "the file BASH_ENV names ran"
t_instance "${T_MINIMAL[@]}"

t_case "what a load does not refuse tehas_require_instance_settings does, in the words it always had"
for pair in "TEHAS_REPO|widget|expected owner/name" "TEHAS_ALLOWED_LABELERS|'a  b'|expected GitHub logins separated by single spaces"; do
  t_instance "${T_MINIMAL[@]}" "${pair%%|*}=$(cut -d'|' -f2 <<<"$pair")"
  t_run "$load || exit 9; echo loaded; tehas_require_instance_settings; echo not-reached"
  t_eq "$t_rc" 78
  t_contains "$t_out" "loaded"
  t_contains "$t_out" "tehas: ${pair%%|*} is not valid: ${pair##*|} (see $T/instance/instance.env)"
  t_lacks "$t_out" "not-reached"
done

t_case "tehas_require_instance_settings asks the console: no load before it, and the instance's rules still hold"
t_instance "${T_MINIMAL[@]}" TEHAS_REPO=widget
t_run ". \"\$T_ROOT/lib/config.sh\"; TEHAS_HOME=\$T/home/tehas tehas_require_instance_settings; echo not-reached"
t_eq "$t_rc" 78
t_contains "$t_out" "tehas: TEHAS_REPO is not valid: expected owner/name"
t_lacks "$t_out" "not-reached"
t_instance "${T_MINIMAL[@]}"
t_run ". \"\$T_ROOT/lib/config.sh\"; TEHAS_HOME=\$T/home/tehas tehas_require_instance_settings && echo fine"
t_eq "$t_out" "fine"

t_case "a shell's own PI_RUKAS_FOLLOW, as versions.env leaves it unexported, is checked by a load"
t_instance "${T_MINIMAL[@]}"
t_run "PI_RUKAS_FOLLOW=main; $load; echo rc=\$?"
t_contains "$t_out" "PI_RUKAS_FOLLOW is not valid"
t_contains "$t_out" "rc=78"

t_case "the host's hook stays from the environment, and an instance line of it is not what a load uses"
t_run "$load && echo \"\$TEHAS_SETTINGS_CMD\""
t_eq "$t_out" "$TEHAS_SETTINGS_CMD"

t_case "with no console a load is one line and 78, the shell goes on, and nothing is read"
T_ENV=(TEHAS_SETTINGS_CMD="$T/no-such-console")
t_run "$load; echo rc=\$? [\${TEHAS_REPO:-}]"
T_ENV=()
t_eq "$t_out" "tehas: no console at $T/no-such-console to load the settings: run install.sh
rc=78 []"

t_case "a console beside lib/config.sh is the one a load runs, and then the home's"
mkdir -p "$T/tree/lib" "$T/tree/libexec" "$T/home/tehas/libexec"
cp "$T_ROOT/lib/config.sh" "$T/tree/lib/"
for at in "$T/tree/libexec/console" "$T/home/tehas/libexec/console"; do
  printf '#!%s\necho "echo at %s"\n' "$T_BASH" "$at" >"$at"
  chmod +x "$at"
done
t_run "unset TEHAS_SETTINGS_CMD; . \"\$T/tree/lib/config.sh\"; tehas_load"
t_eq "$t_out" "at $T/tree/libexec/console"
rm -f "$T/tree/libexec/console"
t_run "unset TEHAS_SETTINGS_CMD; . \"\$T/tree/lib/config.sh\"; tehas_load"
t_eq "$t_out" "at $T/home/tehas/libexec/console"
rm -f "$T/home/tehas/libexec/console"
t_run "unset TEHAS_SETTINGS_CMD; . \"\$T/tree/lib/config.sh\"; tehas_load; echo rc=\$?"
t_contains "$t_out" "no console at $T/home/tehas/libexec/console"
t_contains "$t_out" "rc=78"

t_case "a console that cannot run or does not know the verb is 78, its words are not run, and the shell goes on"
printf 'not a program\0\0\0' >"$T/garbage"
chmod +x "$T/garbage"
printf '#!/nonexistent/interpreter\n' >"$T/no-interpreter"
chmod +x "$T/no-interpreter"
printf '#!%s\necho "touch %s/ran"\nexit 2\n' "$T_BASH" "$T" >"$T/older-console"
chmod +x "$T/older-console"
for console in "$T/garbage" "$T/no-interpreter" "$T/older-console" /; do
  T_ENV=(TEHAS_SETTINGS_CMD="$console")
  t_run "$load; echo rc=\$? [\${TEHAS_REPO:-}]"
  t_contains "$t_out" "rc=78 []"
  t_contains "$t_out" "tehas: the console at $console cannot load the settings"
done
T_ENV=()
[ ! -e "$T/ran" ] || t_bad "what a console that failed printed was run"

t_case "a login shell with no console stays usable and does not hang"
t_instance "${T_MINIMAL[@]}"
mkdir -p "$T/home/tehas/lib"
cp "$T_ROOT/lib/config.sh" "$T/home/tehas/lib/"
T_ENV=(TEHAS_SETTINGS_CMD=)
t_run 'timeout 20 bash -c ". \"\$T_ROOT/host/env.sh\"; echo shell-is-here \$?"'
T_ENV=()
t_contains "$t_out" "tehas: no console at $T/home/tehas/libexec/console to load the settings: run install.sh"
t_contains "$t_out" "shell-is-here 0"
rm -rf "$T/home/tehas/lib"
rm -f "$T/bin/nproc"

t_done
