#!/usr/bin/env bash
# Tests for libexec/tidy against a stub gh: which labels come off a closed
# issue, which never do, what an open issue keeps, tehas:shipped on the issue a
# merged pull request closed, the one search and the one closer read and their
# timeouts, the dry run and the settings.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

# The command as a host has it: beside the console, which decides what the read
# of what closed an issue means (console/src/shipped.rs) and is this checkout's
# own build. A tidy with no console beside it is a different case, further down.
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_console || t_bad "no console: $t_console_why"
shim=$T/tree
mkdir -p "$shim/libexec"
ln -s "$T_ROOT/libexec/tidy" "$shim/libexec/tidy"
ln -s "$T_ROOT/lib" "$shim/lib"
cp "$T_CONSOLE" "$shim/libexec/console"
tidy=$shim/libexec/tidy
# The line of a read that answered and named no merged closer, and the one of a
# read that failed or timed out: the two must read differently, or a rate-limited
# search looks like a factory that shipped nothing.
no_closer() { printf 'no merged pull request was read as the closer'; }
read_failed_line() { printf 'WARN: tidy: the read of what closed these issues failed, tehas:shipped was not added for %s issues\n' "$1"; }
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
    # One call names every issue it asks about, as number:<n> terms of the
    # search, and the answer holds one node per issue. The number of each
    # closer file is the number it answers for, so the whole answer is built
    # here from the terms the caller sent. A name that has no file closes the
    # call: the stub would answer nothing about an issue the caller named, and
    # a caller that reads no answer adds no label, so the case would pass for
    # the wrong reason. NO_NODE makes it answer a whole search that never
    # speaks of the issues it was asked about, as a lagged index does.
    # The call is matched on "$*", the arguments joined into one word, so the
    # number has to be split out of the word that carries it.
    terms=""
    for a in "$@"; do case $a in q=*) terms=${a#q=} ;; esac; done
    if [ -e "$T/fx/no-node" ]; then
      echo '{"data":{"search":{"nodes":[]}}}'
      exit 0
    fi
    nodes=""
    for a in $terms; do
      case $a in
        number:*)
          n=${a#number:}
          if [ ! -e "$T/fx/closer-$n.json" ]; then
            echo "no closer file for issue $n" >&2
            exit 1
          fi
          [ -z "$nodes" ] || nodes=$nodes,
          nodes=$nodes$(cat "$T/fx/closer-$n.json") ;;
      esac
    done
    if [ -e "$T/fx/answer-errors" ]; then
      # A query GitHub declined and answered anyway: errors beside the data.
      echo "{\"errors\":[{\"message\":\"declined\"}],$(cat "$T/fx/answer-errors")}"
    else
      echo "{\"data\":{\"search\":{\"nodes\":[$nodes]}}}"
    fi ;;
esac'
reset() {
  rm -f "$T"/calls/* "$fx"/*
  rm -rf "$T/home/tehas/state"
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
}
edits() { t_calls gh | grep '^issue edit' || true; }
reads() { t_calls gh | grep '^api graphql' || true; }
# The ceiling of one closer read, from the console through the library, so the
# case cannot ask for a number the query the same console prints would not answer
# for.
tehas_page_count() {
  "$T_CONSOLE" shipped page
}
# The names a poll asks GitHub for, read out of the one search of the last run.
# Parsed rather than retyped: a case that wrote the list out again would pass on
# a search that named a label the console never printed. The last grep closes the
# pipe early, so pipefail of the library is off for the read.
search_labels() {
  (
    set +o pipefail
    t_calls gh | grep '^issue list' | sed -n 's/.* --search label:\(.*\) --limit .*/\1/p' |
      tr -d '"' | tr ',' '\n' | grep .
  ) || true
}
# The console holds the rule of the poll, so a command of the platform reads it
# before it calls GitHub, and a console of an older tehas that says nothing is said
# then. The cases from here to "one search a poll" put the console beside the
# command under test: the shapes of a bad one, and of none beside the command at
# all. The next case brings the console of this checkout back, so every case after
# it runs against the real program and no answer of a stub stands in for it.
#
# The answers of the stub are files under the scratch of the case, named by the
# case, and not words of the environment: tehas_load of lib/config.sh drops every
# TEHAS_* name it does not know, so a setting of a stub would be gone before the
# first call of the poll.
stub_console() { # [good|empty|one-state|no-read]: the console beside the command
  # answers the words a poll reads of it, each from a file of the case named below.
  # The one argument names the shape of the list `tidy states` prints and of the
  # two words the read of a merge is asked for: `empty` is a console that prints no
  # name at all, `one-state` one that prints the nine state labels and no label of
  # a merge after them, and `no-read` one that gives no page and no label of a
  # merge, as a console of an older tehas does. Anything else is the good console.
  # The names are read out of the console of this checkout, so a case never writes
  # them out and cannot drift from the rule. A reset of a case takes the files
  # away, so every case calls this after its reset and the poll sees the console of
  # that case alone.
  local states_file=$T/fx/states.txt shape=${1-good} names
  mkdir -p "$T/fx"
  # The list is read whole into one name and written by one printf, so the file of the
  # case is written once and by the one command that holds the list.
  names=$("$T_CONSOLE" tidy states) || { t_bad "the console printed no state labels"; return 1; }
  if [ "$shape" = empty ]; then
    names=
  elif [ "$shape" = one-state ]; then
    # The nine state labels and no label of a merge after them. The caller reads the
    # last name of the list as that label, so this console has lost the rule of what
    # it may never take off: the poll spares the last name and searches for the rest.
    names=${names%$'\n'*}
  fi
  printf '%s\n' "$names" >"$states_file"
  cat >"$shim/libexec/console" <<STUB
#!/usr/bin/env bash
fx=\$T/fx
case "\$1 \$2" in
  "tidy states") [ -e "\$fx/states.txt" ] || exit 1
    cat "\$fx/states.txt" ;;
  "tidy plan") cat "\$fx/plan.txt" ;;
  "shipped page") [ -e "\$fx/no-read" ] && exit 1
    cat "\$fx/page.txt" ;;
  "shipped label") [ -e "\$fx/no-read" ] && exit 1
    cat "\$fx/label.txt" ;;
