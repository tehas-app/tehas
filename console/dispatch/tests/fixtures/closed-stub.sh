#!/usr/bin/env bash
fx=$T/fx
read_or() { # <file> <default>
  [ -r "$1" ] && printf '%s' "$(<"$1")" && return
  printf '%s' "$2"
}
lists=$(for f in ready blocked asking plans refines awaiting marked; do read_or "$fx/$f.json" "[]"; echo; done | jq -cs 'map((. // []) | flatten)')
jq -n --argjson lists "$lists" '
  {data: {repository: {databaseId: 111, issues: {pageInfo: {hasNextPage: false, endCursor: null},
    nodes: ([$lists[][] | select(.state == "CLOSED") | .number | tonumber? // empty] | unique
      | map({number: ., title: "an issue", body: "", state: "CLOSED", stateReason: "DONE",
        url: ("https://github.com/acme/widget/issues/" + (. | tostring)),
        createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z",
        author: null, issueType: null, milestone: null,
        labels: {totalCount: 1, nodes: [{name: "tehas:running"}]}, parent: null}))}}}}'
