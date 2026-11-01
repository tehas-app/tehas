#!/usr/bin/env bash
# Tests for the improvement loop: the record a cycle leaves (tehas-cycle
# after), what a cycle ran with (lib/improve.sh), and how a whole log is packed.
# shellcheck disable=SC2016  # stub bodies and snippets are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/improve_lib.sh
. "$(dirname "$0")/improve_lib.sh" || exit 1

t_cycle || t_bad "no tehas-cycle: $t_cycle_why"
# lib <snippet>: run it with the settings and the loop's library loaded.
# tehas_improve_record is the verb under the arguments of the function it
# replaced.
lib() {
  t_run '. "$T_ROOT/lib/config.sh"; tehas_load || exit 78
tehas_improve_record() { "$T/cycle-bin" after "$1" < <(printf "%s\0" --attempt 1 --started "$2" --rc "$3" --status "$4" --step "$5" --pr "${6:-}" --oom "${7:-0}" --image "${8:-}" --rukas-image "${9:-}" --cap "${10:-}" --reason "${11:-}" --build "${12:-}" --forks 0 --signalled 1 --workdir /nowhere --console true --status-cmd true); }
'"$1"
}

# --- The record of a cycle ---------------------------------------------------
started=$(($(date +%s) - 100))
cycle_log() { # the files a cycle of issue 7 leaves on the host
  {
    echo '{"type":"extension_ui_request","id":"a","method":"setStatus","statusText":"review"}'
    echo '{"type":"extension_ui_request","id":"b","method":"setStatus","statusText":"review"}'
    echo '{"type":"message_update","delta":"x"}'
    echo 'not json'
    jq -n -c --arg t "$(printf 'y%.0s' $(seq 1 2500))" '{type:"message_end",text:$t}'
  } >"$home/logs/work-$REC.jsonl"
  echo "a warning" >"$home/logs/work-$REC.jsonl.err"
  echo "preflight ok" >"$home/logs/work-$REC.jsonl.preflight"
  echo "console text" >"$home/live/console-7.txt"
}

t_case "a cycle's end writes a whole record"
reset
cycle_log
lib "tehas_improve_record 7 $started 0 handoff review https://github.com/acme/widget/pull/9"
t_eq "$t_rc" 0
r=$base/window/$REC
t_eq "$(jq -c '[.issue, .status, .step, .rc, .pr, .log, .oomKills]' "$r/cycle.json")" \
  "[7,\"handoff\",\"review\",0,\"https://github.com/acme/widget/pull/9\",\"work-$REC.jsonl\",0]"
t_eq "$(cat "$r/console.txt")" "console text"
t_eq "$(cat "$r/err.txt")" "a warning"
t_eq "$(cat "$r/preflight.txt")" "preflight ok"

t_case "the stream is condensed: no deltas, a repeated status once, long strings cut"
t_eq "$(wc -l <"$r/stream.jsonl")" 2
t_lacks "$(cat "$r/stream.jsonl")" message_update
t_eq "$(sed -n 2p "$r/stream.jsonl" | jq '.text | length')" 2006

t_case "the record names the pi-rukas commit tehasbot cycle noted at the start"
t_eq "$(jq -c .rukas "$r/cycle.json")" null
reset
t_store_home "$home"
t_started "$home" 7 --by-hand --rukas fedcba9876543210fedcba9876543210fedcba98
cycle_log
lib "tehas_improve_record 7 $started 0 handoff review"
t_eq "$(jq -r .rukas "$base/window/$REC/cycle.json")" fedcba9876543210fedcba9876543210fedcba98

t_case "a record that cannot be read gives null, whatever versions.env says"
printf '%s\n' "PI_RUKAS_COMMIT=0123456789abcdef0123456789abcdef01234567" >"$home/versions.env"
t_store_answers settings-view 1 'store: busy #7: the database is locked'
cycle_log
lib "tehas_improve_record 7 $started 0 handoff review"
t_eq "$(jq -c '[.rukas, .settings]' "$base/window/$REC/cycle.json")" '[null,null]'
rm -rf "$home/libexec" "$home/versions.env"