esac
exit 0
STUB
  chmod +x "$shim/libexec/console"
  printf '100\n' >"$fx/page.txt"
  # The library's own name for the label of a merge, which the console of this
  # checkout prints too: the case reads it out of lib/shipped.sh rather than
  # writing the word out again.
  printf '%s\n' "$(. "$T_ROOT/lib/shipped.sh" && printf '%s' "$tehas_shipped_label")" >"$fx/label.txt"
  : >"$fx/plan.txt"
  [ "$shape" = no-read ] && : >"$fx/no-read"
  return 0
}
# One line of a stubbed plan: the number, the state labels to take off, whether the
# issue carries the label of a merge, and whether the closer read is asked about
# it. These four words are the whole of what the console tells the poll.
stub_plan() { # <number> <labels> <have> <ask>
  printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4"
}
# One issue's node of the closer answer, as the batched search returns it. The
# closer is the only part a case chooses, so the shape of the timeline is
# written once.
node() { # <number> <closer>
  printf '{"number":%s,"timelineItems":{"nodes":[{"closer":%s}]}}' "$1" "$2"
}
merged() { node "$1" '{"merged":true}' >"$fx/closer-$1.json"; }
unmerged() { node "$1" '{"merged":false}' >"$fx/closer-$1.json"; }
by_hand() { node "$1" 'null' >"$fx/closer-$1.json"; }
# A `merged` that is no boolean: the read answers, and the answer is not a
# merge, so the label is never given on it.
string_merged() { node "$1" '{"merged":"true"}' >"$fx/closer-$1.json"; }
numeric_merged() { node "$1" '{"merged":1}' >"$fx/closer-$1.json"; }
object_merged() { node "$1" '{"merged":{"state":"merged"}}' >"$fx/closer-$1.json"; }

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
t_lacks "$t_out" "was not added"

t_case "every state label comes off a closed issue in one edit, and the labels a person chose stay"
reset
echo '[{"number":5,"state":"CLOSED","labels":[{"name":"tehas:running"},{"name":"tehas:awaiting-merge"},{"name":"tehas:question"},{"name":"tehas:planning"},
  {"name":"tehas:refreshing"},{"name":"tehas:blocked"},{"name":"tehas:ready"},{"name":"tehas:plan"},{"name":"tehas:refine"},{"name":"tehas:urgent"},
  {"name":"tehas:next"},{"name":"tehas:prototype"},{"name":"cross-project"},{"name":"bug"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits | wc -l)" 1
for l in tehas:running tehas:question tehas:planning tehas:refreshing tehas:blocked tehas:ready tehas:plan tehas:refine; do
  t_contains "$(edits)" "--remove-label $l"
done
for l in tehas:urgent tehas:next tehas:prototype cross-project bug; do
  t_lacks "$(edits) " "--remove-label $l "
