#!/usr/bin/env bash
# Tests for libexec/work: what the image says it supports, and what a cycle
# is refused or given because of it; the push check and Pi's settings.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/work_lib.sh
. "$(dirname "$0")/work_lib.sh" || exit 1
stub_github

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

# --- What the product's commit holds ------------------------------------------
# The clone is read on the host before any container starts, and only at the
# commit it was made at: its .pi/verify-cmd is the gate pi-rukas runs, and
# pi-rukas guesses a gate from package.json or Cargo.toml, so only a product
# with none of the three parks. The host's checkout is not read: it is the tree
# a person works in, so what it holds uncommitted or untracked decides nothing.
# These cases build the product of their own files and fetch the checkout at it,
# so they come first: the last of them leaves the gate in place for the cases
# that follow, which take the product as they find it.
# One commit holding just these files, put on the product's main, and the
# checkout fetched at it. The file is named, then holds the lines given for it:
# a case says what the product holds in its own words, and the product is the
# one commit at its tip, so the case before loses nothing but its own file. The
# empty commit is the first one, so a product of no files at all is the tree
# work_lib.sh makes.
product_commit() { # <name> <line>...
  local f=$1
  shift
  # The workspace of this case goes; what the last cycle left does not, unless
  # the case says it does. aside() in tehas work moves it to the name beside and
  # takes that name with it, so a marker put there stays across a park.
  rm -rf "$ws" "$ws.state" "$T"/calls/*
  # The tracked tree is the file named and the product's gate: the gate is a
  # tracked file of the product's branch (test/work_lib.sh), and a product of
  # none of the three is what parks a cycle, which product_nothing makes.
  [ "$(git -C "$upstream" ls-files | wc -l)" = 0 ] ||
    git -C "$upstream" rm -rq --cached -- .
  case $f in
    */*) mkdir -p "$upstream/$(dirname "$f")" ;;
  esac
  : >"$upstream/$f"
  while [ "$#" -gt 0 ]; do
    printf '%s\n' "$1" >>"$upstream/$f"
    shift
  done
  git -C "$upstream" add -f -- "$f"
  git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m product
  git -C "$checkout" fetch -q "$upstream" +refs/heads/main:refs/remotes/origin/main
  git -C "$checkout" checkout -q main
}