t_case "the build commit of the runner's result is recorded beside the label's, and only a full commit"
cycle_log
lib "tehas_improve_record 7 $started 0 handoff review '' 0 '' aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa '' '' 1ca933e3cbbae6fff6f41d57973ba2af8ff805fd"
t_eq "$(jq -c '[.rukas, .buildCommit]' "$base/window/$REC/cycle.json")" '["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","1ca933e3cbbae6fff6f41d57973ba2af8ff805fd"]'
cycle_log
lib "tehas_improve_record 7 $started 0 handoff review '' 0 '' '' '' '' 1ca933e"
t_eq "$(jq -c .buildCommit "$base/window/$REC/cycle.json")" null

t_case "the image and its commit, as the container started, win over both"
t_store_home "$home"
t_started "$home" 7 --by-hand --rukas fedcba9876543210fedcba9876543210fedcba98
printf '%s\n' "PI_RUKAS_COMMIT=0123456789abcdef0123456789abcdef01234567" >"$home/versions.env"
img=sha256:$(printf 'e%.0s' {1..64})
cycle_log
lib "tehas_improve_record 7 $started 0 handoff review '' 0 $img aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
t_eq "$(jq -c '[.image, .rukas]' "$base/window/$REC/cycle.json")" "[\"$img\",\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"]"
cycle_log
lib "tehas_improve_record 7 $started 0 handoff review '' 0 'sha256:nothex' 'not a commit'"
t_eq "$(jq -c '[.image, .rukas]' "$base/window/$REC/cycle.json")" '[null,"fedcba9876543210fedcba9876543210fedcba98"]'
cycle_log
lib "tehas_improve_record 7 $started 0 handoff review"
t_eq "$(jq -c '.image' "$base/window/$REC/cycle.json")" null
rm -rf "$home/state" "$home/libexec" "$home/versions.env"

t_case "the cap and its reason are recorded, reduced, and null when there are none"
cycle_log
lib "tehas_improve_record 7 $started 0 handoff handoff '' 0 '' '' 'step-failed:adversarial\`' 'Connection error. @x <b>'"
t_eq "$(jq -c '[.cap, .capReason]' "$base/window/$REC/cycle.json")" '["step-failed:adversarial","Connection error. x b"]'
cycle_log
lib "tehas_improve_record 7 $started 0 handoff review"
t_eq "$(jq -c '[.cap, .capReason]' "$base/window/$REC/cycle.json")" '[null,null]'

t_case "a cycle without a pull request records null"
reset
cycle_log
lib "tehas_improve_record 7 $started 1 aborted handoff"
t_eq "$(jq -c .pr "$base/window/$REC/cycle.json")" null

t_case "the seventh argument is the processes killed for memory, oomKills in the record"
reset
cycle_log
lib "tehas_improve_record 7 $started 137 killed develop '' 3"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.status, .rc, .pr, .oomKills]' "$base/window/$REC/cycle.json")" '["killed",137,null,3]'

t_case "a seventh argument that is not a number counts as nothing"
for bad in "3; rm -rf /" "x" "-1" "1e3" "99999999999999999999"; do
  reset
  cycle_log
  lib "tehas_improve_record 7 $started 2 running develop '' '$bad'"
  t_eq "$t_rc" 0
  t_eq "$(jq -c .oomKills "$base/window/$REC/cycle.json")" 0
done

t_case "with the loop off nothing is written"
reset
t_instance "${T_MINIMAL[@]}"
cycle_log
lib "tehas_improve_record 7 $started 0 handoff review"
t_eq "$t_rc" 0
[ ! -e "$base" ] || t_bad "$base was made"

t_case "no log since the cycle began: no record, and the caller is told"
reset
lib "tehas_improve_record 7 $started 0 handoff review"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas: improve: no log of issue 7 since the cycle began, no record written"
t_contains "$t_out" "WARN: no record of #7 for the improvement loop"
[ -z "$(ls -A "$base/window" 2>/dev/null)" ] || t_bad "a record was written"

t_case "an older log of the same issue is not taken for this cycle's"
reset
cycle_log
touch -d '2 hours ago' "$home/logs/work-$REC.jsonl"
lib "tehas_improve_record 7 $started 0 handoff review"
t_contains "$t_out" "WARN: no record of #7 for the improvement loop"