done
t_contains "$t_out" "tidy #5: removed tehas:running tehas:awaiting-merge tehas:question tehas:planning tehas:refreshing tehas:blocked tehas:ready tehas:plan tehas:refine"

t_case "the search names only the state labels, never tehas:shipped or a person's labels"
reset
echo '[]' >"$fx/closed.json"
t_run "$tidy"
search=$(t_calls gh | grep '^issue list')
t_contains "$search" '--search label:"tehas:running","tehas:awaiting-merge","tehas:question","tehas:planning","tehas:refreshing","tehas:blocked","tehas:ready","tehas:plan","tehas:refine"'
for l in tehas:shipped needs-human-attention tehas:urgent tehas:next tehas:prototype cross-project tehas:pause tehas:replaced tehas:refined; do
  t_lacks "$search" "\"$l\""
done

t_case "the console's own list of state labels is the list the search asks for"
# The names live in the console now (console/src/tidy.rs), so this is the line
# between them: the search is made for exactly the names `tidy states` prints, save
# the label of a merge it ends with, and that label is named nowhere in the call. A
# poll that asked for a name the console does not treat as a state label would list
# an issue it then edits nothing about, and one that dropped a name would leave that
# label on closed issues forever.
reset
echo '[]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
want=$("$T_CONSOLE" tidy states | grep -v '^tehas:shipped$' | paste -sd, -)
t_eq "$(search_labels | paste -sd, -)" "$want"
t_lacks "$(t_calls gh | grep '^issue list')" 'tehas:shipped'

# The console holds the rule of the poll, so a command of the platform reads it
# before it calls GitHub, and a console of an older tehas that says nothing is said
# then. The cases from here to "one search a poll" put the console beside the
# command under test: the shapes of a bad one, and of none beside the command at
# all. Each of them installs its stub after the reset of its case, because reset
# clears the scratch files a case drives, and that case brings the console of this
# checkout back, so every case after it runs against the real program and no answer
# of a stub stands in for it.

t_case "a console that names no state labels stops the poll before the first call, exit 78"
# The names of the search come from the console. Without them the command would ask
# GitHub for nothing and tidy a repository it cannot describe, so the poll ends
# before the search, as it does with no console beside it at all.
reset
stub_console empty
t_run "$tidy"
t_eq "$t_rc" 78
t_contains "$t_out" "the console names no state labels"
t_eq "$(t_calls gh)" ""

t_case "a console that lists the state labels but names no label of a merge still never asks the search for that label"
# The shape of the list that has lost the label of a merge: the poll takes the last
# name as that label and searches for the ones before it, so a list of the nine state
# labels alone reads as eight to search for and the last of them to spare. What is
# pinned here is the thing the command must never do, which is ask GitHub for the
# issues of the label tidy itself gives - a search that named it would take that label
# off the closed issues of a merge - so the run goes on and no call names it.
reset
stub_console one-state
echo '[]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
search=$(t_calls gh | grep '^issue list')
t_eq "$(printf '%s\n' "$search" | grep -c 'tehas:shipped')" 0
t_contains "$search" '"tehas:running"'

t_case "a console that gives no page stops the poll, exit 78"
# The limit of the search and the ceiling of the closer read are the one number the
# console prints for the query it prints; a poll that guessed its own could name
# more issues than the read answers for, and label none of them.
reset
stub_console
printf 'zero\n' >"$T/fx/page.txt"
t_run "$tidy"
t_eq "$t_rc" 78
t_contains "$t_out" "the console gave no page"
t_eq "$(t_calls gh)" ""

t_case "an answer the console reads as no list of issues is the failed search, exit 1"
# `console tidy plan` is the console's own test of the answer: the command does
# not guess at JSON, so an answer of GitHub that is no list - no JSON, an object,
# a bare number - is refused there and the poll logs it as the failed search, and
# not as the empty list of a poll with nothing to tidy. A list of things that name
# no issue is refused the same way: the search named nothing it can plan.
reset
stub_console
echo '{"data":{}}' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: tidy: the console could not read the answer of the search"
t_eq "$(edits)" ""

