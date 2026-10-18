#!/usr/bin/env bash
# Tests for libexec/tidy against a stub gh: which labels come off a closed
# issue, which never do, what an open issue keeps, the one search and its
# timeout, the dry run and the settings.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

tidy="$T_ROOT/libexec/tidy"
fx=$T/fx
mkdir -p "$fx"
t_stub gh 'case "$1 $2" in
  "issue list")
    [ ! -e "$T/fx/search-fails" ] || exit 1
    [ ! -e "$T/fx/search-hangs" ] || exec sleep 20
    cat "$T/fx/closed.json" ;;
  "issue edit") [ "$3" != "$(cat "$T/fx/edit-fails-on" 2>/dev/null)" ] || exit 1 ;;
esac'
reset() {
  rm -f "$T"/calls/* "$fx"/*
  rm -rf "$T/home/tehas/state"
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
  echo '[]' >"$fx/closed.json"
}
edits() { t_calls gh | grep '^issue edit' || true; }

t_case "a closed issue loses tehas:awaiting-merge, and keeps needs-human-attention and tehas:shipped"
reset
echo '[{"number":194,"state":"CLOSED","labels":[{"name":"tehas:awaiting-merge"},{"name":"needs-human-attention"},{"name":"tehas:shipped"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" "issue edit 194 --repo acme/widget --remove-label tehas:awaiting-merge"
t_contains "$t_out" "tidy #194: removed tehas:awaiting-merge"
t_lacks "$t_out" "needs-human-attention"

t_case "every state label comes off a closed issue in one edit, and the labels a person chose stay"
reset
echo '[{"number":5,"state":"CLOSED","labels":[{"name":"tehas:running"},{"name":"tehas:question"},{"name":"tehas:planning"},
  {"name":"tehas:refreshing"},{"name":"tehas:blocked"},{"name":"tehas:ready"},{"name":"tehas:plan"},{"name":"tehas:urgent"},
  {"name":"tehas:next"},{"name":"tehas:prototype"},{"name":"cross-project"},{"name":"bug"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits | wc -l)" 1
for l in tehas:running tehas:question tehas:planning tehas:refreshing tehas:blocked tehas:ready tehas:plan; do
  t_contains "$(edits)" "--remove-label $l"
done
for l in tehas:urgent tehas:next tehas:prototype cross-project bug; do
  t_lacks "$(edits) " "--remove-label $l "
done
t_contains "$t_out" "tidy #5: removed tehas:running tehas:question tehas:planning tehas:refreshing tehas:blocked tehas:ready tehas:plan"

t_case "the list of state labels is the one in the issue, nothing more"
reset
t_run "$tidy"
search=$(t_calls gh | grep '^issue list')
t_contains "$search" '--search label:"tehas:running","tehas:awaiting-merge","tehas:question","tehas:planning","tehas:refreshing","tehas:blocked","tehas:ready","tehas:plan"'
for l in tehas:shipped needs-human-attention tehas:urgent tehas:next tehas:prototype cross-project tehas:pause; do
  t_lacks "$search" "\"$l\""
done

t_case "one search, for closed issues of the repository"
t_eq "$(t_calls gh | grep -c '^issue list')" 1
t_contains "$search" "--repo acme/widget --state closed"
t_eq "$t_rc" 0
t_eq "$t_out" ""

t_case "an open issue is never touched, whatever the answer says"
reset
echo '[{"number":8,"state":"OPEN","labels":[{"name":"tehas:awaiting-merge"}]},{"number":9,"labels":[{"name":"tehas:question"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" ""
t_eq "$t_out" ""

t_case "a closed issue with no state label is not edited"
reset
echo '[{"number":10,"state":"CLOSED","labels":[{"name":"tehas:shipped"},{"name":"needs-human-attention"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$(edits)" ""

t_case "a closed issue whose cycle still holds its lock waits for the cycle"
reset
echo '[{"number":11,"state":"CLOSED","labels":[{"name":"tehas:running"}]},{"number":12,"state":"CLOSED","labels":[{"name":"tehas:running"}]}]' >"$fx/closed.json"
mkdir -p "$T/home/tehas/state/cycles"
exec 9>"$T/home/tehas/state/cycles/11.lock"
flock -n 9
t_run "$tidy"
exec 9>&-
t_eq "$t_rc" 0
t_eq "$(edits)" "issue edit 12 --repo acme/widget --remove-label tehas:running"
t_lacks "$t_out" "#11"

t_case "a dry run removes nothing and says what it would remove"
reset
echo '[{"number":194,"state":"CLOSED","labels":[{"name":"tehas:awaiting-merge"},{"name":"tehas:question"}]}]' >"$fx/closed.json"
t_run "$tidy --dry-run"
t_eq "$t_rc" 0
t_eq "$(edits)" ""
t_eq "$(t_calls gh | grep -c '^issue list')" 1
t_contains "$t_out" "tidy #194: would remove tehas:awaiting-merge tehas:question"

t_case "a failed search is logged, nothing is edited, exit 1"
reset
: >"$fx/search-fails"
t_run "$tidy"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: tidy: the search for closed issues with state labels failed"
t_eq "$(edits)" ""

t_case "an answer that cannot be read is logged, exit 1"
reset
echo 'not json' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: tidy: the search for closed issues with state labels failed"

t_case "a search that hangs is cut off at TEHAS_TIDY_WAIT seconds"
reset
: >"$fx/search-hangs"
T_ENV=(TEHAS_TIDY_WAIT=1)
start=$(date +%s)
t_run "$tidy"
t_eq "$t_rc" 1
t_eq "$(($(date +%s) - start < 10))" 1
t_contains "$t_out" "WARN: tidy: the search for closed issues with state labels failed"

t_case "an edit that fails is logged and the next issue is still tidied"
reset
echo '[{"number":20,"state":"CLOSED","labels":[{"name":"tehas:question"}]},{"number":21,"state":"CLOSED","labels":[{"name":"tehas:ready"}]}]' >"$fx/closed.json"
echo 20 >"$fx/edit-fails-on"
t_run "$tidy"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: tidy #20: could not remove tehas:question"
t_contains "$t_out" "tidy #21: removed tehas:ready"
t_eq "$(edits | wc -l)" 2

t_case "without the repository nothing is called, exit 78"
reset
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$tidy"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_REPO is not set"
t_eq "$(t_calls gh)" ""

t_case "an unknown argument is refused before the settings are read, exit 64"
reset
t_instance
t_run "$tidy --dryrun"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas tidy: unknown argument: --dryrun"
t_eq "$(t_calls gh)" ""
t_run "$tidy --dry-run --help"
t_eq "$t_rc" 64

t_case "--help prints the usage and calls nothing"
reset
t_instance
t_run "$tidy --help"
t_eq "$t_rc" 0
t_contains "$t_out" "Usage:"
t_eq "$(t_calls gh)" ""

t_done