t_case "two cycles that end at once leave two whole records"
reset
cycle_log
cp "$home/logs/work-$REC.jsonl" "$home/logs/work-8-261010-200150.jsonl"
lib "tehas_improve_record 7 $started 0 handoff review & tehas_improve_record 8 $started 0 handoff review & wait"
t_eq "$(find "$base/window" -name cycle.json | wc -l)" 2
t_eq "$(find "$base" -maxdepth 1 -name '.new.*' | wc -l)" 0

t_case "the record holds what the cycle was given of the issue's attachments, and never a file of them"
reset
cycle_log
mkdir -p "$home/state/cycles"
echo '{"images":[{"file":"image-1.png"}],"leftOut":[],"issue":7}' >"$home/state/cycles/7.manifest.json"
echo '# The images, described' >"$home/state/cycles/7.images.md"
echo '# The pull request before this retry' >"$home/state/cycles/7.retry-context.md"
printf 'PNG' >"$home/state/cycles/7.image-1.png"
lib "tehas_improve_record 7 $started 0 handoff review"
t_eq "$t_rc" 0
r=$base/window/$REC
t_eq "$(cd "$r" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" \
  "./console.txt ./cycle.json ./err.txt ./images.md ./manifest.json ./preflight.txt ./retry-context.md ./stream.jsonl "
t_eq "$(cmp "$r/manifest.json" "$home/state/cycles/7.manifest.json" && echo same)" same
t_eq "$(cat "$r/retry-context.md")" "# The pull request before this retry"

t_case "an earlier cycle's, a link and one over the bound are not taken"
reset
cycle_log
mkdir -p "$home/state/cycles"
echo '{}' >"$home/state/cycles/7.manifest.json"
touch -d '2 hours ago' "$home/state/cycles/7.manifest.json"
echo secret >"$T/elsewhere"
ln -s "$T/elsewhere" "$home/state/cycles/7.images.md"
head -c 300000 /dev/zero | tr '\0' a >"$home/state/cycles/7.retry-context.md"
lib "tehas_improve_record 7 $started 0 handoff review"
t_eq "$t_rc" 0
t_eq "$(cd "$base/window/$REC" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" \
  "./console.txt ./cycle.json ./err.txt ./preflight.txt ./stream.jsonl "

t_case "the analyst is told what the attachment files are and what to look for in them"
t_contains "$(cat "$T_ROOT/lib/improve-prompt.md")" "\`manifest.json\`, \`images.md\`, \`retry-context.md\`"
t_contains "$(cat "$T_ROOT/lib/improve-prompt.md")" "a cycle given images or files whose plan does not name what they show"
t_contains "$(cat "$T_ROOT/lib/improve-prompt.md")" "the instance gives a cycle none"
t_contains "$(cat "$T_ROOT/lib/improve-prompt.md")" "this instance gave it no way to see"

# --- What the driver said of children that ran long --------------------------
# The runner's own line for a dispatch that crossed the bound it watches. The
# repeat fields are what pi-rukas adds; a runner that has them names the
# command, how often it repeated and how many identical results came back.
slow_lines() { # the slow lines of one cycle, appended to its stream
  jq -n -c --arg cmd "bash -c 'unset -f nothere; echo rc=$?'" \
    '{v: 1, kind: "dispatch-slow", issue: 7, step: "lens-review",
      role: "code-review-specialist-p", jobId: "mvoc4hbz", elapsedMs: 400257,
      turns: 257, tokens: 40009732, command: $cmd, count: 24,
      identicalResultCount: 24}' >>"$home/logs/work-$REC.jsonl"
  printf '%s\n' \
    '{"v":1,"kind":"dispatch-slow","unstable":true,"step":"develop","role":"developer","elapsedMs":1200000,"turns":300,"tokens":32600000}' \
    >>"$home/logs/work-$REC.jsonl"
}