t_case "the plan the console gives is what the poll edits, and its ask says who is read"
# The command holds no rule of its own: the numbers, the names and the issues the
# closer read is asked about all come from `tidy plan`, and an ask of no means the
# read is not made at all for that issue. The search of GitHub answers nothing here,
# so a poll that decided for itself would find no issue to edit.
reset
stub_console
reset
stub_console
stub_plan 7 tehas:running no yes >"$T/fx/plan.txt"
stub_plan 8 tehas:question yes no >>"$T/fx/plan.txt"
echo '[]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" "issue edit 7 --repo acme/widget --remove-label tehas:running
issue edit 8 --repo acme/widget --remove-label tehas:question"
t_eq "$(reads | wc -l)" 1
t_contains "$(reads)" "number:7"
t_lacks "$(reads)" "number:8"

t_case "no console beside the command is exit 78 and no call"
# The header of this file names it: a tidy with no console is a host where
# install.sh never finished. The command says so instead of tidying on the rule of
# whatever console a PATH happened to hold.
reset
rm -f "$shim/libexec/console"
t_run "$tidy"
t_eq "$t_rc" 78
t_contains "$t_out" "no console beside this command"
t_eq "$(t_calls gh)" ""
cp "$T_CONSOLE" "$shim/libexec/console"

t_case "one search a poll, for the closed issues of the repository"
# The search is the one call of a poll that found nothing to tidy, and no poll that
# found nothing makes a call about a closer. The case above took the console away,
# so it comes back as the one of this checkout: every case from here reads the real
# program, and no answer of a stub stands in for it.
reset
shim=$T/tree
cp "$T_CONSOLE" "$shim/libexec/console"
echo '[]' >"$fx/closed.json"
t_run "$tidy"
search=$(t_calls gh | grep '^issue list')
t_eq "$(t_calls gh | grep -c '^issue list')" 1
t_contains "$search" "--repo acme/widget --state closed"
t_eq "$t_rc" 0
# Nothing was to tidy, so the poll made no call about a closer.
t_eq "$(reads | wc -l)" 0
t_eq "$t_out" ""

t_case "an issue that already carries tehas:shipped gets no second add, and no read"
reset
echo '[{"number":44,"state":"CLOSED","labels":[{"name":"tehas:awaiting-merge"},{"name":"tehas:shipped"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" "issue edit 44 --repo acme/widget --remove-label tehas:awaiting-merge"
t_eq "$(reads | wc -l)" 0
t_lacks "$t_out" "added tehas:shipped"
t_lacks "$t_out" "was not added"

