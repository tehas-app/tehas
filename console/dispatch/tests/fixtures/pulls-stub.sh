#!/usr/bin/env bash
fx=$T/fx
pulls=$( [ -r "$fx/pulls.json" ] && printf '%s' "$(<"$fx/pulls.json")" || echo "[]" )
jq -n --argjson pulls "$pulls" '
  # This repository answers with databaseId 111; a head repository of it
  # says the head is a branch here, another id or a null one a fork. GitHub
  # gives a pull request a body always: none reads as the empty text.
  # The viewer of a me read is the instance bot: its id is the sum of the
  # characters of its login, the id the worlds record for it. A case that
  # wants its pull request taken as the one of the bot names that id as
  # the author of it.
  {data: {viewer: {databaseId: 903}, repository: {databaseId: 111, nameWithOwner: "acme/widget",
    pullRequests: {totalCount: ($pulls | length),
    pageInfo: {hasNextPage: false, endCursor: null},
    nodes: ($pulls | map(. as $p | ($p.number // 0) as $no | {
      number: $no, title: ($p.title // ("pull " + ($no | tostring))),
      url: ($p.html_url // ("https://github.com/acme/widget/pull/" + ($no | tostring))),
      state: ($p.state // "OPEN"), isDraft: false,
      isCrossRepository: ($p.isCrossRepository // false),
      headRepository: (if ($p.headRepository.nameWithOwner // "acme/widget") == "acme/widget"
        then {databaseId: 111, nameWithOwner: "acme/widget"}
        else {databaseId: 999, nameWithOwner: $p.headRepository.nameWithOwner} end),
      headRefName: ($p.headRefName // ("feature/other-" + ($no | tostring))),
      headRefOid: "0123456789abcdef0123456789abcdef01234567",
      baseRefName: "main", baseRefOid: "a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0",
      mergeable: "MERGEABLE", mergeStateStatus: "CLEAN",
      createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z",
      lastEditedAt: null,
      # A closed pull request was closed at a time and a merged one merged
      # at one: GitHub always says when, so the answer says it too unless
      # the case names its own.
      mergedAt: ($p.mergedAt // (if ($p.state // "OPEN") == "MERGED"
        then "2026-01-01T02:00:00Z" else null end)),
      closedAt: ($p.closedAt // (if ($p.state // "OPEN") == "CLOSED"
        then "2026-01-01T02:00:00Z" else null end)),
      additions: 0, deletions: 0, changedFiles: 0,
      author: ($p.author // {login: "alice"}),
      editor: null, mergedBy: null, mergeCommit: null,
      renames: {nodes: []},
      labels: {totalCount: (((($p.labels // {}) | .nodes) // []) | length),
        nodes: ((($p.labels // {}) | .nodes) // [])},
      closingIssuesReferences: {totalCount: 0, nodes: []},
      body: ($p.body // "")}))}}}}'
