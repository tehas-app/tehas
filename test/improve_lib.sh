# shellcheck shell=bash
# shellcheck disable=SC2016,SC2034  # stub bodies and snippets are expanded when they run; the parts read the variables
# What the tests of the improvement loop share: test/improve_*_test.sh source
# this. A run of libexec/improve goes against a stub docker and a stub gh.
# The part has sourced test/lib.sh before this.

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
  mkdir -p "$T/home/.pi/agent"
  echo '{"providers":{"test-provider":{"baseUrl":"http://models.test/v1","apiKey":"unused"}}}' >"$T/home/.pi/agent/models.json"
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

# --- A run -------------------------------------------------------------------
# The script as a host has it, beside the console. The stub masks what the
# tests call a secret; $T/fx/console.rc makes it fail.
tree=$T/tree
mkdir -p "$tree/libexec"
ln -s "$T_ROOT/lib" "$tree/lib"
ln -s "$T_ROOT/libexec/improve" "$tree/libexec/improve"
console_stub() {
  t_stub console '[ "$*" = --mask ] || exit 2
[ -f "$T/fx/console.rc" ] && exit 1
sed "s/hunter2-secret/***/g"'
  mv "$T/bin/console" "$tree/libexec/console"
}
console_stub
improve="$tree/libexec/improve"

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
    for a in "$@"; do case "$a" in *.gz) basename "$a" >>"$T/calls/assets"; zcat "$a" >>"$T/calls/uploaded" ;; esac; done ;;
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
    {title: $t, category: "cycle", body: "The gate ran twice.\n\n## Done when\n\n- test/gate_test.sh covers the second run.",
     evidence: "gate: ok",
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

# A repository for pi-rukas.
with_rukas() { t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory TEHAS_IMPROVE_RUKAS_REPO=acme/rukas; }
