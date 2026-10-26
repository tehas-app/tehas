# shellcheck shell=bash
# shellcheck disable=SC2034  # tehas_shipped_label is read by the callers
# What closed an issue: the one GraphQL read that answers "a merged pull
# request closed this", and the one test that accepts the answer. Two commands
# act on it - tehasbot status (libexec/status) lists the shipped issues and
# gives them the label, and tehas tidy (libexec/tidy) puts the label on in the
# edit that takes the state labels off - so both read the query and the rule
# printed here, as lib/refresh.sh does for the pull requests a refresh works
# on: the shape of the timeline and the acceptance rule have one statement, and
# a change to either reaches the two at once.
#
# The closing pull request is not a field of an issue: it is in the issue's
# closed event, so this is GraphQL and not `gh issue list`.
#
#   tehas_shipped_label           the label's name
#   tehas_shipped_page            how many issues one read of the search may
#                                 name: the page of the query and the ceiling of
#                                 the list a caller builds. The two are one
#                                 number so they cannot drift apart. GitHub puts
#                                 a ceiling on the terms of a search and the
#                                 read carries one per issue, so a longer list
#                                 is not truncated: it is no answer at all
#   tehas_shipped_query           print the GraphQL query: the last closed
#                                 event of each issue the search $q names,
#                                 whether its closer was a merged pull request,
#                                 and the names of the labels the issue holds.
#                                 $q must name every issue asked for, and no
#                                 more than tehas_shipped_page of them
#   tehas_shipped_rule            print the jq definition of `shipped`: of one
#                                 issue node, 0 when its last closed event is
#                                 a merged pull request. The answer is read
#                                 with tehas_shipped_answer
#   tehas_shipped_answer <issue> <json>
#                                 <issue> is digits and nothing else; anything
#                                 else is 1. 0 when a whole search answered and
#                                 named the issue <issue>; 1 when it did not
#                                 answer, or answered and said nothing of that
#                                 issue, which is no answer and not a negative
#                                 one; 2 when it named the issue and no merged
#                                 closer, which is a closed issue, not a read
#                                 that failed. A response that carries top-level
#                                 errors is never a whole answer, even when it
#                                 holds data
#   tehas_shipped_has <node> <label>
#                                 of one node of that query: 0 when <label> is
#                                 one of the names the node holds, 1 when the
#                                 read named its labels and the label is none,
#                                 and 2 when it named no list of them - a caller
#                                 edits on the 2 as on the 1, and never guesses
#                                 that an issue it cannot read lacks the label
#
# Sourcing this file does nothing: it prints nothing and calls nothing.

tehas_shipped_label=tehas:shipped
tehas_shipped_page=100

tehas_shipped_query() {
  # shellcheck disable=SC2016  # $q is a GraphQL variable
  printf 'query($q: String!) { search(type: ISSUE, query: $q, first: %s) { nodes { ... on Issue {\n' "$tehas_shipped_page"
  # shellcheck disable=SC2016  # the rest is a GraphQL document, not shell
  printf '%s\n' '    number labels(first: 100) { nodes { name } } timelineItems(itemTypes: [CLOSED_EVENT], last: 1) { nodes { ... on ClosedEvent {
      closer { ... on PullRequest { merged } } } } } } } } }'
}

tehas_shipped_rule() {
  # A closer is a merge only when GitHub says `merged` is the boolean true: a
  # string, a number or an object is no answer and is never taken for one. The
  # definition is a predicate - it leaves its input alone and answers true or
  # false - so a caller may put it in select() and keep the node.
  # shellcheck disable=SC2016  # a jq program
  printf '%s\n' 'def shipped: ((.timelineItems.nodes // [])[0].closer.merged | . == true);'
}

# Does the node carry <label>? Its names reach jq and not the shell, so one of
# "*" is no glob and one with a space is no two names. 2 is the node that holds
# no list of names, which is no answer and not the empty list of an issue that
# carries none.
# shellcheck disable=SC2329  # tehasbot status calls it, beside the read of the query
tehas_shipped_has() { # <one node of that query> <label>
  local names rc=0
  # shellcheck disable=SC2016  # a jq program
  names=$(jq -ce ' if (((.labels // {}) | type) != "object"
        or ((.labels.nodes // null) | type) != "array") then error("no list")
      else [.labels.nodes[]?.name | tostring] end' <<<"${1:-}" 2>/dev/null) || rc=$?
  [ "$rc" = 0 ] || return 2
  jq -e --arg l "${2:-}" 'index($l) != null' <<<"$names" >/dev/null 2>&1
}

tehas_shipped_answer() { # <issue> <answer of the search>
  local nodes
  # The number is digits or the question was not asked properly. The test is
  # done by substitution and not a pattern so there is one reading of what a
  # digit is, and the number reaches jq as text: --argjson would take any JSON
  # number for the number of an issue, so "42.0" or "1e2" would be accepted.
  [[ ${1:-} == "${1//[^0-9]/}" ]] && [ -n "${1:-}" ] || return 1
  local n=${1:-}
  # The whole answer first: a declined read, a read of no list, or a read that
  # never spoke of this issue, are all 1. Only an issue the read did name and
  # whose closer was no merged pull request is 2. The errors key is refused only
  # when it holds something: gh answers "errors": [] for a read that went well,
  # and an empty list is no refusal.
  nodes=$(jq -ce --arg n "$n" "$(tehas_shipped_rule)
    if ((.errors // []) | length) > 0 then error(\"declined\")
    elif (.data.search.nodes | type) != \"array\" then error(\"no list\")
    else . end
    | .data.search.nodes
    | [ .[] | select(.number == (\$n | tonumber)) ]
    # A read that never named this issue is no answer, not a negative one: the
    # caller cannot be told the issue was closed by a person when the read in
    # fact said nothing of it.
    | if length == 0 then error(\"no node\") else . end
    | [ .[] | select(shipped) ]
    | length" <<<"${2:-}" 2>/dev/null) || return 1
  [ "${nodes:-0}" != 0 ] && return 0
  return 2
}
