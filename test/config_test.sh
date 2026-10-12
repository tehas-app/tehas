#!/usr/bin/env bash
# Tests for lib/config.sh: how instance settings are found, loaded, defaulted
# and validated.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

load='. "$T_ROOT/lib/config.sh"; tehas_load'

t_case "a minimal instance loads and derives the checkout path"
t_instance "${T_MINIMAL[@]}"
t_run "$load && echo \"\$TEHAS_WORK_REPO|\$TEHAS_HOST_ADDR|\$TEHAS_CONCURRENCY\""
t_eq "$t_rc" 0
t_eq "$t_out" "$T/home/tehas/repos/widget|127.0.0.1|4"

t_case "settings are exported to child processes"
t_run "$load && bash -c 'echo \$TEHAS_REPO \$PI_ENSEMBLE_SUBAGENT_MODEL \$PI_ENSEMBLE_MAX_WORKSTREAMS'"
t_eq "$t_out" "acme/widget test-model 4"

t_case "an instance can lower the workstreams, and pi-rukas is told the same number"
t_instance "${T_MINIMAL[@]}" TEHAS_CONCURRENCY=1
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

t_case "the test hooks survive from the environment"
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_WORK_CMD=fake-work TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index
  TEHAS_CYCLE_START_CMD=fake-start TEHAS_CYCLE_START_WAIT=2)
t_run "$load && echo \"\$TEHAS_WORK_CMD \$TEHAS_STATUS_CMD \$TEHAS_INDEX_CMD \$TEHAS_CYCLE_START_CMD \$TEHAS_CYCLE_START_WAIT\""
t_eq "$t_out" "fake-work fake-status fake-index fake-start 2"
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

for bad in "TEHAS_CONCURRENCY=0" "TEHAS_TICKET_CONCURRENCY=0" "TEHAS_TICKET_CONCURRENCY=two" "TEHAS_MAX_AWAITING=-1" "TEHAS_WORK_TIMEOUT=soon" "TEHAS_HOST_ADDR=not an address" \
  "TEHAS_BRANCH=two words" "TEHAS_INDEXING=yes" "TEHAS_ANALYTICS_SRC=http://plain.example/ea.js" \
  "TEHAS_ANALYTICS_SRC=https://a.example/x.js\" onload=\"x" "TEHAS_ANALYTICS_SITE=two words" \
  "TEHAS_ANALYTICS_COOKIE=yes" "TEHAS_PROTOLAB_URL=https://lab.example/" "TEHAS_PROTOLAB_URL=lab.example" \
  "TEHAS_PLAN_MAX_TURNS=0" "TEHAS_PLAN_TIMEOUT=1h" \
  "TEHAS_DOMAIN=https://example.org" "TEHAS_DOMAIN=example.org/" "TEHAS_DOMAIN=Example.org" \
  "TEHAS_DOMAIN=localhost" "TEHAS_DOMAIN=-a.example.org" "TEHAS_PREVIEWS=yes" \
  "TEHAS_PREVIEW_MAX=0" "TEHAS_PREVIEW_MAX=100" "TEHAS_PREVIEW_MIN_FREE=lots" \
  "TEHAS_IMPROVE_REPO=not a repo" "TEHAS_IMPROVE_REPO=acme" "TEHAS_IMPROVE_REPO=acme/factory/more" \
  "TEHAS_IMPROVE_RUKAS_REPO=not a repo" "TEHAS_IMPROVE_RUKAS_REPO=acme"; do
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

t_case "analytics is off unless the instance names a script and a site"
printf '%s\n' "${T_MINIMAL[@]}" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_ANALYTICS_SRC][\$TEHAS_ANALYTICS_SITE][\$TEHAS_ANALYTICS_COOKIE]\""
t_eq "$t_out" "[][][0]"

t_case "analytics settings are loaded as written"
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ANALYTICS_SRC=https://stats.example/ea.js" "TEHAS_ANALYTICS_SITE=widget" \
  "TEHAS_ANALYTICS_COOKIE=1" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load && echo \"[\$TEHAS_ANALYTICS_SRC][\$TEHAS_ANALYTICS_SITE][\$TEHAS_ANALYTICS_COOKIE]\""
t_eq "$t_out" "[https://stats.example/ea.js][widget][1]"

t_case "an analytics script without a site is refused"
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ANALYTICS_SRC=https://stats.example/ea.js" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_ANALYTICS_SITE is not set"

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
t_eq "$t_out" "20 3600"

t_case "the planning limits can be set in the instance"
t_instance "${T_MINIMAL[@]}" TEHAS_PLAN_MAX_TURNS=5 TEHAS_PLAN_TIMEOUT=600
t_run '. "$T_ROOT/lib/config.sh"; tehas_load; echo "$TEHAS_PLAN_MAX_TURNS $TEHAS_PLAN_TIMEOUT"'
t_eq "$t_out" "5 600"

t_case "the planning test hook is kept from the environment, a setting of that name is not"
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_PLAN_CMD=fake-plan TEHAS_PLAN_MAX_TURNS=99)
t_run '. "$T_ROOT/lib/config.sh"; tehas_load; echo "$TEHAS_PLAN_CMD $TEHAS_PLAN_MAX_TURNS"'
t_eq "$t_out" "fake-plan 20"
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

t_done