t_case "the cycle's dispatch-slow lines are kept of the record, with the repeat fields"
reset
cycle_log
t_store_home "$home"
slow_lines
lib "tehas_improve_record 7 $started 0 handoff review"
t_eq "$t_rc" 0
t_eq "$(jq -c -s 'length' "$base/window/$REC/slow.jsonl")" 2
t_eq "$(jq -c -s '.[0] | [.step, .role, .elapsedMs, .turns, .tokens, .count, .identicalResultCount]' "$base/window/$REC/slow.jsonl")" \
  '["lens-review","code-review-specialist-p",400257,257,40009732,24,24]'
t_contains "$(cat "$base/window/$REC/slow.jsonl")" "unset -f nothere"

t_case "a cycle with no slow line leaves no slow.jsonl, and the record's files are as they were"
reset
cycle_log
lib "tehas_improve_record 7 $started 0 handoff review"
t_eq "$t_rc" 0
t_eq "$(find "$base/window/$REC" -name slow.jsonl | wc -l)" 0
t_eq "$(cd "$base/window/$REC" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" \
  "./console.txt ./cycle.json ./err.txt ./preflight.txt ./stream.jsonl "

# One line of a stream can carry a command and an error tail of any size, so
# the kept lines are at most 20 and the file stays under 64 KiB. Each line
# names its own number twice, so the first and last of them say which of the
# 30 the record holds: the whole line is kept or left out, never cut.
t_case "of the slow lines the last 20 are kept and the file stays under 64 KiB"
reset
cycle_log
t_store_home "$home"
{
  pad=$(printf 'x%.0s' $(seq 1 400))
  for i in $(seq 1 30); do
    jq -n -c --arg i "$i" --arg pad "$pad" \
      '{v: 1, kind: "dispatch-slow", step: "lens-review", role: "developer", count: ($i + "|" + $i), fields: [$pad, $pad, $pad]}'
  done
} >>"$home/logs/work-$REC.jsonl"
lib "tehas_improve_record 7 $started 0 handoff review"
t_eq "$t_rc" 0
r=$base/window/$REC/slow.jsonl
t_eq "$(wc -l <"$r")" 20
t_eq "$(sed -n 1p "$r" | jq -r .count)" "11|11"
t_eq "$(sed -n '$p' "$r" | jq -r .count)" "30|30"
t_eq "$(($(wc -c <"$r") < 65536))" 1

t_case "a slow line too big for the bound is left out whole, and no half of one is kept"
reset
cycle_log
t_store_home "$home"
{
  # Four lines over the bound and one under it, so the bound has to refuse the
  # big ones and still keep the small one whole.
  pad=$(printf 'x%.0s' $(seq 1 40000))
  for i in $(seq 1 4); do
    jq -n -c --arg pad "$pad" '{v: 1, kind: "dispatch-slow", step: "lens-review", fields: [$pad, $pad]}'
  done
  jq -n -c '{v: 1, kind: "dispatch-slow", step: "handoff"}'
} >>"$home/logs/work-$REC.jsonl"
lib "tehas_improve_record 7 $started 0 handoff review"
t_eq "$t_rc" 0
r=$base/window/$REC/slow.jsonl
t_eq "$(jq -c -s '[.[].step]' "$r")" '["handoff"]'
t_eq "$(($(wc -c <"$r") >= 65536))" 0

t_case "a line that is not JSON, or names another kind, is not a slow line"
reset
cycle_log
{
  echo 'dispatch-slow in a line that is no JSON'
  echo '{"kind":"dispatch-completed","command":"the same probe","count":24}'
  echo '{"kind":"slow-notice"}'
} >>"$home/logs/work-$REC.jsonl"
lib "tehas_improve_record 7 $started 0 handoff review"
t_eq "$t_rc" 0
t_eq "$(find "$base/window/$REC" -name slow.jsonl | wc -l)" 0

# --- What a cycle ran with --------------------------------------------------
settings() { lib "export $1; \"\$T/cycle-bin\" settings \"\$TEHAS_HOME/s.json\" ${2:-3} ${3:-}"; }

