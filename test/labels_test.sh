#!/usr/bin/env bash
# Tests for libexec/labels against a stub gh: which labels it makes, where,
# and that it needs the repository and nothing else. And for
# tehas_label_allowed in lib/labels.sh: who applied a label, asked of GitHub.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

labels="$T_ROOT/libexec/labels"
t_stub gh 'case "$*" in *"${FAIL_ON:-no label fails}"*) exit 1 ;; esac'
reset() {
  rm -f "$T"/calls/*
  T_ENV=()
  t_instance TEHAS_REPO=acme/widget
}

t_case "it needs only the repository, and makes the ten labels there"
reset
t_run "$labels"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | wc -l)" 10
t_eq "$(t_calls gh | grep -c -e '--repo acme/widget .*--force')" 10
for name in tehas:ready tehas:urgent tehas:running tehas:awaiting-merge tehas:shipped tehas:pause needs-human-attention \
  tehas:question tehas:plan tehas:planning; do
  t_contains "$(t_calls gh)" "label create $name --repo acme/widget"
  t_contains "$t_out" "label $name"
done

t_case "it creates the question label"
t_contains "$(t_calls gh)" "label create tehas:question "

# The description is what a person reads on GitHub. A cycle whose review did
# not reach its end is parked, not labelled awaiting-merge, so no label here
# may claim that the pull request was reviewed.
t_case "no label description says the pull request was reviewed"
reset
t_run "$labels"
t_contains "$(t_calls gh)" "label create tehas:awaiting-merge --repo acme/widget --color fbca04 --description Tehas: PR open, only the merge left to a human --force"
t_lacks "$(t_calls gh)" "reviewed"

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
t_eq "$(t_calls gh | wc -l)" 10

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
