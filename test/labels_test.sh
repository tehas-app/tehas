#!/usr/bin/env bash
# Tests for libexec/labels against a stub gh: which labels it makes, where,
# and that it needs the repository and nothing else.
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

t_case "it needs only the repository, and makes the nine labels there"
reset
t_run "$labels"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | wc -l)" 9
t_eq "$(t_calls gh | grep -c -e '--repo acme/widget .*--force')" 9
for name in tehas:ready tehas:urgent tehas:running tehas:awaiting-merge tehas:pause needs-human-attention \
  tehas:question tehas:plan tehas:planning; do
  t_contains "$(t_calls gh)" "label create $name --repo acme/widget"
  t_contains "$t_out" "label $name"
done

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
t_eq "$(t_calls gh | wc -l)" 9

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

t_done
