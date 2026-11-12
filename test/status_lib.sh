#!/usr/bin/env bash
# shellcheck shell=bash
# shellcheck disable=SC2119,SC2120  # epics_page takes the nodes or none
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
# shellcheck disable=SC2016,SC2034  # stub bodies are expanded when the stub runs; the test files read the variables
# What the tests of the factory's state share: test/status_*_test.sh source this.
# They cover what tehasbot status writes (the queue snapshot and the health file),
# tehasbot health beside it, and what tehas status prints from the health file.
# Here are the stubs, the tree the commands run in, and the builders of the state
# they read that more than one of the four files calls. The part has sourced
# test/lib.sh before this.

# The limit is read and written by the console (console flags).
t_console || t_bad "no console: $t_console_why"
T_ENV_ALWAYS+=(TEHAS_CONSOLE="$T_CONSOLE")

# The command as a host has it: bin/tehasbot above the console, this
# checkout's own build, which holds the command.
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_console || t_bad "no console: $t_console_why"
shim=$T/tree
mkdir -p "$shim/libexec"
ln -s "$T_ROOT/lib" "$shim/lib"
cp "$T_CONSOLE" "$shim/libexec/console"
mkdir -p "$shim/bin"
ln -s "$T_ROOT/bin/tehasbot" "$shim/bin/tehasbot"
status="$shim/bin/tehasbot status"
health="$T_ROOT/bin/tehasbot health"
summary="$T_ROOT/bin/tehas status"
live=$T/home/tehas/live

t_stub gh '
# One answer per search. The count is the only call that asks for a total and
# no nodes, so that shape tells the two apart; the count of the label under
# test is never the thing that routes the answer.
gh_graphql() {
  case "$1" in
    *subIssues*) [ ! -e "$T/epics-down" ] || exit 1; cat "$T/epics-page.json" ;;
    *"first: 0) { issueCount }"*) [ ! -e "$T/count-down" ] || exit 1
      cat "$T/count.json" ;;
    *) cat "$T/shipped.json" ;;
  esac
}
[ ! -e "$T/gh-down" ] || exit 1
case "$*" in
  "issue list"*--search*) cat "$T/issues.json" ;;
  "issue list"*) [ ! -e "$T/all-down" ] || exit 1; cat "$T/all.json" ;;
  # The names of the labels of the control issue, as --jq joins them.
  "issue view"*) [ ! -e "$T/control-down" ] || exit 1
    { [ "$(cat "$T/paused")" != true ] || echo tehas:pause; [ ! -e "$T/allhands" ] || echo tehas:all-hands; } | paste -sd " " ;;
  "issue edit"*) [ -z "${GH_EDIT_FAILS:-}" ] || exit 1 ;;
  # gh is signed in, which is how the releases of a private repository are read.
  "auth status") exit "${GH_AUTH:-0}" ;;
  "api repos/acme/widget/issues/"*"/events"*) cat "$T/events.json" 2>/dev/null || exit 1 ;;
  "api graphql"*) [ ! -e "$T/shipped-down" ] || exit 1; gh_graphql "$*" ;;
  # The published releases of the platform, for the platform line of a release
  # host; nothing until a case puts the answer there.
  "api repos/"*"/releases"*) [ ! -e "$T/rel.json" ] || cat "$T/rel.json" ;;
esac'
# A release host reads the list with gh when it is signed in and with curl
# otherwise; CURL_RC is what curl answers with, and 22 is no answer at all.
t_stub curl '[ ! -e "${CURL_RC:-}" ] && exit "${CURL_RC:-0}"; exit 0'
# No sandbox image: status asks docker for the label of the one cycles run
# (src/rukas.rs), and this host's own must not answer.
t_stub docker 'exit 1'
# The failed tehas units, as systemctl --user --failed --plain --no-legend lists
# them: none until a case writes $T/failed-units, and the read fails with
# SYSTEMCTL_RC when a case names one.
t_stub systemctl 'cat "$T/failed-units" 2>/dev/null; exit "${SYSTEMCTL_RC:-0}"'
closed() { # number closedAt closer [label]: a closed issue as the search for shipped work returns it
  local label=""
  [ -z "${4:-}" ] || label="{\"name\":\"$4\"}"
  printf '{"number":%s,"title":"t","url":"u","createdAt":"2025-12-01T00:00:00Z","updatedAt":"%s","closedAt":"%s","labels":{"nodes":[%s]},"timelineItems":{"nodes":[{"closer":%s}]}}' \
    "$1" "$2" "$2" "$label" "$3"
}
no_labels() { # number closedAt closer: as closed, with no labels field at all - a read that said nothing of them
  printf '{"number":%s,"title":"t","url":"u","updatedAt":"%s","closedAt":"%s","timelineItems":{"nodes":[{"closer":%s}]}}' \
    "$1" "$2" "$2" "$3"
}
shipped() { # closed issues, as the GraphQL answer
  local IFS=,
  printf '{"data":{"search":{"nodes":[%s]}}}' "$*" >"$T/shipped.json"
}
epics_page() { # issue nodes: the one page of open issues the read of the epics answers
  local IFS=,
  printf '{"data":{"repository":{"issues":{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[%s]}}}}' "$*" >"$T/epics-page.json"
}
shipped_count() { # total of the closed issues labelled tehas:shipped, as GraphQL answers it
  printf '{"data":{"search":{"issueCount":%s}}}' "$1" >"$T/count.json"
}
# The state the count cases share: one closed issue a merged pull request closed,
# and the count answer of reset still saying 0.
one_shipped() {
  reset
  shipped "$(closed 20 2026-01-02T00:00:00Z '{"merged":true}')"
}
issue() { # number label state
  printf '{"number":%s,"title":"t","url":"u","state":"%s","labels":[{"name":"%s"}],"createdAt":"2025-12-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z","closedAt":null}' "$1" "${3:-OPEN}" "$2"
}
reset() {
  rm -rf "$T"/calls/* "$T/gh-down" "$T/all-down" "$T/shipped-down" "$T/count-down" "$T/control-down" "$T/allhands" "$T/epics-down" "$live" \
    "$T/home/tehas/state" "$T/home/tehas/current" "$T/home/tehas/versions" "$T/home/tehas/versions.env" "$T/rel.json"
  echo "[]" >"$T/all.json"
  epics_page
  shipped
  shipped_count 0
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
  # The state store the health file reads the planning turns from, with
  # today's backup.
  t_store_home "$T/home/tehas"
  TEHAS_HOME=$T/home/tehas "$T_STORE" backup >/dev/null || t_bad "reset: the store made no backup"
  echo "[$(issue 3 tehas:ready),$(issue 4 tehas:ready),$(issue 5 tehas:awaiting-merge),$(issue 6 needs-human-attention)]" >"$T/issues.json"
  echo false >"$T/paused"
  echo '[]' >"$T/events.json"
}
one() { # issue running status step [started [updated]]: one cycle on the store, as the live view shows it; times are seconds ago
  t_live "$T/home/tehas" "$@"
}
cycle() { # running status step: issue 12, running or the last one
  one 12 "$@"
}

held() { # <why> <text> <free> <await room>
  mkdir -p "$live"
  jq -n --arg why "$1" --arg hold "$2" --argjson free "$3" --argjson aw "$4" --arg at "$(date -u -d '-30 seconds' +%FT%TZ)" \
    '{time: $at, why: $why, hold: $hold, count: 1, room: 3, committed: 2147483648, max: 21474836480, reserve: "2g",
      free: $free, awaitRoom: $aw}' >"$live/dispatch.json"
}
