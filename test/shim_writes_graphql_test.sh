#!/usr/bin/env bash
# Tests for the gh shim's write log: how an api graphql call is told from a
# query and a mutation, and what the line it appends names. The real gh is
# a stub (t_stub) that answers what GH_OUT, GH_ERR and GH_RC name: no
# network, and no GitHub asked of anything.
# shellcheck disable=SC2016,SC2154  # stub bodies are expanded when they run; log and t_err come from shim_writes_lib.sh
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/shim_writes_lib.sh
. "$(dirname "$0")/shim_writes_lib.sh" || exit 1
t_case "a graphql query adds no line and a graphql mutation adds one of kind api POST"
reset
# gh sends POST for every graphql call, a query among them, so the method says
# nothing here and the first word of the query does (#792). What an idle poll
# asks GitHub is queries, and their lines are what made its write figure wrong.
shim api graphql -f 'query=query { viewer { login } }'
t_eq "$t_rc" 0
t_eq "$t_out" "gh api graphql -f query=query { viewer { login } }"
# The shape of the dispatcher's own calls: variables with -F, the document with
# -f, and the word `mutation` inside the document where it names no operation.
shim api graphql -F owner=o -F name=r -f 'query=query($owner: String!, $name: String!) {
  repository(owner: $owner, name: $name) { pullRequests(states: OPEN, first: 100) { nodes { mutation } } } }'
t_eq "$t_rc" 0
# A named operation says what it is whether the kind comes first or its own
# name does. A name of its own and no kind is a query, even when the body calls
# a mutation field, which is what GraphQL does with a field under a query.
shim api graphql -f 'query=query AddLabel { addLabels(subjectId: "x") { clientMutationId } }'
shim api graphql -f 'query=AddLabel { addLabels(subjectId: "x") { clientMutationId } }'
shim api graphql -f 'query=fragment F on Issue { mutation }'
# A document that names no operation is a query, and a document of -F is a
# value and not the operation gh is asked for.
shim api graphql -f 'query={ viewer { mutationField } }'
shim api graphql -F query='query { a }'
shim api graphql -F query='mutation { x }'
# Nothing on the query side of it either: the document of one -f, a search
# string of another, and flags of gh's that name no operation at all.
shim api graphql -f q='repo:o/r is:issue is:closed' -f 'query=query { a }'
shim api graphql -XPOST -f 'query=query { a }'
shim api graphql --input=x -f 'query=query { a }'
t_eq "$(lines)" ""
# The mutations, one line each, of the kind the poll's figure counts as other:
# the kind first, the kind in caps, the kind after a name of its own, and the
# kind after a name and a document of its own, each separated by a newline.
for a in 'mutation { x }' 'MUTATION { x }' '
    AddLabelling

    mutation
    ($o: String!) {
      setLabels(subjectId: $o) { clientMutationId }
    }' 'mutation($n: Int!) {
      addLabels(subjectId: $n, labelIds: ["mutation"]) { clientMutationId }
    }'; do
  shim api graphql -f "query=$a"
  t_eq "$t_rc" 0
  t_eq "$(field 2)" "api POST"
  t_eq "$(field 3)" "-"
  t_eq "$(field 4)" "-"
done
t_eq "$(lines | wc -l)" 4
# The document of a call is never in the line, as no other field value is: the
# reads above, whose every -f and -F value says mutation, added no line of
# their own and no line of these four names the path of a repository.

# The REST classification of an api call that is not graphql is as it was: the
# words of the query field of one of them name no operation.
t_case "an api call that is not graphql is still told by its method and its fields"
reset
shim api repos/o/r/branches -f 'query=mutation { x }'
t_eq "$(field 2)" "api POST"
t_eq "$(field 3)" "o/r"
shim api -X POST 'repos/o/r/issues/12/comments?mutation=1' -f body=x
t_eq "$(field 2)" "api POST"
t_eq "$(field 3)" "o/r"
t_eq "$(field 4)" "12"
shim api repos/o/r
t_eq "$(lines | wc -l)" 2

t_done