t_case "of two closed issues, only the one without the label is read and labelled"
reset
merged 45
echo '[{"number":45,"state":"CLOSED","labels":[{"name":"tehas:awaiting-merge"}]},
  {"number":46,"state":"CLOSED","labels":[{"name":"tehas:running"},{"name":"tehas:shipped"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" "issue edit 45 --repo acme/widget --remove-label tehas:awaiting-merge --add-label tehas:shipped
issue edit 46 --repo acme/widget --remove-label tehas:running"
t_eq "$(reads | wc -l)" 1
t_contains "$(reads)" "number:45"
t_lacks "$(reads)" "number:46"
t_eq "$(printf '%s\n' "$t_out" | grep -c 'was not added')" 0

t_case "an open issue is never touched, whatever the answer says"
reset
merged 8
merged 9
echo '[{"number":8,"state":"OPEN","labels":[{"name":"tehas:awaiting-merge"}]},{"number":9,"labels":[{"name":"tehas:question"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" ""
t_eq "$(reads | wc -l)" 0
t_eq "$t_out" ""

t_case "a closed issue with no state label is not edited"
reset
merged 10
echo '[{"number":10,"state":"CLOSED","labels":[{"name":"tehas:shipped"},{"name":"needs-human-attention"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" ""
t_eq "$(reads | wc -l)" 0

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
# The issue that waits for its cycle is not asked about at all.
t_eq "$(reads | wc -l)" 1
t_contains "$(reads)" "-f q=repo:acme/widget is:issue is:closed number:12"
t_contains "$(reads)" "first: 100"

t_case "the labels come off an issue the search listed without tehas:shipped, when the closer was merged"
reset
merged 13
echo '[{"number":13,"state":"CLOSED","labels":[{"name":"tehas:running"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" "issue edit 13 --repo acme/widget --remove-label tehas:running --add-label tehas:shipped"
t_contains "$(reads)" "q=repo:acme/widget is:issue is:closed number:13"

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
# The read failed at all: one line for the poll says so, and no line calls the
# closer unmerged for an issue the read never answered.
t_contains "$t_out" "$(read_failed_line 1)"
t_lacks "$t_out" "$(no_closer)"

t_case "a failed search is logged, nothing is edited, exit 1"
reset
: >"$fx/search-fails"
t_run "$tidy"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: tidy: the search for closed issues with state labels failed"
t_eq "$(edits)" ""

t_case "an answer that cannot be read is logged, exit 1"
# The word a poll gets for bytes that hold no JSON at all, named as itself: the
# search of GitHub answered, so the line is the console's and not gh's.
reset
echo 'not json' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: tidy: the console could not read the answer of the search"

t_case "a search that hangs is cut off at TEHAS_TIDY_WAIT seconds"
reset
: >"$fx/search-hangs"
T_ENV=(TEHAS_TIDY_WAIT=1)
t_start
t_run "$tidy"
t_eq "$t_rc" 1
t_fast 10000
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
t_eq "$(printf '%s\n' "$t_out" | grep -c 'was not added')" 2

t_case "one read a poll answers for every issue, and each keeps its own closer"
reset
merged 30
unmerged 31
by_hand 32
echo '[{"number":30,"state":"CLOSED","labels":[{"name":"tehas:awaiting-merge"}]},
  {"number":31,"state":"CLOSED","labels":[{"name":"tehas:awaiting-merge"}]},
  {"number":32,"state":"CLOSED","labels":[{"name":"tehas:awaiting-merge"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(reads | wc -l)" 1
t_contains "$(reads)" "number:30 number:31 number:32"
t_eq "$(edits | grep -c -- '--add-label tehas:shipped')" 1
t_eq "$(edits)" "issue edit 30 --repo acme/widget --remove-label tehas:awaiting-merge --add-label tehas:shipped
issue edit 31 --repo acme/widget --remove-label tehas:awaiting-merge
issue edit 32 --repo acme/widget --remove-label tehas:awaiting-merge"

t_case "a merged answer of one issue never labels another"
reset
merged 34
unmerged 35
echo '[{"number":34,"state":"CLOSED","labels":[{"name":"tehas:awaiting-merge"}]},
  {"number":35,"state":"CLOSED","labels":[{"name":"tehas:awaiting-merge"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits | grep -c -- '--add-label tehas:shipped')" 1
t_eq "$(edits)" "issue edit 34 --repo acme/widget --remove-label tehas:awaiting-merge --add-label tehas:shipped
issue edit 35 --repo acme/widget --remove-label tehas:awaiting-merge"

t_case "a merged that is no boolean is no merge: string, number and object"
reset
string_merged 36
numeric_merged 37
object_merged 38
echo '[{"number":36,"state":"CLOSED","labels":[{"name":"tehas:awaiting-merge"}]},
  {"number":37,"state":"CLOSED","labels":[{"name":"tehas:running"}]},
  {"number":38,"state":"CLOSED","labels":[{"name":"tehas:ready"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits | grep -c -- '--add-label')" 0
t_eq "$(edits | wc -l)" 3
t_eq "$(printf '%s\n' "$t_out" | grep -c 'was not added')" 3

t_case "the label is still given when the search forgot to list it, so a race adds it once"
reset
merged 33
echo '[{"number":33,"state":"CLOSED","labels":[{"name":"tehas:awaiting-merge"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" "issue edit 33 --repo acme/widget --remove-label tehas:awaiting-merge --add-label tehas:shipped"
t_contains "$t_out" "tidy #33: added tehas:shipped"

t_case "a closer read that fails still takes the state labels off, and says the read failed"
reset
: >"$fx/graphql-fails"
echo '[{"number":40,"state":"CLOSED","labels":[{"name":"tehas:question"}]},
  {"number":41,"state":"CLOSED","labels":[{"name":"tehas:plan"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" "issue edit 40 --repo acme/widget --remove-label tehas:question
issue edit 41 --repo acme/widget --remove-label tehas:plan"
t_contains "$t_out" "tidy #40: removed tehas:question"
# One line for the poll, not one per issue: a read that never answered is not
# an answer that no closer was merged.
t_contains "$t_out" "$(read_failed_line 2)"
t_lacks "$t_out" "$(no_closer)"
t_lacks "$t_out" "added tehas:shipped"
t_eq "$(t_calls gh | grep -c 'add-label')" 0

t_case "a closer read that hangs is cut off, the labels still come off, and the poll goes on"
reset
: >"$fx/graphql-hangs"
echo '[{"number":42,"state":"CLOSED","labels":[{"name":"tehas:question"}]}]' >"$fx/closed.json"
T_ENV=(TEHAS_TIDY_WAIT=1)
t_start
t_run "$tidy"
t_eq "$t_rc" 0
# One issue, one read, one second: the bound is the wait, not a loose ceiling.
t_fast 5000
t_eq "$(edits)" "issue edit 42 --repo acme/widget --remove-label tehas:question"
t_contains "$t_out" "tidy #42: removed tehas:question"
t_contains "$t_out" "$(read_failed_line 1)"
t_lacks "$t_out" "$(no_closer)"

t_case "a read that answers and names none of the issues is a failed read, not a closed issue"
reset
: >"$fx/no-node"
merged 47
echo '[{"number":47,"state":"CLOSED","labels":[{"name":"tehas:question"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" "issue edit 47 --repo acme/widget --remove-label tehas:question"
t_eq "$(t_calls gh | grep -c 'add-label')" 0
t_contains "$t_out" "$(read_failed_line 1)"
t_lacks "$t_out" "$(no_closer)"

t_case "a read that answers and names the issue with no merged closer is logged per issue"
reset
by_hand 48
echo '[{"number":48,"state":"CLOSED","labels":[{"name":"tehas:question"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" "issue edit 48 --repo acme/widget --remove-label tehas:question"
t_contains "$t_out" "$(no_closer 48)"
t_lacks "$t_out" "the read of what closed these issues failed"

t_case "a GraphQL answer that carries errors adds nothing, even where it holds a merged closer"
reset
merged 43
echo '{"data":{"search":{"nodes":[{"number":43,"timelineItems":{"nodes":[{"closer":{"merged":true}}]}}]}}}' >"$fx/answer-errors"
echo '[{"number":43,"state":"CLOSED","labels":[{"name":"tehas:question"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" "issue edit 43 --repo acme/widget --remove-label tehas:question"
t_eq "$(t_calls gh | grep -c 'add-label')" 0
t_contains "$t_out" "$(read_failed_line 1)"
t_lacks "$t_out" "$(no_closer)"

t_case "a label that is not a state label is neither taken off nor written to the log"
# The names come from GitHub. One that is not a state label is not passed to the
# edit, and a name carrying a newline of its own cannot put a second line in
# dispatch.log.
reset
merged 55
printf '[{"number":55,"state":"CLOSED","labels":[{"name":"tehas:running"},{"name":"fake\\u000aWARN: tidy #999: forged line"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" "issue edit 55 --repo acme/widget --remove-label tehas:running --add-label tehas:shipped"
t_lacks "$(edits)" "fake"
t_eq "$(printf '%s\n' "$t_out" | grep -c 'tidy #55')" 2
t_lacks "$t_out" "forged line"

t_case "the control issue is tidied like any other closed issue, and the label never goes on it"
# libexec/status leaves the number TEHAS_CONTROL_ISSUE names out of every list
# and every count: it holds the pause switch and is not work. tidy now writes the
# label too, so it must follow the same rule, whatever the closer read answers.
reset
merged 1
merged 23
echo '[{"number":1,"state":"CLOSED","labels":[{"name":"tehas:running"}]},
  {"number":23,"state":"CLOSED","labels":[{"name":"tehas:running"}]}]' >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(edits)" "issue edit 23 --repo acme/widget --remove-label tehas:running --add-label tehas:shipped"
t_lacks "$t_out" "#1:"
t_eq "$(reads | wc -l)" 1
t_lacks "$(reads)" "number:1"
t_contains "$(reads)" "number:23"

t_case "the candidates past the page of the read are left to a later poll"
# More candidates than the page of the shared read, twice over: the terms of a
# search are capped, so the read is asked about tehas_shipped_page of them and
# no more, and the rest keep their state labels for the polls that follow.
reset
page=$(tehas_page_count)
last=$((page + page + 100))
for n in $(seq 100 "$last"); do merged "$n"; done
{
  printf '['
  first=1
  for n in $(seq 100 "$last"); do
    [ "$first" = 1 ] || printf ','
    first=0
    printf '{"number":%s,"state":"CLOSED","labels":[{"name":"tehas:running"}]}' "$n"
  done
  printf ']'
} >"$fx/closed.json"
t_run "$tidy"
t_eq "$t_rc" 0
t_eq "$(reads | wc -l)" 1
t_eq "$(edits | grep -c -- '--add-label tehas:shipped')" "$page"
t_contains "$(reads)" "number:100"
# The candidates past the page were not named: a list past the ceiling of a
# search is no answer at all, so a longer one is refused rather than truncated.
t_lacks "$(reads)" "number:$last"
t_eq "$(edits | wc -l)" "$((last - 100 + 1))"

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
