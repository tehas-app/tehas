#!/usr/bin/env bash
n=${1:?}
fx=$T/fx
ev=$( [ -r "$fx/events-$n.json" ] && printf '%s' "$(<"$fx/events-$n.json")" || echo "[]" )
dropped=$([ -e "$fx/page-drops-events" ] && echo true || echo false)
jq -n --argjson n "$n" --argjson ev "$ev" --argjson dropped "$dropped" '
  # GraphQL names the user id databaseId; an id the REST fixture gives is kept.
  def actor($a): if $a == null then null else {login: $a.login}
    + (if $a | has("id") then {databaseId: $a.id}
       elif $a | has("databaseId") then {databaseId: $a.databaseId} else {} end) end;
  def evnode($e): {__typename: (if $e.event == "labeled" then "LabeledEvent"
      elif $e.event == "unlabeled" then "UnlabeledEvent" else "RenamedTitleEvent" end),
    createdAt: $e.created_at, actor: actor($e.actor)}
    + (if $e.label then {label: $e.label} else {} end)
    # A rename answers its titles: the adapter reads an event that names
    # none as no answer at all.
    + (if $e.event == "renamed" then {previousTitle: ($e.previous_title // "an old title"),
        currentTitle: ($e.current_title // "a new title")} else {} end);
  ([$ev[] | select($dropped | not) | evnode(.)]) as $nodes |
  {data: {repository: {databaseId: 111, issue: {number: $n, milestone: null,
    closes: {totalCount: 0, pageInfo: {hasPreviousPage: false, startCursor: null}, nodes: []},
    blockedBy: {totalCount: 0, nodes: []},
    subIssues: {totalCount: 0, pageInfo: {hasNextPage: false, endCursor: null}, nodes: []},
    mentions: {totalCount: 0, pageInfo: {hasNextPage: false, endCursor: null}, nodes: []},
    events: {totalCount: ($nodes | length),
      pageInfo: {hasNextPage: false, endCursor: null, hasPreviousPage: false, startCursor: null},
      nodes: $nodes},
    parent: null}}}}'