# The product whose commit holds nothing at all, which is what parks a cycle.
# work_lib.sh leaves the gate in the product's worktree too, so it goes from the
# index and from that worktree alike.
product_nothing() {
  rm -rf "$ws" "$ws.state" "$T"/calls/*
  rm -f "$upstream/.pi/verify-cmd"
  [ "$(git -C "$upstream" ls-files | wc -l)" = 0 ] ||
    git -C "$upstream" rm -rq --cached -- .
  git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m nothing
  # The product with no gate and no stack has an empty tree at its tip: the
  # fixture's gate is tracked, so it goes from the worktree as well as the index.
  if [ -n "$(git -C "$upstream" ls-tree -r --name-only main)" ]; then
    echo "tehas work tests: the product of nothing still holds a file"
    exit 1
  fi
  git -C "$checkout" fetch -q "$upstream" +refs/heads/main:refs/remotes/origin/main
  git -C "$checkout" checkout -q main
}

t_case "a product with no gate and no stack the runner can guess parks the cycle: 78, named, and no container"
product_nothing
rm -rf "$ws.before-retry"
t_run "$work 7"
t_eq "$t_rc" 78
t_contains "$t_out" "acme/widget has no .pi/verify-cmd and no package.json or Cargo.toml"
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
# The park keeps the clone it made, as the path that cannot make one does: only
# a directory of the cycle before is ever moved, and never one that may hold
# commits of its own.
t_eq "$(git -C "$ws" rev-parse --verify origin/main 2>/dev/null | wc -l)" 1

t_case "a park leaves the workspace it made and what the last cycle left, and says so"
mkdir -p "$ws.before-retry"
echo unrecoverable >"$ws.before-retry/left-over.txt"
product_nothing
t_run "$work 7"
t_eq "$t_rc" 78
t_eq "$(cat "$ws.before-retry/left-over.txt")" unrecoverable
t_contains "$t_out" "holds what the last cycle left"
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0

t_case "a gate of comments and blank lines only is no gate: the cycle parks"
product_commit .pi/verify-cmd '# scripts/verify.sh' '' '   '
t_run "$work 7"
t_eq "$t_rc" 78
t_contains "$t_out" "has no .pi/verify-cmd and no package.json or Cargo.toml"
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0

t_case "the checkout's own files decide nothing: a product with a gate in its commit runs"
product_commit .pi/verify-cmd 'scripts/verify.sh'
rm -rf "$checkout/.pi"
t_run "$work 7"
t_eq "$t_rc" 2

t_case "a product whose gate names the script runs, and the clone of it holds the gate"
product_commit .pi/verify-cmd 'scripts/verify.sh'
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(cat "$ws/.pi/verify-cmd")" "scripts/verify.sh"

t_case "a product of package.json and no gate runs: pi-rukas guesses the stack"
product_commit package.json '{"name":"widget"}'
t_run "$work 7"
t_eq "$t_rc" 2

t_case "a product of Cargo.toml and no gate runs"
product_commit Cargo.toml '[package]' 'name = "widget"'
t_run "$work 7"
t_eq "$t_rc" 2

t_case "a stack file only in the checkout's worktree is no stack: the cycle parks"
product_nothing
printf '{"name":"widget"}\n' >"$checkout/package.json"
t_run "$work 7"
t_eq "$t_rc" 78
t_contains "$t_out" "has no .pi/verify-cmd and no package.json or Cargo.toml"
rm -f "$checkout/package.json"
product_commit .pi/verify-cmd 'scripts/verify.sh'

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

t_case "an image that does not read the child's bound stops the cycle: 78, named"
caps_fresh
jq '.inputs |= map(select(.name != "PI_ENSEMBLE_SPAWN_TIMEOUT_MS"))' "$T/caps.json" >"$T/caps.new" && mv "$T/caps.new" "$T/caps.json"
t_run "$work 7"
caps_refused
t_contains "$t_out" "does not support what this cycle is given, not starting a cycle"
t_contains "$t_out" "the input PI_ENSEMBLE_SPAWN_TIMEOUT_MS"
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

# A refresh's record, as the dispatcher writes it, or another start's. The
# body it approved is the one the stub answers with, so the text check of a
# start that carries a record is met.
refresh_record() { # <how>
  local -a pr=()
  mkdir -p "$fx/net" "$T/home/tehas/state/cycles"
  issue_is "" NONE
  t_store_db "$T/home/tehas"
  [ "$1" != refresh ] || pr=(--refresh-pr 50 --refresh-head "$(printf 'a%.0s' {1..40})")
  t_approve "$T/home/tehas" 7 alice "$1" "$(date -u -d 2026-10-01T00:00:00Z +%s)" \
    --body-sha "$(jq -n '{body: ""}' | tehas_body_sha256)" "${pr[@]}"
  t_started "$T/home/tehas" 7
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
t_store_db "$T/home/tehas"
caps_good

t_case "a refresh on an image that does not take --continue-pr starts nothing"
caps_fresh
refresh_record refresh
t_run "$work 7"
caps_refused
t_contains "$t_out" "the flag --continue-pr"
t_store_db "$T/home/tehas"

for how in label retry-tick outage-retry fixer; do
  t_case "a start of how $how never passes --continue-pr, also when the image takes it"
  caps_fresh
  caps_continue
  refresh_record "$how"
  t_run "$work 7"
  t_eq "$t_rc" 2
  t_lacks "$(cycle_args | tr '\n' ' ')" "--continue-pr"
  t_store_db "$T/home/tehas"
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

t_case "the handoff channel and allowance, the deadline and the gate runs at once are this command's, not the instance's"
caps_fresh
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9 PI_ENSEMBLE_HANDOFF_CHANNEL=forge \
  PI_ENSEMBLE_CYCLE_BUDGET_MS=60000 PI_ENSEMBLE_VERIFY_CONCURRENCY=9 PI_ENSEMBLE_CYCLE_DEADLINE=1 \
  PI_ENSEMBLE_HANDOFF_TIMEOUT_MS=5000
before=$(date +%s)
t_run "$work 7"
t_eq "$(after -e | grep -c '^PI_ENSEMBLE_HANDOFF_CHANNEL')" 1
t_contains "$(after -e)" "PI_ENSEMBLE_HANDOFF_CHANNEL=host"
t_eq "$(after -e | grep '^PI_ENSEMBLE_HANDOFF_TIMEOUT_MS')" "PI_ENSEMBLE_HANDOFF_TIMEOUT_MS=480000"
t_lacks "$(after -e)" "PI_ENSEMBLE_CYCLE_BUDGET_MS"
# A container of a 2048m ceiling runs one gate at a time.
t_eq "$(after -e | grep '^PI_ENSEMBLE_VERIFY_CONCURRENCY')" "PI_ENSEMBLE_VERIFY_CONCURRENCY=1"
# Six hours less the handoff allowance, in milliseconds since the epoch.
deadline=$(after -e | sed -n 's/^PI_ENSEMBLE_CYCLE_DEADLINE=//p')
t_eq "$(after -e | grep -c '^PI_ENSEMBLE_CYCLE_DEADLINE')" 1
t_eq "$([ "$deadline" -ge $(((before + 21120) * 1000)) ] && [ "$deadline" -le $(((before + 21120 + 60) * 1000)) ] && echo in-range)" in-range
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9

t_case "the deadline does not grow with the run of the gate"
caps_fresh
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9 TEHAS_WORK_TIMEOUT=7200 TEHAS_VERIFY_TIMEOUT=2400
before=$(date +%s)
t_run "$work 7"
deadline=$(after -e | sed -n 's/^PI_ENSEMBLE_CYCLE_DEADLINE=//p')
# 7200 - 480
t_eq "$([ "$deadline" -ge $(((before + 6720) * 1000)) ] && [ "$deadline" -le $(((before + 6720 + 60) * 1000)) ] && echo in-range)" in-range
t_eq "$(after -e | grep '^PI_ENSEMBLE_HANDOFF_TIMEOUT_MS')" "PI_ENSEMBLE_HANDOFF_TIMEOUT_MS=480000"
caps_good

t_case "a time limit with no room for a deadline gives none, says so, and does not need the feature"
caps_fresh
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9 TEHAS_WORK_TIMEOUT=480 TEHAS_VERIFY_TIMEOUT=120 TEHAS_CHILD_TIMEOUT=480
jq 'del(.features.cycleDeadline)' "$T/caps.json" >"$T/caps.new" && mv "$T/caps.new" "$T/caps.json"
t_run "$work 7"
t_lacks "$t_out" "does not support"
t_contains "$t_out" "the time limit of 480s leaves no room for a deadline before it, the cycle has none"
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

t_case "the push check's container gets the workspace, the same variables and the security options, no more"
t_eq "$(preflight_args | unknown_options "$image" "--user -e -w -v --entrypoint --cap-drop --cap-add --security-opt --tmpfs" "--rm")" ""
t_eq "$(preflight_args | security_line)" "$security_opts "
t_eq "$(preflight_args | security_of --cap-add | LC_ALL=C sort)" "CHOWN
FOWNER"
# The whole line is counted, not just the block security_line matches: a second
# mount stands outside it and is the one drift the contiguous match cannot see.
t_eq "$(preflight_args | count_of --tmpfs)" "1"
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
given=$(cat "$T"/calls/docker.args.*)
t_contains "$given" "pi-rukas-headless"
t_lacks "$given" "docker.sock"
t_lacks "$given" ".ssh"
t_lacks "$given" ".config/tehas"
t_lacks "$given" "--privileged"

# The options themselves, as tehas-cycle sandbox prints them: the one list the
# cycle, the planning turn, the session and the push check are all given. The
# units pin them on the docker line; this pins the source of them.
real=$tree/libexec/tehas-cycle.real
words_of() { t_run ". '$T_ROOT/lib/config.sh'; tehas_load || exit 9; '$real' sandbox limits 'tehas work' 'a cycle' | tr '\0' '\n'; exit \"\${PIPESTATUS[0]}\""; }
t_case "the security options drop every capability and take privileges away"
words_of
t_eq "$t_rc" 0
t_eq "$(printf '%s\n' "$t_out" | sed -n '15,24p' | tr '\n' ' ')" "$security_opts "
# No capability is left in the default list by name, and nothing is added
# beyond the two a gate needs: a wider list here is a change of the contract.
t_eq "$(printf '%s\n' "$t_out" | sed -n 's/^--cap-add=//p')" ""

# The guard of tehas_sandbox_limits (lib/sandbox.sh), which judges what the
# program hands out before Docker is called: asked of the command, so a guard
# that drifted from the list is a failure in this file and not a cycle that stops
# on a host. The tables of the size are in console/core/src/sandbox.rs.
t_case "a list the program hands out that is not the documented one starts no cycle"
short=$T/short-caps
mkdir -p "$short/libexec"
ln -s "$T_ROOT/lib" "$short/lib"
ln -s "$T_ROOT/libexec/work" "$short/libexec/work"
fourteen="--cgroup-parent tehas.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096 --cpu-shares 1024"
for words in "--cgroup-parent tehas.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096" \
  "--cgroup-parent other.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096 --cpu-shares 1024" \
  "--memory 2048m --cgroup-parent tehas.slice --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096 --cpu-shares 1024" ""; do
  rm -f "$T"/calls/*
  # shellcheck disable=SC2086  # the words are the list
  t_fake_limits "$short/libexec" $words ${words:+$security_opts}
  t_run "$short/libexec/work 7"
  t_eq "$t_rc" 78
  t_contains "$t_out" "tehas work: the container limits could not be made, not starting"
  t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
done
for opts in "--cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true" \
  "--cap-drop NET_RAW --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true --tmpfs /tmp:rw,exec,nosuid,nodev,size=1024m,mode=1777"; do
  rm -f "$T"/calls/*
  # shellcheck disable=SC2086  # the words are the list
  t_fake_limits "$short/libexec" $fourteen $opts
  t_run "$short/libexec/work 7"
  t_eq "$t_rc" 78
  t_contains "$t_out" "tehas_sandbox_security gave "
  t_contains "$t_out" "wants 10: not starting a cycle"
  t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
done

# A cycle's /tmp is a tmpfs, whose pages count against --memory and cannot be
# reclaimed: one as large as the ceiling ends in OOM kills, not ENOSPC. It is
# half the ceiling, and without a ceiling there is no list to start with.
t_case "the /tmp tmpfs is half the container's memory ceiling, and no ceiling gives no options"
words_of
t_contains "$t_out" "/tmp:rw,exec,nosuid,nodev,size=1024m,mode=1777"
# size=0 would be no bound at all, so a ceiling under 10 MiB is no ceiling.
printf '%s\n' FragmentPath=/etc/systemd/system/tehas.slice ActiveState=active MemoryMax=33554432 >"$T/slice.show"
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9 TEHAS_CYCLE_RESERVE=6m
words_of
t_eq "$t_rc" 78
t_contains "$t_out" "tehas work: tehas_sandbox_security gave 0 words, wants 0: not starting a cycle"
slice_good
t_instance "${T_MINIMAL[@]}" PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
t_run "$work 7"

# The manual's table of "What a container may not do" is the same list as the
# function's, option by option and value by value: the page and the container
# are pinned to one another, so a row edited on its own fails here.
t_case "the manual's table names the options the container is given"
t_eq "$(awk '
    /^### What a container may not do$/ { inside = 1; next }
    inside && /^##/ { inside = 0 }
    inside && substr($0, 1, 3) == "| `" {
      row = substr($0, 4)
      opt = row; sub(/`.*$/, "", opt)
      val = row; sub(/^[^|]*\|[ `]*/, "", val); sub(/`.*$/, "", val)
      if (opt != "" && val != "") out[++n] = opt "|" val
    }
    END { for (i = 1; i <= n; i++) print out[i] }
  ' "$T_ROOT/host/docs/host.md")" "$(security_doc_rows)"

t_case "no container of a cycle, a turn, a session or the push check is started privileged or with the host's network"
t_lacks "$(cat "$T"/calls/docker.args.*)" "--privileged"
t_lacks "$(cat "$T"/calls/docker.args.*)" "--net=host"
t_lacks "$(cat "$T"/calls/docker.args.*)" "--network host"

# The credential helper git is given, by name and by value: the container's
# entry, the key tehas setup asks the host's git about, and the command the gh
# stub of test/setup_test.sh writes are one contract in lib/sandbox.sh, so a
# change to its spelling that only one of the three follows fails here. The
# value is not a secret: it is a command on PATH, and the host's operator's own
# helper is the only value that is withheld, in libexec/setup.
t_case "the helper a container's git runs is the one key and the one command"
t_run ". '$T_ROOT/lib/config.sh'; . '$T_ROOT/lib/sandbox.sh'; printf '%s\n' \"\$(tehas_git_github_host)\" \"\$(tehas_git_credential_helper_key)\" \"\$(tehas_sandbox_helper_command)\""
t_eq "$t_rc" 0
t_eq "$(printf '%s\n' "$t_out")" "github.com
credential.https://github.com.helper
!gh auth git-credential"

# The container's own entries, read off the docker options of the cycle the
# cases above started: the name on one line and its value on the next, the way
# docker reads them. The first value is empty on purpose - the entry that ends
# any list an operator's file left before it - so the empty line is matched
# against the name itself rather than read back as nothing.
t_eq "$(after -e | sed -n 's/^GIT_CONFIG_KEY_0=//p')" "credential.https://github.com.helper"
t_eq "$(after -e | sed -n 's/^GIT_CONFIG_KEY_1=//p')" "credential.https://github.com.helper"
t_eq "$(after -e | sed -n 's/^GIT_CONFIG_VALUE_1=//p')" "!gh auth git-credential"
t_eq "$(after -e | grep -c '^GIT_CONFIG_VALUE_0=$')" 1

t_case "host aliases become --add-host entries"
t_contains "$(cycle_args)" "--add-host=models:192.0.2.9"

t_done
