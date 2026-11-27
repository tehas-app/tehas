#!/usr/bin/env bash
fx=$T/fx
# Read without cat: an idle poll's count of programs is the poll's, not the
# stub's.
read_or() { # <file> <second file or ""> <default>
  [ -r "$1" ] && printf '%s' "$(<"$1")" && return
  [ -n "$2" ] && [ -r "$2" ] && printf '%s' "$(<"$2")" && return
  printf '%s' "$3"
}
lists=$(for f in ready blocked asking plans refines awaiting marked; do read_or "$fx/$f.json" "" "[]"; echo; done | jq -cs 'map((. // []) | flatten)')
nums=$(jq -r '[.[][] | select((.state // "OPEN") != "CLOSED") | .number | tonumber? // empty] + [1] | unique | .[]' <<<"$lists")
# An issue whose cycle holds a lock file is marked running, as on a host; and
# an issue a case wrote the REST answer of is on GitHub too: an open one is in
# the queue whether or not a list names it. With $fx/page-drops-events the
# page answers every timeline empty, as GitHub can for a large page.
# Held, as flock -n finds it. The system's flock, on an open file and with no
# command: a case may count the flock and true it stubs. /proc/locks does not
# do, as in a container its inodes are not the ones ls -i gives.
fl=$(command -pv flock)
locked=$(for f in "$T_CYCLES"/[0-9]*.lock; do
  [ -e "$f" ] || continue
  { "$fl" -n 3 || { f=${f##*/}; echo "${f%.lock}"; }; } 3<"$f"
done | jq -Rcs 'split("\n") | map(select(. != "") | tonumber)')
for n in $(jq -r '.[]' <<<"$locked"); do
  [ "$(jq -r '.state // "open" | ascii_downcase' "$fx/work-$n.json" 2>/dev/null || echo open)" = open ] && nums=$(printf '%s\n%s\n' "$nums" "$n" | sort -un)
done
for w in "$fx"/work-*.json; do
  [ -f "$w" ] || continue
  n=${w##*/work-}
  n=${n%.json}
  [ "$(jq -r '.state // "open" | ascii_downcase' "$w" 2>/dev/null)" = open ] && nums=$(printf '%s
%s
' "$nums" "$n" | sort -un)
done
items=$(for n in $nums; do
  jq -nc --argjson n "$n" --argjson ev "$(read_or "$fx/events-$n.json" "" "[]")" \
    --argjson w "$(read_or "$fx/work-$n.json" "" null)" \
    --argjson e "$(read_or "$fx/edit-$n.json" "$fx/edit.json" "{}")" \
    --argjson p "$(read_or "$fx/prs-$n.json" "$fx/prs.json" "{}")" \
    --argjson i "$(read_or "$fx/issue-$n.json" "" "{}")" \
    '{n: $n, ev: $ev, w: $w, e: ($e.data.repository.issue // {}), p: $p, i: $i}'
done | jq -cs .)
jq -n --argjson lists "$lists" --argjson items "$items" --argjson control "$(read_or "$fx/control.json" "" '{"labels":[]}')" \
  --argjson repo "$(read_or "$fx/repo.json" "" '{}')" --argjson locked "$locked" \
  --argjson dropped "$([ -e "$fx/page-drops-events" ] && echo true || echo false)" '
  # GraphQL names the user id databaseId; an id the REST fixture gives is kept.
  def actor($a): if $a == null then null else {login: $a.login}
    + (if $a | has("id") then {databaseId: $a.id}
       elif $a | has("databaseId") then {databaseId: $a.databaseId} else {} end) end;
  def labels($n): [$lists[][] | select((.number | tonumber?) == $n) | (.labels // [])[].name]
    + (if $n == 1 then [$control.labels[].name] else [] end)
    # Being on a list is carrying its label, as gh issue list --label has it.
    + [range(7) as $i | select(any($lists[$i][]; (.number | tonumber?) == $n))
       | ["tehas:ready", "tehas:blocked", "tehas:question", "tehas:plan", "tehas:refine",
          "tehas:awaiting-merge", "tehas:running"][$i]] | unique;
  # An event of the timeline, as GitHub answers the question of the page.
  def evnode($e): {__typename: (if $e.event == "labeled" then "LabeledEvent"
      elif $e.event == "unlabeled" then "UnlabeledEvent" else "RenamedTitleEvent" end),
    createdAt: $e.created_at, actor: actor($e.actor)}
    + (if $e.label then {label: $e.label} else {} end)
    # A rename answers its titles: the adapter reads an event that names
    # none as no answer at all.
    + (if $e.event == "renamed" then {previousTitle: ($e.previous_title // "an old title"),
        currentTitle: ($e.current_title // "a new title")} else {} end);
  # A linked pull request is of the bot on a branch of the repository, unless
  # a case says otherwise: every field the digest of a change is read from.
  def prnode($p): (($p.url // "") | split("/") | last) as $last |
    ($p.number // ($last | tonumber? // 0)) as $no | {
      number: $no, title: ($p.title // ("pull " + ($no | tostring))),
      url: ($p.url // ("https://github.com/acme/widget/pull/" + ($no | tostring))),
      state: ($p.state // "OPEN"), isDraft: false,
      isCrossRepository: ($p.isCrossRepository // false),
      headRefName: ($p.headRefName // ("feature/issue-" + ($no | tostring))),
      headRefOid: "0123456789abcdef0123456789abcdef01234567",
      baseRefName: "main", baseRefOid: "a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0",
      mergeable: "MERGEABLE", mergeStateStatus: "CLEAN",
      createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z",
      lastEditedAt: null, mergedAt: null, closedAt: null,
      additions: 0, deletions: 0, changedFiles: 0,
      author: ($p.author // {login: "tehas-bot"}),
      editor: null, mergedBy: null, mergeCommit: null,
      renames: {nodes: []},
      labels: {totalCount: 0, nodes: []},
      closingIssuesReferences: {totalCount: 0, nodes: []},
      repository: {databaseId: 111, nameWithOwner: "acme/widget"}};
  ($items | map(select(((.w.state // "open") | ascii_downcase) == "open")
    | select((.i.state // "OPEN") != "CLOSED"))) as $open |
  {data: {repository: {databaseId: 111, nameWithOwner: $repo.nameWithOwner,
    issues: {totalCount: ($open | length),
      pageInfo: {hasNextPage: false, endCursor: null},
      nodes: [$open[] | . as $i |
        # The labels its lists give it, and those of its REST answer when a
        # case wrote one: on GitHub the two agree. The answer of gh issue
        # view, when a case wrote its state, is the truth. A locked cycle
        # marks the issue running, as on a host.
        ((if $i.i.state then [($i.i.labels // [])[] | .name? // . | strings]
            else (labels($i.n) + [($i.w.labels // [])[] | .name? // . | strings]) end)
          + (if any($locked[]; . == $i.n) then ["tehas:running"] else [] end) | unique) as $names |
        # The timeline the case wrote, plus the application of each carried
        # label the case gave no event of: a page that shows a mark the
        # timeline does not vouch for is cut short, and the adapter then
        # reads each kind apart.
        ([$i.ev[] | select($dropped | not) | .label.name? // empty] | unique) as $named |
        ([$i.ev[] | select($dropped | not) | evnode(.)]
          + [if $dropped then empty else $names[] end
             | select(. as $l | ($named | index($l)) == null)
             | evnode({"event": "labeled", "created_at": "2026-01-01T00:00:00Z",
                       "actor": {"login": "tehas-bot"}, "label": {"name": .}})]) as $events |
        {number: .n, title: (.e.title // .i.title // .w.title // ""),
          body: (.e.body // .i.body // .w.body // ""),
          state: "OPEN", stateReason: null,
          url: ("https://github.com/acme/widget/issues/" + (.n | tostring)),
          createdAt: (.w.created_at // "2026-01-01T00:00:00Z"),
          updatedAt: (.w.updated_at // "2026-01-01T00:00:00Z"),
          author: null, issueType: null, milestone: null,
          labels: {totalCount: ($names | length), nodes: ($names | map({name: .}))},
          parent: null,
          lastEditedAt: .e.lastEditedAt, editor: .e.editor,
          comments: {totalCount: 0},
          assignees: {totalCount: 0, nodes: []},
          # What it waits on and its parts, whole: the summaries the REST
          # answer of the issue holds, answered as their nodes.
          blockedBy: {totalCount: ((.w.issue_dependencies_summary.blocked_by // 0)),
            nodes: [range(.w.issue_dependencies_summary.blocked_by // 0) as $k |
              {__typename: "Issue", number: (8000 + $k), state: "OPEN",
               repository: {databaseId: 111, nameWithOwner: "acme/widget"}}]},
          subIssues: {totalCount: ((.w.sub_issues_summary.total // 0)),
            pageInfo: {hasNextPage: false, endCursor: null},
            nodes: [range(.w.sub_issues_summary.total // 0) as $k |
              {__typename: "Issue", number: (9000 + $k),
               state: (if $k < (.w.sub_issues_summary.completed // 0) then "CLOSED" else "OPEN" end),
               repository: {databaseId: 111, nameWithOwner: "acme/widget"}}]},
          mentions: {totalCount: 0, pageInfo: {hasNextPage: false, endCursor: null}, nodes: []},
          events: {totalCount: ($events | length),
            pageInfo: {hasPreviousPage: false, startCursor: null}, nodes: $events},
          closedByPullRequestsReferences: {
            totalCount: ((.p.data.repository.issue.closedByPullRequestsReferences.nodes // []) | length),
            nodes: ((.p.data.repository.issue.closedByPullRequestsReferences.nodes // []) | map(prnode(.)))}}]}}}}'
