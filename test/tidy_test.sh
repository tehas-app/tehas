#!/usr/bin/env bash
# Tests for libexec/tidy against a stub gh: which labels come off a closed
# issue, which never do, what an open issue keeps, tehas:shipped on the issue a
# merged pull request closed, the one search and its timeout, the dry run and
# the settings.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

tidy="$T_ROOT/libexec/tidy"
fx=$T/fx
mkdir -p "$fx"
# shellcheck disable=SC1083  # the braces of the JSON answers below are literal
t_stub gh 'case "$1 $2" in
  "issue list")
    [ ! -e "$T/fx/search-fails" ] || exit 1
    [ ! -e "$T/fx/search-hangs" ] || exec sleep 20
    cat "$T/fx/closed.json" ;;
  "issue edit") [ "$3" != "$(cat "$T/fx/edit-fails-on" 2>/dev/null)" ] || exit 1 ;;
  "api graphql")
    [ ! -e "$T/fx/graphql-fails" ] || exit 1
    [ ! -e "$T/fx/graphql-hangs" ] || exec sleep 20
    # The query names the issue, in the argv as the caller wrote it. The
    # answer is the file for that issue, the one every issue shares, or a
    # closer that closed nothing. t_stub writes this body with printf, so a
    # percent sign or a backslash here would be eaten on the way: the number
    # after number: is split onto its own word and cut, with no escape at all.
    n=$(echo "$*" | tr " " "\n" | grep "^number:" | cut -d: -f2)
    if [ -e "$T/fx/closer-$n.json" ]; then
      cat "$T/fx/closer-$n.json"
    elif [ -e "$T/fx/closer.json" ]; then
      cat "$T/fx/closer.json"
    else
      # Nothing closed it, as far as this answer goes.
      echo '{"data":{"search":{"nodes":[]}}}'
    fi ;;
