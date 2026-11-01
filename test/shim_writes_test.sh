#!/usr/bin/env bash
# Tests for the gh shim's write log (host/shim/gh): which calls it counts as
# writes, the six fields of the line it appends, and that a write keeps gh's
# own stdout, stderr and exit status whatever the log does. The real gh is a
# stub (t_stub) that answers what GH_OUT, GH_ERR and GH_RC name: no network,
# and no GitHub asked of anything.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

t_console || t_bad "no console: $t_console_why"
log=$T/home/tehas/state/github-writes.log

# The poll's token (#938), measured on the same installed shim. The dispatcher
# gives every gh child of a poll TEHAS_GH_TOKEN, and the shim then exports it
# as GH_TOKEN and runs no libexec/github-token. A token program of the case's
# stands under libexec and says on a file of its own that it ran, so the one
# thing gh's arguments cannot say - that it was not run at all - is seen.
install_shim() {
  local h=$T/home/tehas
  rm -rf "$h" "$T"/calls/* "$T/err" "$T/token-runs"
  mkdir -p "$h/bin" "$h/shim" "$h/libexec"
  cp "$T_ROOT/host/shim/gh" "$h/shim/gh"
  chmod +x "$h/shim/gh"
  # The two lines lib/config.sh and lib/github-app.sh, from the source of this
  # run: the token program of libexec is asked in the environment of the run,
  # so it must be able to load its settings as the host's does.
  mkdir -p "$h/lib"
  cp "$T_ROOT/lib/config.sh" "$T_ROOT/lib/github-app.sh" "$T_ROOT/lib/paths.sh" "$h/lib/"
  # A gh that answers what it is told to: the words, the error and the status
  # are named by the case, in the environment of its run. With nothing named it
  # says the call back on stdout, which is how a case sees that the shim
  # reached gh at all and handed its words back. The words name no token: the
  # token belongs to test/github_app_test.sh, which keeps a real one, and a
  # case of this file names one itself when it wants gh to have had it.
  t_stub gh 'if [ -n "${GH_OUT:-}" ]; then printf "%s\n" "$GH_OUT"; else echo "gh $*"; fi
if [ -n "${SHOW_TOKEN:-}" ]; then printf "token=%s tehas=%s\n" "${GH_TOKEN:-none}" "${TEHAS_GH_TOKEN:-unset}"; fi
[ -z "${GH_ERR:-}" ] || printf "%s\n" "$GH_ERR" >&2
exit "${GH_RC:-0}"'
  # libexec/github-token of this run with the two it needs beside it, which
  # load the settings and print a token that has at least 10 minutes left;
  # the case keeps the one it wants
  # and the run notes here that the program was asked at all. openssl and curl
  # are never reached, because the kept token has its time left.
  cp "$T_ROOT/libexec/github-token" "$T_ROOT/libexec/with-settings" "$h/libexec/"
  chmod +x "$h/libexec/github-token"
  ln -sf "$T_CONSOLE" "$h/libexec/console"
  # t_instance writes the instance and names it in the runs that follow.
  t_instance TEHAS_REPO=acme/widget TEHAS_GITHUB_APP_ID=5452597 \
    TEHAS_GITHUB_APP_INSTALLATION=174742089 "TEHAS_BOT_LOGIN=tehas-app-bot[bot]"
  mkdir -p "$T/home/.config/tehas"
  printf 'a key\n' >"$T/home/.config/tehas/github-app.pem"
  chmod 600 "$T/home/.config/tehas/github-app.pem"
  # The note of a run, beside the kept token of the case, with an hour left.
  # The real program stays under libexec, where its own directory finds the two
  # lines of lib/ above; a case says whether it was asked by what it leaves on
  # $T/token-runs.
  mkdir -p "$h/state/github"
  printf '%s\n' "${TOKEN_ANSWER:-ghs_token}" >"$h/state/github/token"
  echo $(($(date +%s) + 3600)) >"$h/state/github/token.expires"
  {
    printf '%s\n' "printf x >>\"$T/token-runs\""
    cat "$h/libexec/github-token"
  } >"$h/libexec/github-token.new"
  mv "$h/libexec/github-token.new" "$h/libexec/github-token"
  chmod 755 "$h/libexec/github-token"
  mv "$T/bin/gh" "$h/bin/gh"
  chmod +x "$h/bin/gh"
}
reset() {
  install_shim
  T_ENV=()
}
# shim <arguments...>: one call of the installed shim. t_run keeps no stderr of
# its own: it reads the command's two streams together into t_out, so a case
# that wants the error stream alone names it in the snippet it runs itself.
shim() { t_run "$T/home/tehas/shim/gh $(printf '%q ' "$@")"; }
# A snippet for a case that wants the two streams apart: t_run keeps no stream
# of its own, so the error goes to a file the case reads itself.
shim_split() {
  local err=$T/err
  rm -f "$err"
  t_run "$T/home/tehas/shim/gh $(printf '%q ' "$@") 2>$(printf '%q' "$err")"
  t_err=$(cat "$err" 2>/dev/null)
}
# lines: the log's content, empty when there is no log at all.
lines() { cat "$log" 2>/dev/null || true; }
# field <n>: the nth field of the last line, cut at the tabs.
field() { tail -n 1 <<<"$(lines)" | cut -f"$1"; }

t_case "the write of an issue comment is one line with the right kind, repository and number"
reset
shim issue comment 12 -R o/r --body secret
t_eq "$t_rc" 0
t_eq "$(lines | wc -l)" 1
t_eq "$(field 2)" "issue comment"
t_eq "$(field 3)" "o/r"
t_eq "$(field 4)" "12"
t_eq "$(field 5)" "0"
t_eq "$(field 6)" "-"
t_eq "$(field 1 | grep -cE '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$')" 1
t_eq "$(awk -F'\t' '{ print NF }' <<<"$(lines)")" 6
# Neither what was written nor how the call was paid for reaches the log.
t_lacks "$(lines)" secret
t_lacks "$(lines)" ghs_

t_case "an api write names the repository and the number of its path"
reset
shim api -X POST repos/o/r/issues/12/labels -f 'labels[]=x'
t_eq "$t_rc" 0
t_eq "$(lines | wc -l)" 1
t_eq "$(field 2)" "api POST"
t_eq "$(field 3)" "o/r"
t_eq "$(field 4)" "12"
t_eq "$(field 5)" "0"
t_lacks "$(lines)" 'labels[]=x'

t_case "a call with fields and no method is a write, for gh posts it"
reset
shim api repos/o/r/issues/7/comments -f body=hello
t_eq "$(field 2)" "api POST"
t_eq "$(field 3)" "o/r"
t_eq "$(field 4)" "7"
shim api repos/a/b/git/refs -F name=x
t_eq "$(field 2)" "api POST"
t_eq "$(field 3)" "a/b"
t_eq "$(field 4)" "-"
# The same path with neither a method nor a field is a read, so the count says
# the read below added nothing to the two writes above.
shim api repos/o/r
t_eq "$(lines | wc -l)" 2

t_case "the -XPOST and --method=POST forms name the same kind"
reset
shim api -XPUT repos/a/b
t_eq "$(field 2)" "api PUT"
shim api --method=PUT repos/a/b
t_eq "$(field 2)" "api PUT"
shim api -Xput repos/a/b
t_eq "$(field 2)" "api PUT"
shim api --method=delete repos/a/b
t_eq "$(field 2)" "api DELETE"
shim api -X DELETE repos/a/b
t_eq "$(field 2)" "api DELETE"

t_case "reads add no line at all"
reset
# The stub's answer names the call, so one read of the caller's stream says gh
# was reached, and the empty log says nothing was charged for it.
shim issue view 12
t_eq "$t_rc" 0
t_eq "$t_out" "gh issue view 12"
shim api repos/o/r
t_eq "$t_rc" 0
shim pr view 3 --repo o/r
t_eq "$t_rc" 0
shim issue list --repo o/r
t_eq "$t_rc" 0
shim pr status
t_eq "$t_rc" 0
shim label list --repo o/r
t_eq "$t_rc" 0
shim label get tehas:ready
t_eq "$t_rc" 0
shim auth status
t_eq "$t_rc" 0
t_eq "$(lines)" ""

t_case "a call GitHub rate limited keeps gh's status and stderr, and its line ends 429"
reset
T_ENV=(GH_OUT="gh issue comment 12 -R o/r --body secret" GH_ERR="HTTP 429: Too Many Requests" GH_RC=1)
shim issue comment 12 -R o/r --body secret
t_eq "$t_rc" 1
t_eq "$t_out" "gh issue comment 12 -R o/r --body secret
HTTP 429: Too Many Requests"
t_eq "$(lines | wc -l)" 1
t_eq "$(field 5)" "1"
t_eq "$(field 6)" "429"

t_case "a forbidden call gives 403, and a call that says both gives 403"
reset
T_ENV=(GH_ERR="HTTP 403: Resource not accessible by integration" GH_RC=1)
shim pr merge 5 --repo o/r
t_eq "$t_rc" 1
t_eq "$(field 6)" "403"
T_ENV=(GH_ERR="HTTP 403: forbidden
HTTP 429: too many" GH_RC=1)
shim issue close 9 -R o/r
t_eq "$(field 6)" "403"

t_case "stdout, stderr and the status of a write are gh's"
reset
# What gh prints reaches the caller on the stream gh chose, and the status is
# gh's own and not the shim's.
T_ENV=(GH_OUT="a word out" GH_ERR="a word on error" GH_RC=7)
shim issue edit 4 -R o/r --title T
t_eq "$t_rc" 7
t_eq "$t_out" "a word out
a word on error"
t_eq "$(lines | wc -l)" 1
t_eq "$(field 5)" "7"

t_case "the kinds are the two words of the call, label's three among them"
reset
shim pr comment 8 -R o/r
t_eq "$(field 2)" "pr comment"
t_eq "$(field 3)" "o/r"
t_eq "$(field 4)" "8"
shim issue create --repo o/r --title T
t_eq "$(field 2)" "issue create"
t_eq "$(field 3)" "o/r"
t_eq "$(field 4)" "-"
shim issue close 3
t_eq "$(field 2)" "issue close"
t_eq "$(field 3)" "-"
t_eq "$(field 4)" "3"
shim pr reopen 11 --repo=o/r
t_eq "$(field 2)" "pr reopen"
t_eq "$(field 3)" "o/r"
t_eq "$(field 4)" "11"
shim pr ready 6
t_eq "$(field 2)" "pr ready"
t_eq "$(field 4)" "6"
shim label create tehas:ready --repo o/r
t_eq "$(field 2)" "label create"
t_eq "$(field 3)" "o/r"
t_eq "$(field 4)" "-"
shim label edit tehas:ready --repo o/r
t_eq "$(field 2)" "label edit"
shim label delete tehas:ready -R o/r
t_eq "$(field 2)" "label delete"
t_eq "$(lines | wc -l)" 8

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

t_case "a log that cannot be written changes nothing of gh's result"
reset
# A log that is a directory cannot be opened for appending. gh is called
# exactly as before and its words come back on their own streams; the one word
# about the log is on the error stream, named here as the error stream of the
# run, because t_run keeps no stream of its own.
mkdir -p "$(dirname "$log")"
mkdir "$log"
shim_split issue comment 2 -R o/r
t_eq "$t_rc" 0
t_eq "$t_out" "gh issue comment 2 -R o/r"
t_eq "$t_err" "gh: the write log $log could not be written"
t_eq "$(t_calls gh)" "issue comment 2 -R o/r"
# Nothing was added to a log that cannot be appended to: it is still empty,
# which is what `wc -c` of an empty directory says, and what a log of a write
# that was never recorded must say.
t_eq "$(wc -c <"$log" 2>/dev/null | tr -d '[:space:]' | head -n 1)" 0

# The same call, with the log made a plain file again, is written: the shim
# gives up on the one line and nothing else.
rm -rf "$log"
shim issue comment 1 -R o/r >/dev/null
t_eq "$(lines | wc -l)" 1

t_case "a home with no state directory gets one, and the line"
reset
rm -rf "$T/home/tehas/state"
shim issue comment 3 -R o/r >/dev/null
t_eq "$t_rc" 0
t_eq "$(lines | wc -l)" 1

# The poll's token (#938): the dispatcher gives every gh child of a poll the
# token it took at its start as TEHAS_GH_TOKEN, and the shim hands it to gh as
# GH_TOKEN and runs no libexec/github-token. The token program of the home says
# on a file of its own that it ran, so a case sees both that gh was given the
# token and that the program was not run for the call.
t_case "with TEHAS_GH_TOKEN set the shim gives gh that token and runs no token program"
reset
T_ENV=(TEHAS_GITHUB_APP_ID=5452597 TEHAS_GH_TOKEN=ghs_poll SHOW_TOKEN=1)
shim issue view 12 -R o/r
t_eq "$t_rc" 0
t_eq "$t_out" "gh issue view 12 -R o/r
token=ghs_poll tehas=ghs_poll"
# A read, so no line of its own, and no run of the token program either.
t_eq "$(lines)" ""
t_eq "$(cat "$T/token-runs" 2>/dev/null | wc -c | tr -d '[:space:]' | head -n 1)" 0

t_case "without TEHAS_GH_TOKEN the shim runs the token program, as it did"
reset
T_ENV=(TEHAS_GITHUB_APP_ID=5452597 SHOW_TOKEN=1)
shim issue view 12 -R o/r
t_eq "$t_rc" 0
t_eq "$t_out" "gh issue view 12 -R o/r
token=ghs_token tehas=unset"
# The program ran once for the one call of the shim.
t_eq "$(cat "$T/token-runs" | wc -c | tr -d '[:space:]' | head -n 1)" 1

t_case "an empty TEHAS_GH_TOKEN asks the token program as the name being unset does"
reset
# env leaves an empty value assigned, as it does for the other names of the
# run. What a case can read of such a call is that the program was asked: the
# name itself reaches the stub of gh empty or not as env gave it, and the shim
# asks for a token either way - the two readings together are the case.
T_ENV=(TEHAS_GITHUB_APP_ID=5452597 TEHAS_GH_TOKEN= SHOW_TOKEN=1)
shim issue view 12 -R o/r
t_eq "$t_rc" 0
t_contains "$t_out" "token=ghs_token"
t_eq "$(cat "$T/token-runs" | wc -c | tr -d '[:space:]' | head -n 1)" 1

t_case "with no app named, TEHAS_GH_TOKEN still reaches gh as its token"
reset
T_ENV=(TEHAS_GH_TOKEN=ghs_poll SHOW_TOKEN=1)
shim api repos/o/r
t_eq "$t_rc" 0
t_eq "$t_out" "gh api repos/o/r
token=ghs_poll tehas=ghs_poll"
t_eq "$(cat "$T/token-runs" 2>/dev/null | wc -c | tr -d '[:space:]' | head -n 1)" 0

t_done
