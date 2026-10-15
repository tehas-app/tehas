#!/usr/bin/env bash
# Tests for the improvement loop: the record a cycle leaves (lib/improve.sh),
# how a whole log is packed, and a run of libexec/improve against a stub
# docker and a stub gh.
# shellcheck disable=SC2016  # stub bodies and snippets are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

home=$T/home/tehas
base=$home/tmp/improve
fx=$T/fx
REC=7-261010-200140

reset() {
  rm -rf "$fx" "$home" "$T"/calls/*
  mkdir -p "$fx" "$home/logs" "$home/live"
  echo '[]' >"$fx/open.json"
  echo '[]' >"$fx/closed.json"
  echo '[]' >"$fx/rukas-open.json"
  echo '[]' >"$fx/rukas-closed.json"
  T_ENV=("TEHAS_CGROUP_ROOT=$T_CGROUP")
  slice_good
  t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory
}
# A host with the root step done: the slice loaded from its file with 8 GiB,
# Docker on the systemd driver and cgroup v2, and nothing in the slice.
slice_good() {
  printf '%s\n' FragmentPath=/etc/systemd/system/tehas.slice ActiveState=active MemoryMax=8589934592 >"$T/slice.show"
  echo "systemd 2" >"$T/docker.info"
  slice_state 0.00
}
t_stub systemctl 'cat "$T/slice.show"'
# lib <snippet>: run it with the settings and the loop's library loaded.
lib() { t_run '. "$T_ROOT/lib/config.sh"; . "$T_ROOT/lib/improve.sh"; tehas_load || exit 78; '"$1"; }

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
mkdir -p "$home/state/cycles"
echo fedcba9876543210fedcba9876543210fedcba98 >"$home/state/cycles/7.rukas"
cycle_log
lib "tehas_improve_record 7 $started 0 handoff review"
t_eq "$(jq -r .rukas "$base/window/$REC/cycle.json")" fedcba9876543210fedcba9876543210fedcba98

t_case "a note that is not a commit records null, whatever versions.env says"
echo 'not a commit' >"$home/state/cycles/7.rukas"
printf '%s\n' "PI_RUKAS_COMMIT=0123456789abcdef0123456789abcdef01234567" >"$home/versions.env"
cycle_log
lib "tehas_improve_record 7 $started 0 handoff review"
t_eq "$(jq -c .rukas "$base/window/$REC/cycle.json")" null
rm -rf "$home/state/cycles" "$home/versions.env"

t_case "the build commit of the runner's result is recorded beside the label's, and only a full commit"
cycle_log
lib "tehas_improve_record 7 $started 0 handoff review '' 0 '' aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa '' '' 1ca933e3cbbae6fff6f41d57973ba2af8ff805fd"
t_eq "$(jq -c '[.rukas, .buildCommit]' "$base/window/$REC/cycle.json")" '["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","1ca933e3cbbae6fff6f41d57973ba2af8ff805fd"]'
cycle_log
lib "tehas_improve_record 7 $started 0 handoff review '' 0 '' '' '' '' 1ca933e"
t_eq "$(jq -c .buildCommit "$base/window/$REC/cycle.json")" null

t_case "the image and its commit, as the container started, win over both"
mkdir -p "$home/state/cycles"
echo fedcba9876543210fedcba9876543210fedcba98 >"$home/state/cycles/7.rukas"
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
rm -rf "$home/state/cycles" "$home/versions.env"

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
t_eq "$t_rc" 1
t_contains "$t_out" "no log of issue 7"
[ -z "$(ls -A "$base/window" 2>/dev/null)" ] || t_bad "a record was written"

t_case "an older log of the same issue is not taken for this cycle's"
reset
cycle_log
touch -d '2 hours ago' "$home/logs/work-$REC.jsonl"
lib "tehas_improve_record 7 $started 0 handoff review"
t_eq "$t_rc" 1

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

# --- What a cycle ran with --------------------------------------------------
settings() { lib "export $1; tehas_improve_settings \"\$TEHAS_HOME/s.json\" ${2:-3} ${3:-}"; }

t_case "the settings a container is given, the TEHAS_* that shape a cycle, and how many cycles ran"
reset
mkdir -p "$home"
settings "PI_ENSEMBLE_MAX_WORKSTREAMS=1 TEHAS_WORK_TIMEOUT=21600 TEHAS_HOST_ADDR=192.0.2.9 TEHAS_ANALYTICS_COOKIE=c" 3 \
  "-e GH_TOKEN -e PI_ENSEMBLE_SANDBOX_MODE=1 -e PI_ENSEMBLE_MAX_WORKSTREAMS -e GIT_CONFIG_COUNT=4 -v /a:/b -e PI_ENSEMBLE_EXTRA_IMAGE_DIRS=/w.images"
t_eq "$t_rc" 0
t_eq "$(jq -c .container "$home/s.json")" \
  '{"PI_ENSEMBLE_SANDBOX_MODE":"1","PI_ENSEMBLE_MAX_WORKSTREAMS":"1","PI_ENSEMBLE_EXTRA_IMAGE_DIRS":"[withheld]"}'
t_eq "$(jq -c .concurrentCycles "$home/s.json")" 3
t_eq "$(jq -r '.host.TEHAS_WORK_TIMEOUT' "$home/s.json")" 21600
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

t_case "the record carries this cycle's settings, the count and the provider retry line"
reset
cycle_log
mkdir -p "$home/state/cycles"
echo '{"container":{"PI_ENSEMBLE_MAX_WORKSTREAMS":"1"},"host":{"TEHAS_WORK_TIMEOUT":"21600"},"concurrentCycles":4}' \
  >"$home/state/cycles/7.settings"
printf '%s\n' "preflight ok" "provider retry: timeoutMs=600000 maxRetries=8 maxRetryDelayMs=60000 httpIdleTimeoutMs=unset" \
  >"$home/logs/work-$REC.jsonl.preflight"
lib "tehas_improve_record 7 $started 0 handoff review"
r=$base/window/$REC
t_eq "$(jq -c '[.settings, .concurrentCycles, .providerRetry]' "$r/cycle.json")" \
  '[{"container":{"PI_ENSEMBLE_MAX_WORKSTREAMS":"1"},"host":{"TEHAS_WORK_TIMEOUT":"21600"}},4,"provider retry: timeoutMs=600000 maxRetries=8 maxRetryDelayMs=60000 httpIdleTimeoutMs=unset"]'

t_case "an earlier cycle's settings, a link, or a file of another shape give null"
for kind in old link shape; do
  reset
  cycle_log
  mkdir -p "$home/state/cycles"
  f=$home/state/cycles/7.settings
  case $kind in
    old) echo '{"container":{},"host":{},"concurrentCycles":1}' >"$f"; touch -d '2 hours ago' "$f" ;;
    link) echo '{"container":{},"host":{},"concurrentCycles":1}' >"$T/elsewhere"; ln -s "$T/elsewhere" "$f" ;;
    shape) echo '{"container":{"A":1},"host":{},"concurrentCycles":1}' >"$f" ;;
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

# --- Packing a whole log -----------------------------------------------------
# 300 lines that compress badly, so a small limit forces pieces.
big_log() { for i in $(seq 1 300); do printf '%s\n' "$i" | sha256sum; done >"$1"; }

t_case "a log under the limit is one file"
reset
big_log "$fx/log"
lib 'tehas_improve_pack "$T/fx/log" "$T/fx/gz" ticket-7.log 1000000'
t_eq "$t_rc" 0
t_eq "$(ls "$fx/gz")" "ticket-7.log.gz"
t_eq "$(zcat "$fx/gz/ticket-7.log.gz" | cksum)" "$(cksum <"$fx/log")"

t_case "a log over the limit is numbered pieces that join back to it"
reset
big_log "$fx/log"
lib 'tehas_improve_pack "$T/fx/log" "$T/fx/gz" ticket-7.log 3000'
t_eq "$t_rc" 0
n=$(find "$fx/gz" -name 'ticket-7.log.*.gz' | wc -l)
[ "$n" -ge 2 ] || t_bad "expected pieces, got $n"
[ ! -e "$fx/gz/ticket-7.log.gz" ] || t_bad "the whole file was left beside the pieces"
t_eq "$(for i in $(seq 1 "$n"); do zcat "$fx/gz/ticket-7.log.$i.gz"; done | cksum)" "$(cksum <"$fx/log")"
t_eq "$(find "$fx/gz" -name '*.gz' -size +3000c | wc -l)" 0
t_eq "$(find "$fx/gz" -mindepth 1 ! -name '*.gz' | wc -l)" 0

t_case "a line too long for one piece: no pieces are left"
reset
head -c 20000 /dev/urandom | base64 -w 0 >"$fx/log"
echo >>"$fx/log"
echo "second line" >>"$fx/log"
lib 'tehas_improve_pack "$T/fx/log" "$T/fx/gz" ticket-7.log 3000'
t_eq "$t_rc" 1
t_eq "$(find "$fx/gz" -name '*.gz' | wc -l)" 0

t_case "the shape of a token is seen"
reset
echo "push with ghp_$(printf 'a%.0s' $(seq 1 36)) failed" >"$fx/log"
lib 'tehas_improve_secret "$T/fx/log"'
t_eq "$t_rc" 0
echo "-----BEGIN OPENSSH PRIVATE KEY-----" >"$fx/log"
lib 'tehas_improve_secret "$T/fx/log"'
t_eq "$t_rc" 0
echo "{\"apiKey\":\"sk-$(printf 'b%.0s' $(seq 1 24))\"}" >"$fx/log"
lib 'tehas_improve_secret "$T/fx/log"'
t_eq "$t_rc" 0

t_case "an ordinary log is not taken for a secret"
reset
big_log "$fx/log"
{
  echo "the ghp_ prefix and sk-short are words here"
  echo "check disk-usage-of-the-container-first"
  echo '{"branch":"task-1234-add-the-login-page"}'
} >>"$fx/log"
lib 'tehas_improve_secret "$T/fx/log"'
t_eq "$t_rc" 1

# --- A run -------------------------------------------------------------------
improve="$T_ROOT/libexec/improve"

# docker leaves $T/fx/findings.json where the analyst would write, and keeps
# its arguments one to a line. $T/fx/docker.rc is an exit status to fail with;
# $T/fx/docker.cut one to end with after the file is written, as an analyst
# stopped at its bound would.
t_stub docker '
if [ "$1" = info ]; then cat "$T/docker.info"; exit; fi
printf "%s\n" "$@" >"$T/calls/docker-args"
if [ "$1" = run ] && { true >&9; } 2>/dev/null; then : >"$T/calls/docker-lock"; fi
out=""
for a in "$@"; do case "$a" in *:/improve/out) out=${a%:/improve/out} ;; esac; done
cp "${out%/out}/in/issues.json" "$T/calls/issues.json" 2>/dev/null
[ -z "$out" ] || ls "${out%/out}/in/cycles" >"$T/calls/cycles"
[ -f "$T/fx/docker.rc" ] && { echo "No API key found for test-provider."; echo "See docs/models.md"; exit "$(cat "$T/fx/docker.rc")"; }
if [ -L "$T/fx/findings.json" ]; then cp -P "$T/fx/findings.json" "$out/findings.json"
elif [ -f "$T/fx/findings.json" ]; then cp "$T/fx/findings.json" "$out/findings.json"; fi
[ -f "$T/fx/docker.cut" ] && exit "$(cat "$T/fx/docker.cut")"
exit 0'

# timeout runs the command at once and keeps what it was asked.
t_stub timeout '[ "$1" = -k ] && shift 2
shift
exec "$@"'

# gh answers from fixtures. A file $T/fx/gh-<what>.rc makes that call fail.
# acme/rukas has a stack of its own, the rukas-*.json fixtures.
t_stub gh '
repo="" prev="" pre=""
for a in "$@"; do [ "$prev" = --repo ] && repo=$a; prev=$a; done
[ "$repo" = acme/rukas ] && pre=rukas-
case "$1 $2" in
  "issue list")
    [ -f "$T/fx/gh-${pre}list.rc" ] && exit 1
    # A cycle that ends while the stack is fetched.
    if [ -f "$T/fx/late" ]; then
      mkdir -p "$T/home/tehas/tmp/improve/window/$(cat "$T/fx/late")"
      echo "{}" >"$T/home/tehas/tmp/improve/window/$(cat "$T/fx/late")/cycle.json"
    fi
    case " $* " in *" --state closed "*) cat "$T/fx/${pre}closed.json" ;; *) cat "$T/fx/${pre}open.json" ;; esac ;;
  "issue create")
    n=$(($(cat "$T/fx/next" 2>/dev/null || echo 100) + 1))
    echo "$n" >"$T/fx/next"
    [ -f "$T/fx/gh-create.rc" ] && [ "$n" -ge "$(cat "$T/fx/gh-create.rc")" ] && exit 1
    while [ $# -gt 0 ]; do
      case "$1" in
        --body-file) cp "$2" "$T/calls/body-$n" ;;
        --title) printf "%s\n" "$2" >>"$T/calls/titles" ;;
      esac
      shift
    done
    echo "$repo" >>"$T/calls/filed-in"
    echo "https://github.com/$repo/issues/$n" ;;
  "issue comment")
    [ -f "$T/fx/gh-comment.rc" ] && exit 1
    n=$3
    while [ $# -gt 0 ]; do
      case "$1" in --body-file) cp "$2" "$T/calls/comment-$n" ;; esac
      shift
    done ;;
  "api "*)
    # The events of an issue and a comparison, from fixtures, as --jq would
    # leave them. Without a fixture the call fails.
    case " $* " in
      *"/events "*) [ -f "$T/fx/events" ] || exit 1; cat "$T/fx/events" ;;
      *"/compare/"*) [ -f "$T/fx/compare" ] || exit 1; cat "$T/fx/compare" ;;
    esac ;;
  "release view") [ -f "$T/fx/release" ]; exit $? ;;
  "release create") [ -f "$T/fx/gh-release.rc" ] && exit 1; : >"$T/fx/release" ;;
  "release upload")
    [ -f "$T/fx/gh-release.rc" ] && exit 1
    for a in "$@"; do case "$a" in *.gz) basename "$a" >>"$T/calls/assets" ;; esac; done ;;
esac'

record() { # <record>: a record in the window and the cycle's log in logs/
  mkdir -p "$base/window/$1"
  echo '{"issue":7}' >"$base/window/$1/cycle.json"
  echo '{"type":"agent_end"}' >"$home/logs/work-$1.jsonl"
}
finding() { # <title> [<jq object to merge in>]
  local more=${2:-}
  [ -n "$more" ] || more='{}'
  jq -n -c --arg t "$1" --arg rec "$REC" --argjson more "$more" '
    {title: $t, category: "cycle", body: "The gate ran twice.", evidence: "gate: ok",
     cycles: [$rec], duplicate_of: null} + $more'
}
findings() { printf '%s\n' "$@" | jq -s '{findings: .}' >"$fx/findings.json"; }
stack() { # <number> <title> [<state>] [<label>]
  jq -n -c --argjson n "$1" --arg t "$2" --arg s "${3:-OPEN}" --arg l "${4:-}" '
    {number: $n, title: $t, state: $s, body: "", labels: (if $l == "" then [] else [{name: $l}] end)}'
}
ready() { # a window with one record and one good finding
  reset
  record "$REC"
  findings "$(finding "The gate runs twice in one cycle")"
}
titles() { cat "$T/calls/titles" 2>/dev/null || true; }

t_case "with the loop off the command says so and calls nothing"
reset
t_instance "${T_MINIMAL[@]}"
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "is off"
t_eq "$(t_calls gh)$(t_calls docker)" ""
[ ! -e "$base" ] || t_bad "$base was made"

t_case "a malformed repository exits 78 before any call"
reset
record "$REC"
t_instance "${T_MINIMAL[@]}" "TEHAS_IMPROVE_REPO='not a repo'"
t_run "$improve"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)$(t_calls docker)" ""

t_case "an argument is refused"
reset
t_run "$improve now"
t_eq "$t_rc" 64

t_case "an empty window calls neither docker nor gh"
reset
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "cycles=0"
t_eq "$(t_calls gh)$(t_calls docker)" ""

t_case "a run files a finding and leaves nothing behind"
ready
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "cycles=1 findings=1 invalid=0 duplicates=0 of=none over=0 filed=1 issues=#101"
t_eq "$(titles)" "The gate runs twice in one cycle"
t_contains "$(t_calls gh)" "--label tehas:improvement"
t_lacks "$(t_calls gh)" "tehas:ready"
t_eq "$(find "$base" -mindepth 1 ! -name window | wc -l)" 0

t_case "the analyst's container is given exactly this"
args=$(cat "$T/calls/docker-args")
run_dir=$(printf '%s\n' "$args" | sed -n 's|:/improve/in:ro$||p')
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '-v' | grep -v -x -e '-v' -e '--')" \
  "$run_dir:/improve/in:ro
${run_dir%/in}/out:/improve/out
$T/home/.pi/agent/models.json:/home/vscode/.pi/agent/models.json:ro"
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '-e' | grep -v -x -e '-e' -e '--')" "HOME=/home/vscode"
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '--cgroup-parent' | tail -n 1)" tehas.slice
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '--cpus' | tail -n 1)" 1
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '--memory' | tail -n 1)" 2g
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '--memory-swap' | tail -n 1)" 2g
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '--pids-limit' | tail -n 1)" 1024
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '--cpu-shares' | tail -n 1)" 1024
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '--tools' | tail -n 1)" "read,grep,find,ls,write"
for flag in -p --no-session --no-extensions --no-skills --no-prompt-templates --no-context-files; do
  printf '%s\n' "$args" | grep -q -x -- "$flag" || t_bad "pi was not given $flag"
done
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '--model' | tail -n 1)" test-model
t_eq "$(printf '%s\n' "$args" | grep -x -A1 -- '--provider' | tail -n 1)" test-provider
t_lacks "$args" docker.sock
t_lacks "$args" GH_TOKEN
t_lacks "$args" .ssh
t_lacks "$args" "$home/logs"

t_case "the analyst's container gets no option beyond these, and its limits are its own"
# Every option up to the image that is not one this command is known to pass,
# read the way docker reads them. See test/work_test.sh.
t_eq "$(printf '%s\n' "$args" | awk -v image="tehas/pi-rukas:uid$(id -u)" \
  -v valued=" --name --cgroup-parent --cpus --memory --memory-swap --pids-limit --cpu-shares -e -w -v " -v plain=" --rm --init " '
    NR == 1 { next } # "run"
    skip { skip = 0; next }
    $0 == image { found = 1; exit }
    index(valued, " " $0 " ") { skip = 1; next }
    index(plain, " " $0 " ") { next }
    { print }
    END { if (!found) print "no image" }')" ""
# A batch job: a quota of one CPU and 2g whatever the reserve is, and no floor.
t_lacks "$args" "--memory-reservation"
t_eq "$(printf '%s\n' "$args" | grep -c -x -- --init)" 1
t_lacks "$args" "--cidfile"

t_case "the slice is read once: one systemctl call, one docker info"
t_eq "$(t_calls systemctl)" "show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax"
t_eq "$(t_calls docker | grep -c '^info ')" 1

t_case "the issue body is the script's, with the evidence fenced and the log linked"
body=$(cat "$T/calls/body-101")
t_contains "$body" "improvement loop"
t_contains "$body" "test-model"
t_contains "$body" "**Category:** cycle"
t_contains "$body" "**Issues:** acme/widget#7"
t_contains "$body" "**Working on it:** This is a problem in the factory. It is fixed in this repository: a cycle builds the fix once a person applies \`tehas:ready\`."
t_contains "$body" "The gate ran twice."
t_contains "$body" '````text'
t_contains "$body" "https://github.com/acme/factory/releases/download/tehas-improve-logs/ticket-$REC.log.gz"
t_eq "$(cat "$T/calls/assets")" "ticket-$REC.log.gz"

# refused: 78, no GitHub call, no container, and the record still in the window.
refused() {
  t_eq "$t_rc" 78
  t_eq "$(t_calls gh)$(t_calls docker | grep -v '^info ')" ""
  [ -f "$base/window/$REC/cycle.json" ] || t_bad "the record is gone"
  t_eq "$(find "$base" -mindepth 1 -maxdepth 1 ! -name window | wc -l)" 0
}

t_case "refuses with 78 without the slice, before any docker run"
ready
printf '%s\n' FragmentPath= ActiveState=inactive MemoryMax=infinity >"$T/slice.show"
t_run "$improve"
refused
t_contains "$t_out" "tehas improve: no slice file on this host (sudo ./bootstrap.sh slice)"

t_case "a cgroupfs driver, cgroup v1 and a silent docker are refused the same way"
for answer in "cgroupfs 2" "systemd 1" ""; do
  ready
  echo "$answer" >"$T/docker.info"
  t_run "$improve"
  refused
  t_contains "$t_out" "tehas improve: "
  t_contains "$t_out" "ocker"
done

t_case "a slice with no limit is refused"
ready
printf '%s\n' FragmentPath=/etc/systemd/system/tehas.slice ActiveState=active MemoryMax=infinity >"$T/slice.show"
t_run "$improve"
refused
t_contains "$t_out" "tehas improve: tehas.slice has no memory limit"

t_case "with nothing to read the slice is not asked about"
reset
printf '%s\n' FragmentPath= ActiveState=inactive MemoryMax=infinity >"$T/slice.show"
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(t_calls systemctl)$(t_calls docker)" ""

t_case "GitHub does not give the stack: nothing runs and the window is as it was"
ready
: >"$fx/gh-list.rc"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(t_calls docker | grep -v '^info ')" ""
[ -f "$base/window/$REC/cycle.json" ] || t_bad "the record is gone"

t_case "ten open improvement issues: the run waits and the records stay"
ready
for i in $(seq 1 10); do stack "$i" "Finding $i" OPEN tehas:improvement; done | jq -s . >"$fx/open.json"
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "waiting"
t_eq "$(t_calls docker | grep -v '^info ')" ""
[ -f "$base/window/$REC/cycle.json" ] || t_bad "the record is gone"

t_case "eight open improvement issues leave room for two"
ready
for i in $(seq 1 8); do stack "$i" "Finding $i" OPEN tehas:improvement; done | jq -s . >"$fx/open.json"
findings "$(finding One)" "$(finding Two)" "$(finding Three)"
t_run "$improve"
t_eq "$(titles | wc -l)" 2
t_contains "$t_out" "over=1"

t_case "at most three issues a run"
ready
findings "$(finding One)" "$(finding Two)" "$(finding Three)" "$(finding Four)"
t_run "$improve"
t_eq "$(titles | wc -l)" 3
t_contains "$t_out" "filed=3"

t_case "the analyst fails: exit 1 and the records go back to the window"
ready
echo 3 >"$fx/docker.rc"
t_run "$improve"
t_eq "$t_rc" 1
t_contains "$t_out" "failed"
[ -f "$base/window/$REC/cycle.json" ] || t_bad "the record is gone"
t_eq "$(find "$base" -maxdepth 1 -name 'run-*' | wc -l)" 0
t_eq "$(titles)" ""

t_case "what a failed analyst printed is kept under the run's stamp, and the line says where"
kept=$(find "$home/logs" -name 'improve-analyst-*.log')
[[ $kept =~ /improve-analyst-[0-9]{6}-[0-9]{6}\.log$ ]] || t_bad "kept as '$kept'"
t_contains "$t_out" "the analyst ended with 3, its output is in $kept"
t_contains "$(cat "$kept")" "No API key found for test-provider."

t_case "a later run, failed or not, leaves the output of a failed one where it is"
ready
echo "from a run that failed" >"$home/logs/improve-analyst-261010-000000.log"
echo 3 >"$fx/docker.rc"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(cat "$home/logs/improve-analyst-261010-000000.log")" "from a run that failed"
t_eq "$(find "$home/logs" -name 'improve-analyst-*.log' | wc -l)" 2
rm "$fx/docker.rc"
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(find "$home/logs" -name 'improve-analyst-*.log' | wc -l)" 2

t_case "a run that works keeps no analyst output of its own"
ready
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(find "$home/logs" -name 'improve-analyst*' | wc -l)" 0

t_case "no findings file files nothing"
ready
rm "$fx/findings.json"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(titles)" ""

t_case "a findings file that is a link is not read"
ready
finding "From a file of the analyst's choosing" | jq -s '{findings: .}' >"$fx/elsewhere.json"
rm "$fx/findings.json"
ln -s "$fx/elsewhere.json" "$fx/findings.json"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(titles)" ""

t_case "a findings file over 1 MB is not read"
ready
{
  printf '{"findings":[],"pad":"'
  head -c 1100000 /dev/zero | tr '\0' a
  printf '"}'
} >"$fx/findings.json"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(titles)" ""

t_case "a file that is not the shape asked for files nothing"
ready
echo '["a list, not an object"]' >"$fx/findings.json"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(titles)" ""
[ -f "$base/window/$REC/cycle.json" ] || t_bad "the record is gone"

t_case "findings of the wrong shape are dropped one by one"
ready
findings "$(finding Good)" \
  "$(finding "$(printf 't%.0s' $(seq 1 101))")" \
  "$(finding "Two
lines")" \
  "$(finding BadCategory '{"category":"product"}')" \
  "$(finding LongBody "$(jq -n -c --arg b "$(printf 'b%.0s' $(seq 1 6001))" '{body:$b}')")" \
  "$(finding UnknownCycle '{"cycles":["9-000000-000000"]}')" \
  "$(finding NoCycles '{"cycles":[]}')" \
  "$(finding BadDuplicate '{"duplicate_of":999}')" \
  '"a string"'
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(titles)" "Good"
t_contains "$t_out" "findings=9 invalid=8"

t_case "a finding the model names a duplicate of is dropped"
ready
stack 4 "Pause and resume" | jq -s . >"$fx/open.json"
findings "$(finding "A pause switch on the host" '{"duplicate_of":4}')"
t_run "$improve"
t_eq "$(titles)" ""
t_contains "$t_out" "duplicates=1 of=#4"

t_case "a title that matches an open issue is dropped, whatever the model says"
ready
stack 4 "the gate runs twice, in one cycle!" | jq -s . >"$fx/open.json"
t_run "$improve"
t_eq "$(titles)" ""
t_contains "$t_out" "duplicates=1"

t_case "a title that matches a closed issue is filed"
ready
stack 4 "The gate runs twice in one cycle" CLOSED | jq -s . >"$fx/closed.json"
t_run "$improve"
t_eq "$(titles)" "The gate runs twice in one cycle"

t_case "the same title twice in one run is filed once"
ready
findings "$(finding "The gate runs twice")" "$(finding "the gate runs twice.")"
t_run "$improve"
t_eq "$(titles)" "The gate runs twice"

# --- A repository for pi-rukas -----------------------------------------------
with_rukas() { t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory TEHAS_IMPROVE_RUKAS_REPO=acme/rukas; }
rukas_finding() { finding "$1" '{"category":"pi-rukas"}'; }

t_case "a pi-rukas finding is filed in the repository for pi-rukas, the rest where they were"
ready
with_rukas
findings "$(finding "The gate runs twice")" "$(rukas_finding "The driver drops a path")"
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/filed-in")" "acme/factory
acme/rukas"
t_contains "$t_out" "filed=2 issues=#101,acme/rukas#102"
t_eq "$(t_calls gh | grep -c '^label create .*--repo acme/rukas')" 1
t_eq "$(t_calls gh | grep -c '^label create .*--repo acme/factory')" 1

t_case "each body says what working on it means: built here, or fixed in pi-rukas after a label"
t_contains "$(cat "$T/calls/body-101")" "a cycle builds the fix once a person applies \`tehas:ready\`"
body=$(cat "$T/calls/body-102")
t_contains "$body" "It is fixed here, in pi-rukas, and not worked around in acme/widget."
t_contains "$body" "Nothing starts until a person applies \`tehas:ready\`."
t_lacks "$body" "a cycle builds the fix"

t_case "its body names the product's issue, and the log stays in the loop's own repository"
t_contains "$body" "**Issues:** acme/widget#7"
t_contains "$body" "https://github.com/acme/factory/releases/download/tehas-improve-logs/ticket-$REC.log.gz"
t_lacks "$(t_calls gh | grep '^release ')" "acme/rukas"

t_case "a finding on cycles of two issues names both, each once"
ready
record 9-261010-210000
record 9-261010-220000
findings "$(finding "The gate runs twice" "{\"cycles\":[\"$REC\",\"9-261010-210000\",\"9-261010-220000\"]}")"
t_run "$improve"
t_contains "$(cat "$T/calls/body-101")" "**Issues:** acme/widget#7, acme/widget#9"

t_case "without a repository for pi-rukas its findings go with the rest"
ready
findings "$(rukas_finding "The driver drops a path")"
t_run "$improve"
t_eq "$(cat "$T/calls/filed-in")" "acme/factory"
t_lacks "$(t_calls gh)" "acme/rukas"
t_contains "$(cat "$T/calls/body-101")" "A cycle here cannot fix it and must not work around it: it changes no code"
t_lacks "$(cat "$T/calls/body-101")" "a cycle builds the fix"

t_case "a title open in either repository is not filed again"
for where in open rukas-open; do
  ready
  with_rukas
  stack 4 "the driver drops a path" | jq -s . >"$fx/$where.json"
  findings "$(rukas_finding "The driver drops a path")"
  t_run "$improve"
  t_eq "$(titles)" ""
  t_contains "$t_out" "duplicates=1"
done

t_case "a duplicate may be named in the repository for pi-rukas"
ready
with_rukas
stack 12 "Safety net loses a file" | jq -s . >"$fx/rukas-open.json"
findings "$(finding "The driver drops a path" '{"category":"pi-rukas","duplicate_of":12}')"
t_run "$improve"
t_eq "$(titles)" ""
t_contains "$t_out" "duplicates=1 of=#12"

t_case "the analyst is told which repository each issue of the stack is in"
ready
with_rukas
stack 4 "Pause and resume" | jq -s . >"$fx/open.json"
stack 12 "Safety net loses a file" | jq -s . >"$fx/rukas-open.json"
t_run "$improve"
t_eq "$(jq -c 'map([.repo, .number])' "$T/calls/issues.json")" '[["acme/factory",4],["acme/rukas",12]]'

full() { for i in $(seq 1 "$1"); do stack "$i" "$2 $i" OPEN tehas:improvement; done | jq -s .; }

t_case "each repository has the ten to itself: one that is full does not hold the other back"
ready
with_rukas
full 10 Finding >"$fx/open.json"
findings "$(finding "The gate runs twice")" "$(rukas_finding "The driver drops a path")"
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/filed-in")" "acme/rukas"
t_contains "$t_out" "over=1 filed=1 issues=acme/rukas#101"

t_case "the repository for pi-rukas is full: its findings are over, the rest are filed"
ready
with_rukas
full 10 "Driver finding" >"$fx/rukas-open.json"
findings "$(rukas_finding "The driver drops a path")" "$(finding "The gate runs twice")"
t_run "$improve"
t_eq "$(cat "$T/calls/filed-in")" "acme/factory"
t_contains "$t_out" "over=1 filed=1 issues=#101"

t_case "eight open in the repository for pi-rukas leave room for two there"
ready
with_rukas
full 8 "Driver finding" >"$fx/rukas-open.json"
findings "$(rukas_finding One)" "$(rukas_finding Two)" "$(rukas_finding Three)"
t_run "$improve"
t_eq "$(titles | wc -l)" 2
t_contains "$t_out" "over=1"

t_case "at most three issues a run, whatever the repositories"
ready
with_rukas
findings "$(finding One)" "$(rukas_finding Two)" "$(finding Three)" "$(rukas_finding Four)"
t_run "$improve"
t_eq "$(titles | wc -l)" 3
t_contains "$t_out" "over=1 filed=3"

t_case "the instance sets the limit: twelve open and a limit of 30 leave room"
ready
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory TEHAS_IMPROVE_MAX_OPEN=30
full 12 Finding >"$fx/open.json"
t_run "$improve"
t_contains "$t_out" "filed=1"
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory TEHAS_IMPROVE_MAX_OPEN=12
record "$REC"
t_run "$improve"
t_contains "$t_out" "waiting: 12 open issues"

t_case "both repositories full: the run waits and the records stay"
ready
with_rukas
full 10 Finding >"$fx/open.json"
full 10 "Driver finding" >"$fx/rukas-open.json"
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "waiting: 10 open issues carry tehas:improvement in acme/factory acme/rukas"
t_eq "$(t_calls docker | grep -v '^info ')" ""
[ -f "$base/window/$REC/cycle.json" ] || t_bad "the record is gone"

t_case "GitHub does not give the stack of the repository for pi-rukas: nothing runs"
ready
with_rukas
: >"$fx/gh-rukas-list.rc"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(t_calls docker | grep -v '^info ')" ""
[ -f "$base/window/$REC/cycle.json" ] || t_bad "the record is gone"

t_case "a malformed repository for pi-rukas exits 78 before any call"
ready
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory "TEHAS_IMPROVE_RUKAS_REPO='not a repo'"
t_run "$improve"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)$(t_calls docker)" ""

# --- A boundary finding ------------------------------------------------------
boundary_finding() { finding "$1" '{"category":"boundary","factory_part":"Give the container a CPU quota.","rukas_part":"Size the gate from the cgroup."}'; }

t_case "a boundary finding is two issues, one a repository, each with its half, linked both ways"
ready
with_rukas
findings "$(boundary_finding "Gates time out under four cycles")"
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/filed-in")" "acme/factory
acme/rukas"
t_contains "$t_out" "filed=2 issues=#101,acme/rukas#102"
t_eq "$(t_calls gh | grep '^issue create' | grep -c -e '--label tehas:improvement --label cross-project')" 2
t_contains "$(t_calls gh)" "label create cross-project --repo acme/factory"
t_contains "$(t_calls gh)" "label create cross-project --repo acme/rukas"
first=$(cat "$T/calls/body-101")
second=$(cat "$T/calls/body-102")
t_contains "$first" "**Category:** boundary"
t_contains "$first" "This is the factory's half"
t_contains "$first" "**This repository's half**

Give the container a CPU quota."
t_contains "$first" "**The other half**, filed in acme/rukas"
t_contains "$second" "**This repository's half**

Size the gate from the cgroup."
t_contains "$second" "**The other half**, filed in acme/factory: https://github.com/acme/factory/issues/101"
t_contains "$(t_calls gh)" "issue comment 101 --repo acme/factory --body The half for pi-rukas is filed as https://github.com/acme/rukas/issues/102."

t_case "a boundary finding without both halves is invalid"
ready
with_rukas
findings "$(finding One '{"category":"boundary","factory_part":"x"}')" \
  "$(finding Two '{"category":"boundary","factory_part":"x","rukas_part":""}')" \
  "$(finding Three '{"category":"boundary","factory_part":"x","rukas_part":"a\u0007b"}')"
t_run "$improve"
t_contains "$t_out" "invalid=3"
t_eq "$(titles)" ""

t_case "a boundary finding needs room in both repositories, and counts as two of the three"
ready
with_rukas
full 10 "Driver finding" >"$fx/rukas-open.json"
findings "$(boundary_finding "Gates time out")"
t_run "$improve"
t_contains "$t_out" "over=1 filed=0"
ready
with_rukas
findings "$(finding One)" "$(boundary_finding Two)" "$(finding Three)"
t_run "$improve"
t_contains "$t_out" "over=1 filed=3 issues=#101,#102,acme/rukas#103"

t_case "the half for pi-rukas is refused: the factory's half says so, and the run fails"
ready
with_rukas
findings "$(boundary_finding "Gates time out")"
echo 102 >"$fx/gh-create.rc"
t_run "$improve"
t_eq "$t_rc" 1
t_contains "$t_out" "GitHub refused an issue; filed before that: #101"
t_contains "$(t_calls gh)" "issue comment 101 --repo acme/factory --body GitHub refused the half for pi-rukas, so it is not filed in acme/rukas"
t_lacks "$(t_calls gh)" "The half for pi-rukas is filed as"

t_case "without a repository for pi-rukas a boundary finding is one issue with both halves"
ready
findings "$(boundary_finding "Gates time out")"
t_run "$improve"
t_contains "$t_out" "filed=1 issues=#101"
body=$(cat "$T/calls/body-101")
t_contains "$body" "this repository is not pi-rukas"
t_contains "$body" "**The factory's half**

Give the container a CPU quota."
t_contains "$body" "**The half for pi-rukas**

Size the gate from the cgroup."
t_contains "$(t_calls gh)" "--label cross-project"

t_case "the halves of a finding of another category are left out"
ready
findings "$(finding One '{"factory_part":"stray","rukas_part":"stray"}')"
t_run "$improve"
t_contains "$t_out" "filed=1"
t_lacks "$(cat "$T/calls/body-101")" stray
t_lacks "$(t_calls gh)" cross-project

# --- An issue worked on again ------------------------------------------------
OLD=7-261009-100000
ticket() { # <number> [<cycles line>] [<label>] [<state>]: an issue the loop filed
  jq -n -c --argjson n "$1" --arg c "${2-$OLD}" --arg l "${3-tehas:improvement}" --arg s "${4:-OPEN}" '
    {number: $n, title: "Safety net loses a file", state: $s, labels: (if $l == "" then [] else [{name: $l}] end),
     body: "The driver drops a path.\n\n**Cycles:** \($c)\n\n**Whole logs**\n"}'
}
recheck() { # [<jq object to merge in>]
  local more=${1:-}
  [ -n "$more" ] || more='{}'
  jq -n -c --arg rec "$REC" --argjson more "$more" '
    {repo: "acme/rukas", number: 12, cycle: $rec, seen: false, note: "safety net committed 3 of 3 paths"} + $more'
}
rechecks() { printf '%s\n' "$@" | jq -s '{findings: [], followups: .}' >"$fx/findings.json"; }
again() { # issue 7 was worked on again, and acme/rukas#12 rests on its earlier cycle
  ready
  with_rukas
  jq -c '. + {rukas: "0123456789abcdef0123456789abcdef01234567"}' "$base/window/$REC/cycle.json" >"$fx/c.json"
  mv "$fx/c.json" "$base/window/$REC/cycle.json"
  ticket 12 | jq -s . >"$fx/rukas-open.json"
  rechecks "$(recheck)"
}
commented() { find "$T/calls" -name 'comment-*' | wc -l; }

t_case "an issue worked on again: what the new cycle shows is a comment on the filed issue"
again
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "filed=0 issues=none rechecked=1"
t_contains "$(t_calls gh)" "issue comment 12 --repo acme/rukas"
body=$(cat "$T/calls/comment-12")
t_contains "$body" "acme/widget#7 was worked on again: cycle \`$REC\`"
t_contains "$body" 'with pi-rukas at `0123456789abcdef0123456789abcdef01234567`.'
t_contains "$body" "**Seen again:** no"
t_contains "$body" "safety net committed 3 of 3 paths"
t_contains "$body" "https://github.com/acme/factory/releases/download/tehas-improve-logs/ticket-$REC.log.gz"
t_lacks "$(t_calls gh)" "issue close"
t_lacks "$(t_calls gh)" "issue reopen"

t_case "the analyst is given the cycles each filed issue rests on"
t_eq "$(jq -c '.[] | [.number, .cycles]' "$T/calls/issues.json")" "[12,[\"$OLD\"]]"

t_case "a problem seen again says so, on a closed issue too, in the loop's own repository too"
again
echo '[]' >"$fx/rukas-open.json"
ticket 4 "$OLD, 9-261009-110000" tehas:improvement CLOSED | jq -s . >"$fx/closed.json"
rechecks "$(recheck '{"repo":"acme/factory","number":4,"seen":true}')"
t_run "$improve"
t_contains "$(t_calls gh)" "issue comment 4 --repo acme/factory"
t_contains "$(cat "$T/calls/comment-4")" "**Seen again:** yes"

t_case "the comment names the image the cycle ran when the record has one"
again
img=sha256:$(printf 'e%.0s' {1..64})
jq -c --arg i "$img" '. + {image: $i}' "$base/window/$REC/cycle.json" >"$fx/c.json"
mv "$fx/c.json" "$base/window/$REC/cycle.json"
t_run "$improve"
t_contains "$(cat "$T/calls/comment-12")" "with pi-rukas at \`0123456789abcdef0123456789abcdef01234567\`, image \`$img\`."

t_case "a record that names no pi-rukas commit: the comment says so"
again
record "$REC"
t_run "$improve"
t_contains "$(cat "$T/calls/comment-12")" "a commit the record does not name"

t_case "an open issue is not checked for a fix"
again
t_run "$improve"
t_lacks "$(t_calls gh)" "api "
t_contains "$(cat "$T/calls/comment-12")" "**Seen again:** no"

FIX=abcdefabcdefabcdefabcdefabcdefabcdefabcd
closed_again() { # issue 12 of acme/rukas closed by $FIX, worked on again, seen
  again
  echo '[]' >"$fx/rukas-open.json"
  ticket 12 "$OLD" tehas:improvement CLOSED | jq -s . >"$fx/rukas-closed.json"
  printf '%s\n' - "$FIX" >"$fx/events"
  rechecks "$(recheck '{"seen":true}')"
}

t_case "seen again in a build without the fix: the comment says not in this build"
for st in behind diverged; do
  closed_again
  echo "$st" >"$fx/compare"
  t_run "$improve"
  t_contains "$(t_calls gh)" "api --paginate repos/acme/rukas/issues/12/events"
  t_contains "$(t_calls gh)" "api repos/acme/rukas/compare/$FIX...0123456789abcdef0123456789abcdef01234567"
  body=$(cat "$T/calls/comment-12")
  t_contains "$body" "**Seen again:** not in this build: the fix \`abcdefabcdef\` is not in \`0123456789ab\`"
  t_lacks "$body" "**Seen again:** yes"
done

t_case "seen again in a build with the fix: yes, and the comment says the fix was in it"
for st in ahead identical; do
  closed_again
  echo "$st" >"$fx/compare"
  t_run "$improve"
  t_contains "$(cat "$T/calls/comment-12")" "**Seen again:** yes, with the fix \`abcdefabcdef\` in this build"
done

t_case "the fix is checked against the runner's build commit when the record has one, not the label's"
closed_again
jq -c '. + {buildCommit: "1ca933e3cbbae6fff6f41d57973ba2af8ff805fd"}' "$base/window/$REC/cycle.json" >"$fx/c.json"
mv "$fx/c.json" "$base/window/$REC/cycle.json"
echo behind >"$fx/compare"
t_run "$improve"
t_contains "$(t_calls gh)" "api repos/acme/rukas/compare/$FIX...1ca933e3cbbae6fff6f41d57973ba2af8ff805fd"
t_lacks "$(t_calls gh)" "compare/$FIX...0123456789abcdef"
t_contains "$(cat "$T/calls/comment-12")" "with pi-rukas at \`1ca933e3cbbae6fff6f41d57973ba2af8ff805fd\`"
t_contains "$(cat "$T/calls/comment-12")" "not in this build: the fix \`abcdefabcdef\` is not in \`1ca933e3cbba\`"

t_case "not seen in a build without the fix says so"
closed_again
echo behind >"$fx/compare"
rechecks "$(recheck)"
t_run "$improve"
t_contains "$(cat "$T/calls/comment-12")" "**Seen again:** no, though the fix \`abcdefabcdef\` is not in \`0123456789ab\`"

t_case "a build that is not known, or no answer, says build unknown and never not in this build"
closed_again
record "$REC"
echo behind >"$fx/compare"
t_run "$improve"
t_lacks "$(t_calls gh)" "/compare/"
t_contains "$(cat "$T/calls/comment-12")" "**Seen again:** yes, build unknown: the record names no commit of acme/rukas"
closed_again
t_run "$improve"
t_contains "$(cat "$T/calls/comment-12")" "**Seen again:** yes, build unknown: GitHub did not say whether the fix"

t_case "an issue closed by hand has no fix, and one of the factory is never compared to pi-rukas"
closed_again
printf '%s\n' "$FIX" - >"$fx/events"
echo behind >"$fx/compare"
t_run "$improve"
t_lacks "$(t_calls gh)" "/compare/"
t_contains "$(cat "$T/calls/comment-12")" "**Seen again:** yes"
t_lacks "$(cat "$T/calls/comment-12")" "build"
again
echo '[]' >"$fx/rukas-open.json"
ticket 4 "$OLD" tehas:improvement CLOSED | jq -s . >"$fx/closed.json"
printf '%s\n' "$FIX" >"$fx/events"
echo behind >"$fx/compare"
rechecks "$(recheck '{"repo":"acme/factory","number":4,"seen":true}')"
t_run "$improve"
t_lacks "$(t_calls gh)" "/compare/"
t_contains "$(cat "$T/calls/comment-4")" "**Seen again:** yes, build unknown: the record names no commit of acme/factory"

t_case "no comment on an issue the loop did not file"
again
ticket 12 "$OLD" "" | jq -s . >"$fx/rukas-open.json"
t_run "$improve"
t_eq "$(commented)" 0
t_contains "$t_out" "rechecked=0"

t_case "no comment on a filed issue that rests on no cycle of the same product issue"
for c in "9-261009-100000" "$REC" "77-261009-100000" ""; do
  again
  ticket 12 "$c" | jq -s . >"$fx/rukas-open.json"
  t_run "$improve"
  t_eq "$(commented)" 0
done

t_case "rechecks of the wrong shape are dropped one by one"
again
rechecks "$(recheck '{"number":13}')" "$(recheck '{"repo":"acme/other"}')" "$(recheck '{"cycle":"7-261011-000000"}')" \
  "$(recheck '{"seen":"no"}')" "$(recheck '{"note":""}')" "$(recheck "{\"note\":\"$(printf 'x%.0s' $(seq 1 1501))\"}")" \
  "$(recheck '{"note":"a\u0007b"}')" '"a string"'
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(commented)" 0

t_case "followups that are not a list are no rechecks, and the findings are still filed"
again
jq -n --argjson f "$(finding "The gate runs twice")" '{findings: [$f], followups: "all of them"}' >"$fx/findings.json"
t_run "$improve"
t_contains "$t_out" "filed=1 issues=#101 rechecked=0"

t_case "the same recheck twice is one comment"
again
rechecks "$(recheck)" "$(recheck '{"note":"again"}')"
t_run "$improve"
t_eq "$(t_calls gh | grep -c '^issue comment ')" 1

t_case "at most ten rechecks a run"
again
echo '[]' >"$fx/rukas-open.json"
for i in $(seq 1 12); do ticket "$i" "$OLD" tehas:improvement CLOSED; done | jq -s . >"$fx/rukas-closed.json"
for i in $(seq 1 12); do recheck "{\"number\":$i}"; done | jq -s '{findings: [], followups: .}' >"$fx/findings.json"
t_run "$improve"
t_eq "$(t_calls gh | grep -c '^issue comment ')" 10

t_case "a mention in a recheck does not reach the comment"
again
rechecks "$(recheck '{"note":"thanks @alice <!-- hidden -->"}')"
t_run "$improve"
t_lacks "$(cat "$T/calls/comment-12")" "@alice"
t_contains "$(cat "$T/calls/comment-12")" "&lt;!-- hidden"

t_case "GitHub refuses the comment: the run finishes and the line says so"
again
: >"$fx/gh-comment.rc"
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "rechecked=0 not-commented=acme/rukas#12"
t_eq "$(find "$base" -mindepth 1 ! -name window | wc -l)" 0

t_case "a mention does not reach the body and a comment reaches it as text"
ready
findings "$(finding Mention '{"body":"Ask @alice. <!-- hidden --> And ```fence```.","evidence":"a ``` b"}')"
t_run "$improve"
body=$(cat "$T/calls/body-101")
t_lacks "$body" "@alice"
t_lacks "$body" "<!--"
t_contains "$body" "&lt;!-- hidden"
t_lacks "$body" 'a ``` b'

t_case "a log with the shape of a token is not uploaded and the issue says so"
ready
echo "ghp_$(printf 'a%.0s' $(seq 1 36))" >>"$home/logs/work-$REC.jsonl"
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/assets" 2>/dev/null)" ""
t_contains "$(cat "$T/calls/body-101")" "shaped like a token"
t_contains "$t_out" "secret-shape=$REC"

t_case "a log that is gone: the issue is filed and says so"
ready
rm "$home/logs/work-$REC.jsonl"
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$(cat "$T/calls/body-101")" "no longer on the host"

t_case "no permission to make a release: the issue is filed without the log"
ready
: >"$fx/gh-release.rc"
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$(cat "$T/calls/body-101")" "could not be uploaded"

t_case "a cycle named by two findings is uploaded once"
ready
findings "$(finding One)" "$(finding Two)"
t_run "$improve"
t_eq "$(wc -l <"$T/calls/assets")" 1

t_case "GitHub refuses the second issue: exit 1 and the records are kept"
ready
findings "$(finding One)" "$(finding Two)"
echo 102 >"$fx/gh-create.rc"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(titles)" "One"
[ -f "$base/window/$REC/cycle.json" ] || t_bad "the record is gone"

t_case "a run directory left by a killed run gives its records back"
reset
findings "$(finding "The gate runs twice in one cycle")"
mkdir -p "$base/run-261010-000000/in/cycles/$REC" "$base/run-261010-000000/out"
echo '{"issue":7}' >"$base/run-261010-000000/in/cycles/$REC/cycle.json"
echo '{"type":"agent_end"}' >"$home/logs/work-$REC.jsonl"
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "cycles=1"
t_eq "$(titles)" "The gate runs twice in one cycle"
t_eq "$(find "$base" -maxdepth 1 -name 'run-*' | wc -l)" 0

t_case "a directory in the window that is not a record is removed, not read"
ready
mkdir -p "$base/window/..odd name"
t_run "$improve"
t_contains "$t_out" "cycles=1"

t_case "a cycle that ends while the stack is fetched is read in this run, not lost"
ready
echo 8-261010-200150 >"$fx/late"
echo '{"type":"agent_end"}' >"$home/logs/work-8-261010-200150.jsonl"
findings "$(finding "The late cycle parked" '{"cycles":["8-261010-200150"]}')"
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "cycles=2 findings=1 invalid=0"
t_eq "$(titles)" "The late cycle parked"

t_case "the window is never renamed: a run moves its records out of it"
ready
ino=$(stat -c %i "$base/window")
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(stat -c %i "$base/window")" "$ino"

# --- How many records a run takes -------------------------------------------
cap() { cat "$home/state/improve.cap" 2>/dev/null || echo none; }
many() { # <record>...: a window with these records and no findings
  reset
  for r in "$@"; do record "$r"; done
  echo '{"findings": [], "followups": []}' >"$fx/findings.json"
}
taken() { t_calls cycles | tr '\n' ' '; }
left() { find "$base/window" -mindepth 1 -maxdepth 1 -printf '%f\n' | sort | tr '\n' ' '; }

t_case "a run takes the five oldest records, by time and not by issue, and leaves the rest"
many 9-261001-000000 1-261003-000000 2-261002-120000 30-261002-000000 4-261004-000000 5-261005-000000 6-261006-000000
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(taken)" "1-261003-000000 2-261002-120000 30-261002-000000 4-261004-000000 9-261001-000000 "
t_eq "$(left)" "5-261005-000000 6-261006-000000 "
t_contains "$t_out" "cycles=5 "
t_contains "$t_out" "left=2"

t_case "an analyst stopped at its bound halves what the next run takes"
many 1-261001-000000 2-261002-000000 3-261003-000000 4-261004-000000 5-261005-000000 6-261006-000000
echo 124 >"$fx/docker.rc"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(cap)" 2
t_contains "$t_out" "the next run takes at most 2"
t_eq "$(left)" "1-261001-000000 2-261002-000000 3-261003-000000 4-261004-000000 5-261005-000000 6-261006-000000 "
t_run "$improve"
t_eq "$(taken)" "1-261001-000000 2-261002-000000 "
t_eq "$(cap)" 1
t_run "$improve"
t_eq "$(cap)" 1

t_case "a client that needed the kill counts as stopped at the bound"
many 1-261001-000000 2-261002-000000 3-261003-000000 4-261004-000000
echo 137 >"$fx/docker.rc"
t_run "$improve"
t_eq "$(cap)" 2

t_case "half of what it took, not of the cap: three records stopped at the bound leave one"
many 1-261001-000000 2-261002-000000 3-261003-000000
echo 124 >"$fx/docker.rc"
t_run "$improve"
t_eq "$(cap)" 1

t_case "the analyst is told its time and to write its file early"
ready
t_run "$improve"
p=$(sed -n '/^--model$/,$p' "$T/calls/docker-args" | sed 1,2d)
t_contains "$p" "You have 60 minutes"
t_contains "$p" "Write it early"
t_lacks "$p" "@MINUTES@"

t_case "stopped at its bound with a good file: what it holds is filed, and only the records it names are done"
many 1-261001-000000 2-261002-000000 "$REC"
findings "$(finding "The gate runs twice in one cycle")"
echo 124 >"$fx/docker.cut"
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(titles)" "The gate runs twice in one cycle"
t_eq "$(left)" "1-261001-000000 2-261002-000000 "
t_contains "$t_out" "cycles=3 findings=1"
t_contains "$t_out" "stopped=124 returned=2"
t_eq "$(cap)" 1
kept=$(find "$home/logs" -name 'improve-analyst-*.log')
[ -n "$kept" ] || t_bad "the output of a stopped analyst was not kept"
t_contains "$t_out" "output=$kept"

t_case "a record named only by a followup is done too"
many 1-261001-000000 "$REC"
jq -n -c --arg rec "$REC" '{findings: [{title: 1, cycles: ["1-261001-000000", 7]}],
  followups: [{cycle: $rec}, "x"]}' >"$fx/findings.json"
echo 124 >"$fx/docker.cut"
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(left)" ""
t_contains "$t_out" "invalid=1"
t_contains "$t_out" "returned=0"

t_case "stopped at its bound with nothing found yet: every record goes back"
many 1-261001-000000 "$REC"
echo 124 >"$fx/docker.cut"
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(left)" "1-261001-000000 $REC "
t_contains "$t_out" "returned=2"

t_case "stopped at its bound with a file of the wrong shape: exit 1 and every record goes back"
many 1-261001-000000 "$REC"
echo '{"findings": [' >"$fx/findings.json"
echo 124 >"$fx/docker.cut"
t_run "$improve"
t_eq "$t_rc" 1
t_contains "$t_out" "at its bound of 60 minutes; the next run takes at most 1; findings.json is not the shape asked for"
t_eq "$(left)" "1-261001-000000 $REC "
t_eq "$(titles)" ""

t_case "stopped at its bound with no file: exit 1 and every record goes back"
many 1-261001-000000 "$REC"
rm "$fx/findings.json"
echo 124 >"$fx/docker.cut"
t_run "$improve"
t_eq "$t_rc" 1
t_contains "$t_out" "the next run takes at most 1; the analyst left no findings.json"
t_eq "$(left)" "1-261001-000000 $REC "

t_case "an analyst that fails otherwise is not read, whatever file it left"
many "$REC"
findings "$(finding "The gate runs twice in one cycle")"
echo 1 >"$fx/docker.cut"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(titles)" ""
t_eq "$(left)" "$REC "

t_case "a run that finishes in time doubles what the next takes, up to five"
many 1-261001-000000 2-261002-000000 3-261003-000000 4-261004-000000 5-261005-000000 6-261006-000000 7-261007-000000
mkdir -p "$home/state" && echo 1 >"$home/state/improve.cap"
t_run "$improve"
t_eq "$(taken)" "1-261001-000000 "
t_eq "$(cap)" 2
t_run "$improve"
t_eq "$(taken)" "2-261002-000000 3-261003-000000 "
t_eq "$(cap)" 4
t_run "$improve"
t_eq "$(cap)" 5

t_case "an analyst that fails for another reason leaves the cap as it was"
many 1-261001-000000 2-261002-000000 3-261003-000000 4-261004-000000
mkdir -p "$home/state" && echo 4 >"$home/state/improve.cap"
echo 3 >"$fx/docker.rc"
t_run "$improve"
t_eq "$(cap)" 4

t_case "a cap that is not a number from 1 to 5 is taken as 5"
for bad in 0 6 x "" "3; rm -rf /" 12; do
  many 1-261001-000000 2-261002-000000 3-261003-000000 4-261004-000000 5-261005-000000 6-261006-000000
  mkdir -p "$home/state" && printf '%s\n' "$bad" >"$home/state/improve.cap"
  t_run "$improve"
  t_eq "$(t_calls cycles | wc -l)" 5
done

t_case "a container left by a killed run is removed before the analyst starts"
ready
t_run "$improve"
t_eq "$(t_calls docker | grep -v '^info ' | head -n 1)" "rm -f tehas-improve"
t_contains "$(t_calls timeout | grep -v ' systemctl \| docker info ' | head -n 1)" "-k 60 3600 docker run"

t_case "an analyst stopped at its bound has its container removed at once"
ready
echo 124 >"$fx/docker.rc"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(t_calls docker | grep -E '^(run|rm) ' | cut -c1-6 | tr '\n' ' ')" "rm -f  run -- rm -f  "

t_case "an analyst that finished leaves no container to remove"
ready
t_run "$improve"
t_eq "$(t_calls docker | grep -c '^rm -f tehas-improve$')" 1

t_case "the sweep removes what is over seven days old and nothing newer"
reset
record 3-261001-000000
touch -d '8 days ago' "$base/window/3-261001-000000" "$home/logs/work-3-261001-000000.jsonl"
for f in work-3-261001-000000.jsonl.err dispatch-work-3.log index-261001-000000.log dispatch.log improve.log \
  improve-analyst-261001-000000.log improve-analyst.log; do
  touch -d '8 days ago' "$home/logs/$f"
done
touch -d '6 days ago' "$home/logs/work-4-261004-000000.jsonl" "$home/logs/improve-analyst-261004-000000.log"
mkdir -p "$base/.new.old" && touch -d '8 days ago' "$base/.new.old"
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "cycles=0"
t_eq "$(find "$home/logs" -type f -printf '%f\n' | sort | tr '\n' ' ')" "dispatch.log improve-analyst-261004-000000.log improve.log work-4-261004-000000.jsonl "
t_eq "$(find "$base" -mindepth 1 ! -name window | wc -l)" 0

t_case "the analyst's container is not given the run's lock"
ready
t_run "$improve"
t_eq "$(find "$T/calls/docker-args" | wc -l)" 1
t_eq "$(find "$T/calls/docker-lock" 2>/dev/null | wc -l)" 0

t_case "a second run while one is going does nothing"
ready
mkdir -p "$home/state"
before=$t_failed
(
  flock -n 9 || exit 1
  t_run "$improve"
  t_eq "$t_rc" 0
  t_contains "$t_out" "another run"
  t_eq "$(t_calls docker)" ""
  [ "$t_failed" = "$before" ]
) 9>"$home/state/improve.lock" || t_bad "the run did not stand back"

t_done
