# The verdict question of the preview stub gh: the sweep and the run ask the
# code-host port for one pull request by number, an alias each, and this
# answers it from the same fixtures the read by number has, as one GraphQL
# node each. A number whose fixture is there answers the node; one without it
# answers NOT_FOUND, which is how GitHub says it knows no such pull request;
# fx/pull-<n>.rc is a call that fails, as it is for the read by number.
# Called by console/tests/preview/gh.sh and by test/preview_lib.sh.

nodes='"databaseId":7001'
errs=""
for n in $(printf "%s\n" "$*" | tr ' ' '\n' | sed -n 's/^n\([0-9][0-9]*\):$/\1/p'); do
  if [ -f "$T/fx/pull-$n.rc" ]; then
    echo "gh: API rate limit exceeded (HTTP 403)" >&2
    exit 1
  fi
  if [ -f "$T/fx/pull-$n.json" ]; then
    # The node as the adapter reads a pull request of the list: state, head,
    # times, counts, author and its association. What the fixtures do not
    # hold is answered as GitHub always does: a member's open request with
    # its head here, no draft, no edit, no merge.
    node=$(jq -c --argjson here 7001 '{
      number: .number,
      title: (.title // ("Change " + (.number | tostring))),
      state: (if .state == "closed" then "CLOSED" else "OPEN" end),
      isDraft: false,
      isCrossRepository: ((.head.repo != null)
        and (((.head.repo.full_name // "") | ascii_downcase) != "acme/widget")),
      headRepository: (if (.head.repo != null)
        and (((.head.repo.full_name // "") | ascii_downcase) == "acme/widget")
        then {databaseId: $here} else null end),
      headRefName: ("topic-" + (.number | tostring)),
      headRefOid: .head.sha,
      baseRefName: "main",
      baseRefOid: ("ab" * 20),
      createdAt: (.updated_at // "2026-10-11T09:00:00Z"),
      updatedAt: (.updated_at // "2026-10-11T09:00:00Z"),
      mergedAt: null,
      closedAt: (if .state == "closed" then "2026-10-11T10:00:00Z" else null end),
      lastEditedAt: null,
      additions: 1, deletions: 0, changedFiles: 1,
      mergeable: "UNKNOWN", mergeStateStatus: "UNKNOWN",
      url: ("https://github.com/acme/widget/pull/" + (.number | tostring)),
      author: (if .user == null then null else
        {__typename: (.user.type // "User"), login: .user.login, databaseId: 101}
      end),
      authorAssociation: (.author_association // null),
      editor: null, mergedBy: null, mergeCommit: null,
      labels: {totalCount: 0, nodes: []},
      closingIssuesReferences: {totalCount: 0, nodes: []}
    }' "$T/fx/pull-$n.json") || {
      echo "gh: the fixture of $n is no answer" >&2
      exit 1
    }
  else
    node=null
    errs="${errs:+,}{\"type\":\"NOT_FOUND\",\"path\":[\"repository\",\"n$n\"]}"
  fi
  nodes="$nodes,\"n$n\":$node"
done
if [ -n "$errs" ]; then
  printf '{"data":{"repository":{%s}},"errors":[%s]}\n' "$nodes" "$errs"
else
  printf '{"data":{"repository":{%s}}}\n' "$nodes"
fi