t_case "the settings a container is given, the TEHAS_* that shape a cycle, and how many cycles ran"
reset
mkdir -p "$home"
settings "PI_ENSEMBLE_MAX_WORKSTREAMS=1 TEHAS_WORK_TIMEOUT=21600 TEHAS_CHILD_TIMEOUT=7200 TEHAS_HOST_ADDR=192.0.2.9 TEHAS_ANALYTICS_COOKIE=c" 3 \
  "-e GH_TOKEN -e PI_ENSEMBLE_SANDBOX_MODE=1 -e PI_ENSEMBLE_MAX_WORKSTREAMS -e GIT_CONFIG_COUNT=4 -v /a:/b -e PI_ENSEMBLE_EXTRA_IMAGE_DIRS=/w.images"
t_eq "$t_rc" 0
t_eq "$(jq -c .container "$home/s.json")" \
  '{"PI_ENSEMBLE_SANDBOX_MODE":"1","PI_ENSEMBLE_MAX_WORKSTREAMS":"1","PI_ENSEMBLE_EXTRA_IMAGE_DIRS":"[withheld]"}'
t_eq "$(jq -c .concurrentCycles "$home/s.json")" 3
t_eq "$(jq -r '.host.TEHAS_WORK_TIMEOUT' "$home/s.json")" 21600
# The bound of one child too: a spawn-backstop park is read from it, and an
# analyst cannot tell a low instance value from the default without it.
t_eq "$(jq -r '.host.TEHAS_CHILD_TIMEOUT' "$home/s.json")" 7200
t_eq "$(jq -c '.host | has("TEHAS_HOST_ADDR") or has("TEHAS_ANALYTICS_COOKIE") or has("TEHAS_REPO")' "$home/s.json")" false

t_case "a value whose name or shape may be a secret is withheld, and a count that is not one is null"
reset
mkdir -p "$home"
settings "PI_ENSEMBLE_X=1" "'3; x'" "-e PI_ENSEMBLE_API_KEY=k -e PI_ENSEMBLE_NOTE=ghp_$(printf 'a%.0s' {1..36}) -e PI_ENSEMBLE_OK=fine"
t_eq "$(jq -c '[.container.PI_ENSEMBLE_API_KEY, .container.PI_ENSEMBLE_NOTE, .container.PI_ENSEMBLE_OK, .concurrentCycles]' "$home/s.json")" \
  '["[withheld]","[withheld]","fine",null]'
t_lacks "$(cat "$home/s.json")" ghp_

t_case "only a value of a safe shape is written: no URL, host, path, address or token of any kind"
reset
mkdir -p "$home"
long=$(printf 'a1%.0s' {1..20})
opts=""
i=0
for v in "https://user:pw@models.example.com/v1" "http://gpu-box.internal:8080/v1" "gpu-box.internal:8080" \
  "/home/tehas/work" "a@b" "192.0.2.10" "192.0.2.10:11434" "hf_$long" "gsk_$long" "AIza$long" \
  "glpat-$(printf 'x%.0s' {1..20})" "db.lan" "llm:11434" "eyJhbGciOiJIUzI1NiJ9.e30.x" "$(printf 'f%.0s' {1..40})" "$long==" "a b" \
  "$(printf 'x,%.0s' {1..40})"; do
  i=$((i + 1))
  opts+=" -e 'PI_ENSEMBLE_V$i=$v'"
done
settings "PI_ENSEMBLE_X=1" 1 "$opts -e PI_ENSEMBLE_N=600000 -e PI_ENSEMBLE_B=true -e PI_ENSEMBLE_M=qwen3.8-flash-next -e PI_ENSEMBLE_L=a,b:c=d_e"
t_eq "$(jq -c '[.container | to_entries[] | select(.key | test("^PI_ENSEMBLE_V")) | .value] | unique' "$home/s.json")" '["[withheld]"]'
t_eq "$(jq -c '[.container | to_entries[] | select(.key | test("^PI_ENSEMBLE_V"))] | length' "$home/s.json")" "$i"
t_eq "$(jq -c '[.container.PI_ENSEMBLE_N, .container.PI_ENSEMBLE_B, .container.PI_ENSEMBLE_M, .container.PI_ENSEMBLE_L]' "$home/s.json")" \
  '["600000","true","qwen3.8-flash-next","a,b:c=d_e"]'
