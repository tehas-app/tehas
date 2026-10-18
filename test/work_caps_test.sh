#!/usr/bin/env bash
# Tests for libexec/work: what the image says it supports, and what a cycle
# is refused or given because of it; the push check and Pi's settings.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/work_lib.sh
. "$(dirname "$0")/work_lib.sh" || exit 1

# --- What the image supports -----------------------------------------------------
# caps_refused: 78, the image asked, and no workspace made, nothing fetched and
# no cycle started.
caps_refused() {
  t_eq "$t_rc" 78
  t_eq "$(wc -l <"$T/calls/docker.caps")" 1
  t_eq "$(cycle_args)" ""
  t_eq "$(find "$ws" -maxdepth 0 2>/dev/null | wc -l)" 0
  t_eq "$(t_calls gh)" ""
}
caps_fresh() { rm -rf "$T"/calls/* "$ws" "$ws.state" "$ws.before-retry"; }

t_case "the image is asked what it supports, by the runner's path, before the workspace is made"
caps_fresh
t_run "$work 7"
t_eq "$(cat "$T/calls/docker.caps")" "run --rm --init tehas/pi-rukas:uid$(id -u) $runner --capabilities"
caps_at=$(t_calls docker | grep -n -e '--capabilities' | cut -d: -f1 | head -n 1)
push_at=$(t_calls docker | grep -n -e 'push --dry-run' | cut -d: -f1 | head -n 1)
t_eq "$([ -n "$caps_at" ] && [ -n "$push_at" ] && [ "$caps_at" -lt "$push_at" ] && echo before)" before

t_case "an input the container would be given that the image does not list stops the cycle: 78, named"
caps_fresh
jq '.inputs |= map(select(.name != "PI_ENSEMBLE_SUBAGENT_MODEL"))' "$T/caps.json" >"$T/caps.new" && mv "$T/caps.new" "$T/caps.json"
t_run "$work 7"
caps_refused
t_contains "$t_out" "does not support what this cycle is given, not starting a cycle"
t_contains "$t_out" "the input PI_ENSEMBLE_SUBAGENT_MODEL"
caps_good

t_case "an instance line for an input the image does not read stops the cycle: 78, named"
caps_fresh
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_NOT_A_LEVER=1
t_run "$work 7"
caps_refused
t_contains "$t_out" "the input PI_ENSEMBLE_NOT_A_LEVER"

t_case "an input pi-rukas sets on its own children is not one a host may set"
caps_fresh
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_SUBAGENT_MODE=1
t_run "$work 7"
caps_refused
t_contains "$t_out" "the input PI_ENSEMBLE_SUBAGENT_MODE"

t_case "a role's model is one of the inputs the image lists as a family"
caps_fresh
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_MODEL_DEVELOPER=big
t_run "$work 7"
t_eq "$t_rc" 2
t_lacks "$t_out" "does not support"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9

t_case "a role the image does not list as a member of its family stops the cycle: 78, named"
caps_fresh
jq '(.inputs[] | select(.name == "PI_ENSEMBLE_MODEL_<ROLE>" or .name == "PI_ENSEMBLE_PROVIDER_<ROLE>")) += {members: ["OPS"]}' \
  "$T/caps.json" >"$T/caps.new" && mv "$T/caps.new" "$T/caps.json"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_MODEL_DEVELOPER=big
t_run "$work 7"
t_eq "$t_rc" 78
t_contains "$t_out" "the input PI_ENSEMBLE_MODEL_DEVELOPER"
t_contains "$t_out" "the input PI_ENSEMBLE_PROVIDER_DEVELOPER"
t_eq "$(ls "$ws" 2>/dev/null)" ""
caps_fresh
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_MODEL_OPS=big
t_run "$work 7"
t_eq "$t_rc" 2
t_lacks "$t_out" "does not support"
caps_good
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9

t_case "the switches of pi-rukas's own docker wrapper are not given to the container"
t_lacks "$(after -e)" "PI_ENSEMBLE_NO_DOCKER_SOCKET"
t_lacks "$(after -e)" "PI_ENSEMBLE_NO_SSH"

for change in '.contracts.exitCodes.version = 2' '.contracts.resultSchema.version = 3' \
  '.contracts.ndjson.stable.version = 2' 'del(.contracts)'; do
  t_case "an image of other contract versions stops the cycle: $change"
  caps_fresh
  jq "$change" "$T/caps.json" >"$T/caps.new" && mv "$T/caps.new" "$T/caps.json"
  t_run "$work 7"
  caps_refused
  t_contains "$t_out" "version"
  caps_good
done

t_case "a flag the runner does not take stops the cycle, and only when it is passed"
caps_fresh
jq '.verbs.work.flags = ["--json", "--timeout"]' "$T/caps.json" >"$T/caps.new" && mv "$T/caps.new" "$T/caps.json"
t_run "$work 7 --restart"
caps_refused
t_contains "$t_out" "the flag --restart"
caps_fresh
t_run "$work 7"
t_eq "$t_rc" 2
caps_good

# A refresh's record, as the dispatcher writes it, or another start's.
refresh_record() { # <how>
  mkdir -p "$T/home/tehas/state/cycles"
  jq -n --arg how "$1" '{at: "2026-10-01T00:00:00Z", by: "alice", how: $how, body_sha256: ("0" * 64), links: []}' \
    >"$T/home/tehas/state/cycles/7.approved.json"
}

t_case "a refresh passes --continue-pr when the image takes it, with the same mounts and variables as any start"
caps_fresh
caps_continue
t_run "$work 7"
plain_v=$(after -v | LC_ALL=C sort)
plain_e=$(after -e | sed 's/=.*//' | LC_ALL=C sort)
t_lacks "$(cycle_args | tr '\n' ' ')" "--continue-pr"
caps_fresh
refresh_record refresh
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(cycle_args | sed -n "/^$(printf '%s' "tehas/pi-rukas:uid$(id -u)" | sed 's/[\/.]/\\&/g')\$/,\$p" | tr '\n' ' ')" \
  "tehas/pi-rukas:uid$(id -u) $runner work 7 --timeout 360 --json --continue-pr "
t_eq "$(after -v | LC_ALL=C sort)" "$plain_v"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$plain_e"
rm -f "$T/home/tehas/state/cycles/7.approved.json"
caps_good

t_case "a refresh on an image that does not take --continue-pr starts nothing"
caps_fresh
refresh_record refresh
t_run "$work 7"
caps_refused
t_contains "$t_out" "the flag --continue-pr"
rm -f "$T/home/tehas/state/cycles/7.approved.json"

for how in label retry-tick outage-retry fixer; do
  t_case "a start of how $how never passes --continue-pr, also when the image takes it"
  caps_fresh
  caps_continue
  refresh_record "$how"
  t_run "$work 7"
  t_eq "$t_rc" 2
  t_lacks "$(cycle_args | tr '\n' ' ')" "--continue-pr"
  rm -f "$T/home/tehas/state/cycles/7.approved.json"
  caps_good
done

t_case "an image without the host handoff channel stops the cycle, and one without a restart that clears the label stops a restart"
caps_fresh
jq '.features.handoffChannel = false' "$T/caps.json" >"$T/caps.new" && mv "$T/caps.new" "$T/caps.json"
t_run "$work 7"
caps_refused
t_contains "$t_out" "the feature handoffChannel"
caps_good
caps_fresh
jq 'del(.features.restartClearsAttention)' "$T/caps.json" >"$T/caps.new" && mv "$T/caps.new" "$T/caps.json"
t_run "$work 7 --restart"
caps_refused
t_contains "$t_out" "the feature restartClearsAttention"
caps_fresh
t_run "$work 7"
t_eq "$t_rc" 2
caps_good

t_case "the handoff channel, the deadline and the gate runs at once are this command's, not the instance's"
caps_fresh
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9 PI_ENSEMBLE_HANDOFF_CHANNEL=forge \
  PI_ENSEMBLE_CYCLE_BUDGET_MS=60000 PI_ENSEMBLE_VERIFY_CONCURRENCY=9 PI_ENSEMBLE_CYCLE_DEADLINE=1
before=$(date +%s)
t_run "$work 7"
t_eq "$(after -e | grep -c '^PI_ENSEMBLE_HANDOFF_CHANNEL')" 1
t_contains "$(after -e)" "PI_ENSEMBLE_HANDOFF_CHANNEL=host"
t_lacks "$(after -e)" "PI_ENSEMBLE_CYCLE_BUDGET_MS"
# A container of a 2048m ceiling runs one gate at a time.
t_eq "$(after -e | grep '^PI_ENSEMBLE_VERIFY_CONCURRENCY')" "PI_ENSEMBLE_VERIFY_CONCURRENCY=1"
# Six hours less a quarter of them, in milliseconds since the epoch.
deadline=$(after -e | sed -n 's/^PI_ENSEMBLE_CYCLE_DEADLINE=//p')
t_eq "$(after -e | grep -c '^PI_ENSEMBLE_CYCLE_DEADLINE')" 1
t_eq "$([ "$deadline" -ge $(((before + 16200) * 1000)) ] && [ "$deadline" -le $(((before + 16200 + 60) * 1000)) ] && echo in-range)" in-range
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9

t_case "the margin before the time limit holds one run of the gate and the handoff"
caps_fresh
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9 TEHAS_WORK_TIMEOUT=7200 TEHAS_VERIFY_TIMEOUT=2400
before=$(date +%s)
t_run "$work 7"
deadline=$(after -e | sed -n 's/^PI_ENSEMBLE_CYCLE_DEADLINE=//p')
# 7200 - (2400 + 480)
t_eq "$([ "$deadline" -ge $(((before + 4320) * 1000)) ] && [ "$deadline" -le $(((before + 4320 + 60) * 1000)) ] && echo in-range)" in-range

t_case "a time limit with no room for a deadline gives none, says so, and does not need the feature"
caps_fresh
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9 TEHAS_WORK_TIMEOUT=600 TEHAS_VERIFY_TIMEOUT=120
jq 'del(.features.cycleDeadline)' "$T/caps.json" >"$T/caps.new" && mv "$T/caps.new" "$T/caps.json"
t_run "$work 7"
t_lacks "$t_out" "does not support"
t_contains "$t_out" "the time limit of 600s leaves no room for a deadline before it, the cycle has none"
t_lacks "$(after -e)" "PI_ENSEMBLE_CYCLE_DEADLINE"
caps_good
caps_fresh
jq 'del(.features.cycleDeadline)' "$T/caps.json" >"$T/caps.new" && mv "$T/caps.new" "$T/caps.json"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
t_run "$work 7"
caps_refused
t_contains "$t_out" "the feature cycleDeadline"
caps_good

t_case "a workaround whose release the image has reached stops the cycle: 78, named"
caps_fresh
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9 PI_ENSEMBLE_COMMENT_RETENTION=0 \
  TEHAS_RUKAS_WORKAROUNDS=PI_ENSEMBLE_COMMENT_RETENTION@0.12.80
t_run "$work 7"
caps_refused
t_contains "$t_out" "TEHAS_RUKAS_WORKAROUNDS names a workaround this image no longer needs"
t_contains "$t_out" "PI_ENSEMBLE_COMMENT_RETENTION works around a pi-rukas defect fixed in 0.12.80, and the image's pi-rukas is 0.12.80"
t_contains "$t_out" "Remove the setting and its entry from"
caps_fresh
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9 PI_ENSEMBLE_COMMENT_RETENTION=0 \
  TEHAS_RUKAS_WORKAROUNDS=PI_ENSEMBLE_COMMENT_RETENTION@0.12.81
t_run "$work 7"
t_eq "$t_rc" 2
t_lacks "$t_out" "no longer needs"
caps_fresh
jq '.version = "main"' "$T/caps.json" >"$T/caps.new" && mv "$T/caps.new" "$T/caps.json"
t_run "$work 7"
caps_refused
t_contains "$t_out" "the image's pi-rukas version could not be read"
caps_good
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9

t_case "with images on, the image must read the images directory's variable"
caps_fresh
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=1
jq '.inputs |= map(select(.name != "PI_ENSEMBLE_EXTRA_IMAGE_DIRS"))' "$T/caps.json" >"$T/caps.new" && mv "$T/caps.new" "$T/caps.json"
t_run "$work 7"
caps_refused
t_contains "$t_out" "the input PI_ENSEMBLE_EXTRA_IMAGE_DIRS"
caps_good
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9

t_case "an image that gives no answer, or not JSON, stops the cycle"
caps_fresh
T_ENV=("${env_ok[@]}" CAPS_RC=1)
t_run "$work 7"
caps_refused
t_contains "$t_out" "gave no --capabilities answer (exit 1)"
T_ENV=("${env_ok[@]}")
caps_fresh
echo 'error: Module not found' >"$T/caps.json"
t_run "$work 7"
caps_refused
t_contains "$t_out" "gave no --capabilities answer"
caps_good

t_case "names in the answer are not run as patterns beyond their own characters"
caps_fresh
jq '.inputs += [{name: "PI_ENSEMBLE_.*", scope: "cycle"}] | .inputs |= map(select(.name != "PI_ENSEMBLE_SUBAGENT_MODEL"))' \
  "$T/caps.json" >"$T/caps.new" && mv "$T/caps.new" "$T/caps.json"
t_run "$work 7"
caps_refused
t_contains "$t_out" "the input PI_ENSEMBLE_SUBAGENT_MODEL"
caps_good
caps_fresh
t_run "$work 7"

t_case "the push check's container gets the workspace and the same variables, no more"
t_eq "$(preflight_args | unknown_options "$image" "--user -e -w -v --entrypoint" "--rm")" ""
t_eq "$(preflight_args | awk 'prev == "-v" { print } { prev = $0 }')" "$ws:$inside"
t_eq "$(preflight_args | awk 'prev == "-e" { print } { prev = $0 }' | sed 's/=.*//' | LC_ALL=C sort)" \
  "$(printf '%s\n' "$want" HOME | LC_ALL=C sort)"
t_eq "$(preflight_args | awk 'prev == "--user" { print } { prev = $0 }')" "$(id -u):$(id -g)"

t_case "the cycle's log names the provider retry its Pi runs with"
line="provider retry: timeoutMs=600000 maxRetries=8 maxRetryDelayMs=60000 httpIdleTimeoutMs=unset agentMaxRetries=10 agentBaseDelayMs=5000 agentMaxDelayMs=120000"
t_contains "$t_out" "tehas work: $line"
cycle_log=$(printf '%s\n' "$t_out" | sed -n 's/^tehas work: issue 7 started, log //p')
t_eq "$(tail -n 1 "$cycle_log.preflight")" "$line"

t_case "the product's own .pi/settings.json is merged over the host's, as Pi merges them"
mkdir -p "$T/prod/.pi"
echo '{"retry":{"provider":{"maxRetries":2}},"httpIdleTimeoutMs":120000}' >"$T/prod/.pi/settings.json"
t_run ". '$T_ROOT/lib/config.sh'; . '$T_ROOT/lib/sandbox.sh'; tehas_sandbox_retry '$T/prod'"
t_eq "$t_out" "provider retry: timeoutMs=600000 maxRetries=2 maxRetryDelayMs=60000 httpIdleTimeoutMs=120000 agentMaxRetries=10 agentBaseDelayMs=5000 agentMaxDelayMs=120000"
echo '{"retry":' >"$T/prod/.pi/settings.json"
t_run ". '$T_ROOT/lib/config.sh'; . '$T_ROOT/lib/sandbox.sh'; tehas_sandbox_retry '$T/prod'"
t_contains "$t_out" "provider retry: unknown"
t_contains "$t_out" "is not JSON"
mv "$T/home/.pi/agent/settings.json" "$T/settings.kept"
t_run ". '$T_ROOT/lib/config.sh'; . '$T_ROOT/lib/sandbox.sh'; tehas_sandbox_retry '$T/prod'"
t_eq "$t_out" "provider retry: unknown, $T/home/.pi/agent/settings.json is missing"
mv "$T/settings.kept" "$T/home/.pi/agent/settings.json"
rm -rf "$T/prod"

t_case "the generic and example settings let an agent wait out a busy model, not give up after three tries"
for f in host/pi/settings.json instances/example/settings.json; do
  t_eq "$(jq -c '.retry | [.maxRetries, .baseDelayMs, .maxAgentDelayMs]' "$T_ROOT/$f")" "[10,5000,120000]"
done

t_case "without Pi's settings.json nothing runs: no docker call, the workspace and the kept one as they were"
mv "$T/home/.pi/agent/settings.json" "$T/settings.kept"
mkdir -p "$ws.before-retry"
echo kept >"$ws.before-retry/marker"
echo current >"$ws/marker"
snapshot() { find "$ws" "$ws.before-retry" -printf '%P %s %T@\n' 2>/dev/null | LC_ALL=C sort | md5sum; }
before=$(snapshot)
rm -f "$T"/calls/*
t_run "$work 7"
t_eq "$t_rc" 78
t_contains "$t_out" "no $T/home/.pi/agent/settings.json: run install.sh"
t_eq "$(t_calls docker)$(t_calls systemctl)" ""
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
t_eq "$(snapshot)" "$before"
t_eq "$(cat "$ws/marker")" current
t_eq "$(cat "$ws.before-retry/marker")" kept
t_eq "$(find "$T/home/.pi/agent/settings.json" -maxdepth 0 2>/dev/null | wc -l)" 0
mv "$T/settings.kept" "$T/home/.pi/agent/settings.json"
t_run "$work 7"

t_case "the token is passed by name, never written into the arguments"
t_lacks "$(cycle_args)" "test-token"

t_case "nothing that controls the host is reachable"
args=$(cat "$T"/calls/docker.args.*)
t_contains "$args" "pi-rukas-headless"
t_lacks "$args" "docker.sock"
t_lacks "$args" ".ssh"
t_lacks "$args" ".config/tehas"
t_lacks "$args" "--privileged"

t_case "host aliases become --add-host entries"
t_contains "$(cycle_args)" "--add-host=models:192.0.2.9"

t_done