esac'
reset() {
  rm -f "$T"/calls/* "$fx"/*
  rm -rf "$T/home/tehas/state"
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
  echo '[]' >"$fx/closed.json"
}
edits() { t_calls gh | grep '^issue edit' || true; }
# What closed the issue: merged, unmerged, by a person, or a closer that is
# not a pull request at all. The whole answer is built from one template so
# that the closer is the only part that changes; t_stub and the redirection
# take the text as it is, so no escape here can be eaten on the way.
closer_answer() { echo '{"data":{"search":{"nodes":[{"timelineItems":{"nodes":[{"closer":'"$1"'}]}}]}}}'; }
merged() { closer_answer '{"merged":true}' >"$fx/closer-$1.json"; }
unmerged() { closer_answer '{"merged":false}' >"$fx/closer-$1.json"; }
by_hand() { closer_answer 'null' >"$fx/closer-$1.json"; }
closed_by() { closer_answer "$1" >"$fx/closer.json"; }

t_case "a closed issue loses tehas:awaiting-merge, and keeps needs-human-attention and tehas:shipped"
reset
echo '[{"number":194,"state":"CLOSED","labels":[{"name":"tehas:awaiting-merge"},{"name":"needs-human-attention"},{"name":"tehas:shipped"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" "issue edit 194 --repo acme/widget --remove-label tehas:awaiting-merge"
t_contains "$t_out" "tidy #194: removed tehas:awaiting-merge"
t_lacks "$t_out" "needs-human-attention"
t_eq "$(t_calls gh | grep -c 'issue edit 194.*--add-label')" 0
t_lacks "$t_out" "added tehas:shipped"
t_lacks "$t_out" "could not read what closed"

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

t_case "the search names only the state labels, never tehas:shipped or a person's labels"
reset
echo '[]' >"$fx/closed.json"
t_run "$tidy"
search=$(t_calls gh | grep '^issue list')
t_contains "$search" '--search label:"tehas:running","tehas:awaiting-merge","tehas:question","tehas:planning","tehas:refreshing","tehas:blocked","tehas:ready","tehas:plan"'
for l in tehas:shipped needs-human-attention tehas:urgent tehas:next tehas:prototype cross-project tehas:pause; do
  t_lacks "$search" "\"$l\""
done

t_case "one search a poll, for the closed issues of the repository"
t_eq "$(t_calls gh | grep -c '^issue list')" 1
t_contains "$search" "--repo acme/widget --state closed"
t_eq "$t_rc" 0
# The search above asked for no issue, so the poll made no call about a closer.
t_eq "$(t_calls gh | grep -c '^api graphql')" 0
t_eq "$t_out" ""

t_case "an issue that already carries tehas:shipped gets no second add, and no read"
reset
echo '[{"number":44,"state":"CLOSED","labels":[{"name":"tehas:awaiting-merge"},{"name":"tehas:shipped"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" "issue edit 44 --repo acme/widget --remove-label tehas:awaiting-merge"
t_eq "$(t_calls gh | grep -c '^api graphql')" 0
t_lacks "$t_out" "added tehas:shipped"
t_lacks "$t_out" "could not read what closed"

t_case "an open issue is never touched, whatever the answer says"
reset
merged 8
merged 9
echo '[{"number":8,"state":"OPEN","labels":[{"name":"tehas:awaiting-merge"}]},{"number":9,"labels":[{"name":"tehas:question"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" ""
t_eq "$(t_calls gh | grep -c '^api graphql')" 0
t_eq "$t_out" ""

t_case "a closed issue with no state label is not edited"
reset
merged 10
echo '[{"number":10,"state":"CLOSED","labels":[{"name":"tehas:shipped"},{"name":"needs-human-attention"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" ""
t_eq "$(t_calls gh | grep -c '^api graphql')" 0

t_case "a closed issue whose cycle still holds its lock waits for the cycle"
reset
merged 11
merged 12
echo '[{"number":11,"state":"CLOSED","labels":[{"name":"tehas:running"}]},{"number":12,"state":"CLOSED","labels":[{"name":"tehas:running"}]}]' >"$fx/closed.json"
mkdir -p "$T/home/tehas/state/cycles"
exec 9>"$T/home/tehas/state/cycles/11.lock"
flock -n 9
t_run "$tidy"
exec 9>&-
t_eq "$t_rc" 0
t_eq "$(edits)" "issue edit 12 --repo acme/widget --remove-label tehas:running --add-label tehas:shipped"
t_lacks "$t_out" "#11"
t_eq "$(t_calls gh | grep -c '^api graphql')" 1

t_case "the labels come off an issue the search listed without tehas:shipped, when the closer was merged"
reset
closed_by '{"merged":true}'
echo '[{"number":13,"state":"CLOSED","labels":[{"name":"tehas:running"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" "issue edit 13 --repo acme/widget --remove-label tehas:running --add-label tehas:shipped"
t_contains "$(t_calls gh)" "q=repo:acme/widget is:issue is:closed number:13"

t_case "a dry run removes nothing and says what it would remove"
reset
merged 194
echo '[{"number":194,"state":"CLOSED","labels":[{"name":"tehas:awaiting-merge"},{"name":"tehas:question"}]}]' >"$fx/closed.json"
t_run "$tidy --dry-run"
t_eq "$t_rc" 0
t_eq "$(edits)" ""
t_eq "$(t_calls gh | grep -c '^issue list')" 1
t_contains "$t_out" "tidy #194: would remove tehas:awaiting-merge tehas:question"
t_contains "$t_out" "tidy #194: would add tehas:shipped"
t_lacks "$t_out" "removed tehas:awaiting-merge"

t_case "a dry run of an issue whose closer cannot be read says nothing about the label"
reset
echo '[{"number":194,"state":"CLOSED","labels":[{"name":"tehas:awaiting-merge"}]}]' >"$fx/closed.json"
t_run "$tidy --dry-run"
t_eq "$t_rc" 0
t_eq "$(edits)" ""
t_contains "$t_out" "tidy #194: would remove tehas:awaiting-merge"
t_lacks "$t_out" "would add tehas:shipped"
t_contains "$t_out" "WARN: tidy #194: could not read what closed the issue, tehas:shipped was not added"

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

t_case "an issue a merged pull request closed gets tehas:shipped with its removals"
reset
merged 30
unmerged 31
by_hand 32
echo '[{"number":30,"state":"CLOSED","labels":[{"name":"tehas:awaiting-merge"}]},
  {"number":31,"state":"CLOSED","labels":[{"name":"tehas:running"}]},
  {"number":32,"state":"CLOSED","labels":[{"name":"tehas:ready"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits | wc -l)" 3
t_eq "$(edits)" "issue edit 30 --repo acme/widget --remove-label tehas:awaiting-merge --add-label tehas:shipped
issue edit 31 --repo acme/widget --remove-label tehas:running
issue edit 32 --repo acme/widget --remove-label tehas:ready"
t_contains "$t_out" "tidy #30: added tehas:shipped"
t_eq "$(t_calls gh | grep '^issue edit' | grep -c -- '--add-label tehas:shipped')" 1
t_eq "$(printf '%s\n' "$t_out" | grep -c 'could not read what closed')" 2

t_case "the answer of the closer read is the one of that issue, not of another"
reset
merged 30
unmerged 31
echo '[{"number":30,"state":"CLOSED","labels":[{"name":"tehas:awaiting-merge"}]},
  {"number":31,"state":"CLOSED","labels":[{"name":"tehas:awaiting-merge"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits | grep -c -- '--add-label tehas:shipped')" 1
t_eq "$(t_calls gh | grep -c '^api graphql')" 2

t_case "the label is still given when the search forgot to list it, so a race adds it once"
reset
merged 33
echo '[{"number":33,"state":"CLOSED","labels":[{"name":"tehas:awaiting-merge"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" "issue edit 33 --repo acme/widget --remove-label tehas:awaiting-merge --add-label tehas:shipped"
t_contains "$t_out" "tidy #33: added tehas:shipped"

t_case "a GraphQL answer that cannot be read still takes the state labels off, and adds nothing"
reset
: >"$fx/graphql-fails"
echo '[{"number":40,"state":"CLOSED","labels":[{"name":"tehas:question"}]},
  {"number":41,"state":"CLOSED","labels":[{"name":"tehas:plan"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" "issue edit 40 --repo acme/widget --remove-label tehas:question
issue edit 41 --repo acme/widget --remove-label tehas:plan"
t_contains "$t_out" "tidy #40: removed tehas:question"
t_contains "$t_out" "WARN: tidy #40: could not read what closed the issue, tehas:shipped was not added"
t_lacks "$t_out" "added tehas:shipped"
t_eq "$(t_calls gh | grep -c 'add-label')" 0
t_case "a GraphQL answer that hangs is cut off, the labels still come off, and the poll goes on"
reset
: >"$fx/graphql-hangs"
echo '[{"number":42,"state":"CLOSED","labels":[{"name":"tehas:question"}]}]' >"$fx/closed.json"
T_ENV=(TEHAS_TIDY_WAIT=1)
start=$(date +%s)
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(($(date +%s) - start < 15))" 1
t_eq "$(edits)" "issue edit 42 --repo acme/widget --remove-label tehas:question"
t_contains "$t_out" "tidy #42: removed tehas:question"
t_contains "$t_out" "WARN: tidy #42: could not read what closed the issue, tehas:shipped was not added"

t_case "an edit that fails is logged and the next issue is still tidied"
reset
merged 20
merged 21
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
