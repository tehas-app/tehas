#!/usr/bin/env bash
# Tests for tehas labels against a stub gh: which labels it makes, where,
# and that it needs the repository and nothing else. And for
# tehas_label_allowed in lib/labels.sh: who applied a label, asked of GitHub.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

labels="$T_ROOT/bin/tehas labels"
t_console || t_bad "$t_console_why"
T_ENV_ALWAYS=(TEHAS_CONSOLE="$T_CONSOLE")
table=$T/home/tehas/lib/labels.tsv
t_stub gh 'case "$*" in *"${FAIL_ON:-no label fails}"*) exit 1 ;; esac'
reset() {
  rm -f "$T"/calls/*
  T_ENV=()
  t_instance TEHAS_REPO=acme/widget
  mkdir -p "${table%/*}"
  cp "$T_ROOT/host/labels.tsv" "$table"
}

t_case "it needs only the repository, and makes the nineteen labels there"
reset
t_run "$labels"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | wc -l)" 19
t_eq "$(t_calls gh | grep -c -e '--repo acme/widget .*--force')" 19
for name in tehas:ready tehas:urgent tehas:running tehas:awaiting-merge tehas:shipped tehas:pause needs-human-attention \
  tehas:question tehas:plan tehas:refine tehas:planning tehas:replaced tehas:refined tehas:millwright-escalated tehas:next tehas:blocked tehas:refreshing tehas:all-hands tehas:lean; do
  t_contains "$(t_calls gh)" "label create $name --repo acme/widget"
  t_contains "$t_out" "label $name"
done

t_case "it creates the all-hands label"
t_contains "$(t_calls gh)" "label create tehas:all-hands "

t_case "it creates the question label"
t_contains "$(t_calls gh)" "label create tehas:question "

t_case "it creates the planning labels"
t_contains "$(t_calls gh)" "label create tehas:plan "
t_contains "$(t_calls gh)" "label create tehas:planning "

t_case "a label that cannot be made is named, the rest are still made, exit 1"
reset
T_ENV=(FAIL_ON=tehas:urgent)
t_run "$labels"
t_eq "$t_rc" 1
t_contains "$t_out" "tehas labels: could not make tehas:urgent"
t_contains "$t_out" "label tehas:pause"
t_eq "$(t_calls gh | wc -l)" 19

t_case "without the repository nothing is called, exit 78"
reset
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$labels"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_REPO is not set"
t_eq "$(t_calls gh)" ""

t_case "an argument is a usage error, before the settings are read"
reset
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$labels --force"
t_eq "$t_rc" 64
t_contains "$t_out" "usage: tehas labels"
t_lacks "$t_out" "TEHAS_REPO"
t_eq "$(t_calls gh)" ""

t_case "a table that is missing or wrong makes nothing, exit 78"
reset
rm "$table"
t_run "$labels"
t_eq "$t_rc" 78
t_contains "$t_out" "no table of labels at $table: run install.sh"
t_eq "$(t_calls gh)" ""
reset
printf 'tehas:ready\t0e8a16\tfine\nno colour here\n' >"$table"
t_run "$labels"
t_eq "$t_rc" 78
t_contains "$t_out" "$table line 2 is no label"
t_eq "$(t_calls gh)" ""

t_case "a label's description reaches gh as one argument, as the table has it"
reset
t_run "$labels"
t_contains "$(t_calls gh)" "label create tehas:ready --repo acme/widget --color 0e8a16 --description Tehas: eligible for dispatch (apply by hand to authorize a cycle) --force"

# --- tehas_label_allowed -------------------------------------------------------
t_stub gh 'case "$*" in
  "api repos/acme/widget/issues/7/events?per_page=100 --paginate")
    [ ! -e "$T/events-fail" ] || exit 1
    cat "$T/events.json" ;;
esac'
allowed=". $T_ROOT/lib/labels.sh; TEHAS_REPO=acme/widget TEHAS_ALLOWED_LABELERS='alice bob'; tehas_label_allowed 7 tehas:ready"
labelled() { # <login>...: each applies tehas:ready in turn
  local i=0 login
  for login in "$@"; do
    i=$((i + 1))
    printf '{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"%s"},"created_at":"2026-10-1%sT00:00:00Z"}\n' "$login" "$i"
  done | jq -s . >"$T/events.json"
}

t_case "the label counts when an allowed account applied it most recently"
rm -f "$T"/calls/* "$T/events-fail"
labelled mallory bob
t_run "$allowed"
t_eq "$t_rc" 0
t_eq "$(t_calls gh)" "api repos/acme/widget/issues/7/events?per_page=100 --paginate"

t_case "it does not when the last account is not allowed, or no event applies it"
labelled alice mallory
t_run "$allowed"
t_eq "$t_rc" 1
echo '[]' >"$T/events.json"
t_run "$allowed"
t_eq "$t_rc" 1

t_case "events that cannot be read are 2, not a refusal"
: >"$T/events-fail"
t_run "$allowed"
t_eq "$t_rc" 2
rm -f "$T/events-fail"
echo '{"message":"Not Found"}' >"$T/events.json"
t_run "$allowed"
t_eq "$t_rc" 2

t_done