for bad in pw@ gpu-box example.com 192.0.2 hf_ gsk_ AIza glpat eyJ /home; do t_lacks "$(cat "$home/s.json")" "$bad"; done

t_case "a value whose name may be a secret or a host is withheld whatever its shape"
reset
mkdir -p "$home"
opts=""
for frag in KEY TOKEN SECRET PASS PWD COOKIE AUTH CRED BEARER PRIVATE HOST URL ADDR ENDPOINT DOMAIN PROXY; do
  opts+=" -e PI_ENSEMBLE_A_${frag}_Z=1"
done
settings "PI_ENSEMBLE_X=1" 1 "$opts"
t_eq "$(jq -c '[.container[]] | unique' "$home/s.json")" '["[withheld]"]'
t_eq "$(jq -c '.container | length' "$home/s.json")" 16

# on_record <settings JSON>: a cycle of issue 7 started, its image and these
# settings on its record as tehas work puts them there.
on_record() {
  t_store_home "$home"
  t_started "$home" 7 --by-hand
  printf '%s\n' "$1" >"$T/settings.json"
  TEHAS_HOME=$home flock -w 5 "$home/state/cycles/7.lock" "$T_STORE" cycle-image 7 --attempt 1 \
    --image "sha256:$(printf 'e%.0s' {1..64})" --settings "$T/settings.json" >/dev/null
}

t_case "the record carries this cycle's settings, the count and the provider retry line"
reset
cycle_log
on_record '{"container":{"PI_ENSEMBLE_MAX_WORKSTREAMS":"1"},"host":{"TEHAS_WORK_TIMEOUT":"21600"},"concurrentCycles":4}'
printf '%s\n' "preflight ok" "provider retry: timeoutMs=600000 maxRetries=8 maxRetryDelayMs=60000 httpIdleTimeoutMs=unset" \
  >"$home/logs/work-$REC.jsonl.preflight"
lib "tehas_improve_record 7 $started 0 handoff review"
r=$base/window/$REC
t_eq "$(jq -c '[.settings, .concurrentCycles, .providerRetry]' "$r/cycle.json")" \
  '[{"container":{"PI_ENSEMBLE_MAX_WORKSTREAMS":"1"},"host":{"TEHAS_WORK_TIMEOUT":"21600"}},4,"provider retry: timeoutMs=600000 maxRetries=8 maxRetryDelayMs=60000 httpIdleTimeoutMs=unset"]'

t_case "an earlier cycle's settings, settings of another shape, or a secret's give null"
for kind in old shape secret; do
  reset
  cycle_log
  case $kind in
    old)
      on_record '{"container":{"PI_ENSEMBLE_X":"1"},"host":{},"concurrentCycles":1}'
      # The next start clears what the last one had on record.
      t_started "$home" 7 --by-hand
      ;;
    shape) on_record '{"container":{"A":1},"host":{},"concurrentCycles":1}' ;;
    secret) on_record "{\"container\":{\"PI_ENSEMBLE_X\":\"ghp_$(printf 'a%.0s' {1..36})\"},\"host\":{},\"concurrentCycles\":1}" ;;
  esac
  lib "tehas_improve_record 7 $started 0 handoff review"
  t_eq "$(jq -c '[.settings, .concurrentCycles, .providerRetry]' "$base/window/$REC/cycle.json")" '[null,null,null]'
done

t_case "the analyst is told what the settings are and when a finding is a boundary one"
t_contains "$(cat "$T_ROOT/lib/improve-prompt.md")" "\`settings.container\`"
t_contains "$(cat "$T_ROOT/lib/improve-prompt.md")" "\`concurrentCycles\`"
t_contains "$(cat "$T_ROOT/lib/improve-prompt.md")" "\`providerRetry\`"
t_contains "$(cat "$T_ROOT/lib/improve-prompt.md")" "never guess it, never fill it in"
t_contains "$(cat "$T_ROOT/lib/improve-prompt.md")" "\`boundary\` is for a problem with a half on each side"
t_contains "$(cat "$T_ROOT/lib/improve-prompt.md")" "pi-rukas | console | gate | docs | boundary"

t_done
