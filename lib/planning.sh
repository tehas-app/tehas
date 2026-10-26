# shellcheck shell=bash
# The steps of a planning session. Sourced by libexec/dispatch, and written
# for it: these functions use the dispatcher's own helpers (log, say,
# post_question, refresh_checkout), its variables (repo, plan_cmd, who,
# plabel, posted), Q_MAX_OPTIONS of lib/questions.sh, tehas_millwright_is of
# lib/millwright.sh, tehas_analytics_plan of lib/analytics.sh, tehas_rukas_commit
# of lib/cycles.sh and the state store of lib/store.sh.
#
# A session is a Pi session that lives across polls. Its record is on the
# state store (plan-begin, plan-answer, plan-end, plan-show, plan-note): the
# session id, its turns, the options of the open question, whether an outcome
# waits to be posted, whether the last turn failed, whether a turn is running
# and since when, the id of the open question, whether pi-rukas lost it, who
# started it and with which label, the brief, the answers with their text,
# the issues it filed and the comments the poll spoke about once. Each change
# is one verb and one transaction. An ended session stays readable until the
# next tehas:plan of the issue replaces it.
#
# Beside the record, as files, are what a turn's unit and container pass on:
# the outcome the runner wrote (state/plan-<issue>.json.out) until it is
# posted, the turn's exit status (.rc), the lock the unit holds while the turn
# runs (.lock), and what the runner carries from one turn to the next, in
# $TEHAS_WORK_DIR-plan-<issue>.keep (plan_keep), beside the workspace. A first
# turn's brief is a file in $TEHAS_HOME/tmp until the turn ends.
#
# A turn is on record as running before it starts, and before the issue is
# marked as planning. It runs as its own unit, tehas-plan-<issue>.service, which
# holds the lock while the turn runs, as a cycle holds
# state/cycles/<issue>.lock, and leaves the turn's exit status in .rc. A
# running session found at the start of a poll is a turn still in flight, one
# that ended (plan_collect posts it), or, with no lock held, no unit and no
# status, one that was cut off, for example by a restart of the host.
# plan_recover turns that into a failed turn that asks again, or ends a first
# turn that never produced a session.
#
# Each change on GitHub is checked, and the record only moves on once it has
# worked, so a poll that failed halfway is repeated by the next one without
# posting twice or announcing twice. A turn's end that the store would not
# take keeps its .rc, so the next poll collects it again.
#
#   plan_state <issue>             print the base name of the turn's files:
#                                  state/plan-<issue>.json.out, .json.rc and
#                                  .json.lock. The record is the store's
#   plan_keep <issue>              print the directory the runner carries the
#                                  session in
#   plan_sessions                  read the open sessions (plan-show --open)
#                                  into plan_open_json, once a poll and again
#                                  after a planning write; 1 when the store
#                                  could not answer
#   plan_open <issue>              0 when a session is open on the issue, or
#                                  when the store could not say. A caller that
#                                  would undo or spend something on it asks
#                                  plan_sessions first, and waits when that
#                                  fails: plan_open_word says why
#   plan_turn <issue> first|next <file> [<comment id>] [<login>]
#                                  start one turn as its own unit; its outcome
#                                  is posted when it has ended. <file> is the
#                                  brief of a first turn, kept until the turn
#                                  ends, or the text of an answer, removed once
#                                  the store has it. The id is that of the reply
#                                  the answer came from, whose attachments the
#                                  turn is given, and the login is who answered.
#                                  Returns 1 when the store refused the turn
#   plan_finish <issue> <rc> <turns before> <session before>
#                                  record how a turn went and post it; a failed
#                                  first turn takes its session from the
#                                  outcome file, if the runner left one there.
#                                  It first records the turn in the factory's
#                                  analytics, `analytics plan`, from the record
#                                  as the turn left it; a failure there is a
#                                  warning. Returns 1, the .rc kept, when the
#                                  store would not take the end
#   plan_launch <issue> <command>...
#                                  start the turn's unit and return
#   plan_in_flight <issue>         0 while the turn's unit runs or its lock
#                                  is held
#   plan_collect <issue>           post a turn that ended; 1 when none did
#   plan_recover <issue>           finish a turn that was cut off, or end a
#                                  session found lost that could not be ended
#   plan_brief <issue> <JSON> [<refine brief>]
#                                  write a first turn's brief to brief_file
#   plan_post <issue>              post the outcome that waits
#   plan_replace <issue> <filed>   end a refine that filed: the filed issues
#                                  say Replaces and are sub-issues; the issue
#                                  says Replaced by and is closed tehas:replaced,
#                                  or open tehas:refined with later parts left
#   plan_end <issue> <text> [lost] [hand]
#                                  end the session and say why; with hand the
#                                  issue is left needs-human-attention. Returns
#                                  1, having changed nothing, if the labels stay
#   plan_close <issue>             end the session without a word, as when its
#                                  issue was closed

plan_state() { printf '%s/state/plan-%s.json\n' "$TEHAS_HOME" "$1"; }
plan_keep() { printf '%s-plan-%s.keep\n' "${TEHAS_WORK_DIR:?}" "$1"; }
# plan_keep_make <dir>: make the keep directory, in a directory of workspaces
# that tehas made, as tehas plan and tehas work ask, and marked before
# anything is in it. 1 when that directory is someone else's.
plan_keep_make() {
  local works
  works=$(dirname "$1")
  if [ -e "$works" ] && [ ! -f "$works/.tehas-workspaces" ]; then
    log "ERROR: $works exists and was not made by tehas, not touching it"
    return 1
  fi
  mkdir -p "$works" && : >"$works/.tehas-workspaces" && mkdir -p "$1"
}

# The open sessions, as plan-show --open lists them. A planning write of this
# poll makes the next plan_sessions read them again.
plan_open_read=0
plan_open_known=0
plan_open_json='[]'
plan_open_set=" "
plan_open_word=""
plan_sessions() {
  local list
  if [ "$plan_open_read" = 1 ]; then
    [ "$plan_open_known" = 1 ]
    return
  fi
  plan_open_read=1
  plan_open_known=0
  plan_open_json='[]'
  plan_open_set=" "
  plan_open_word=unreadable
  # shellcheck disable=SC2154  # lib/store.sh, which the caller sources, sets the word
  tehas_store plan-show --open || { plan_open_word=$tehas_store_word; return 1; }
  list=$(tehas_store_get '[.sessions[] | select((.issue | type) == "number")]') || return 1
  # shellcheck disable=SC2034  # read by libexec/dispatch
  plan_open_json=$list
  plan_open_set=" $(jq -r 'map(.issue | tostring) | join(" ")' <<<"$list") "
  plan_open_known=1
  # shellcheck disable=SC2034  # read by libexec/dispatch
  plan_open_word=""
}
plan_open() {
  plan_sessions || return 0
  [[ $plan_open_set == *" $1 "* ]]
}
# A write to the store of a session: the open set is read again after it.
plan_store() {
  plan_open_read=0
  tehas_store "$@"
}

# The brief a first turn is given, in $TEHAS_HOME/tmp until the turn has
# ended: the turn's unit reads it after the poll that started it is gone.
plan_brief_rm() { rm -f "$TEHAS_HOME/tmp/plan-$1.brief."*; }

# The files beside a session that ended.
plan_files_rm() {
  local st
  st=$(plan_state "$1")
  rm -rf "$st.out" "$st.rc" "$(plan_keep "$1")"
  plan_brief_rm "$1"
}

# plan_hand: hand when the store's last answer is of a refine rule 5a started.
plan_hand() { [ "$(tehas_store_get '.planning.auto // false' 2>/dev/null)" != true ] || echo hand; }

# The refine brief goes after the issue's text, so the first line is still the
# title the planner is asked to plan. The last line names the issue under
# discussion: without it the planner matches its own issue as the duplicate of
# the plan and stops at turn 1 (issue 542).
plan_brief() {
  local n=$1
  brief_file=""
  [ -e "$TEHAS_HOME/tmp" ] || mkdir -m 700 "$TEHAS_HOME/tmp" 2>/dev/null
  brief_file=$(mktemp "$TEHAS_HOME/tmp/plan-$n.brief.XXXXXX") &&
    jq -r '"# \(.title // "")\n\n\(.body // "")"' <<<"$2" >"$brief_file" &&
    { [ -z "${3:-}" ] || printf '\n\n---\n\n%s\n\nThis request is issue #%s itself. The issue you file replaces #%s: #%s is not a duplicate of this plan.\n' "$(cat "$3")" "$n" "$n" "$n" >>"$brief_file"; } &&
    return 0
  [ -z "$brief_file" ] || rm -f "$brief_file"
  log "ERROR: could not write the brief of #$n"
  return 1
}

# plan_end <issue> <text> [lost]: a lost session is closed before it is
# announced. It stays running on record until it is closed, so the next poll
# ends it again through plan_recover, and says it then: said first, it would
# be said every poll until a close went through. Any other session that could
# not be closed is closed by the next poll without a word, so it is said now.
plan_end() {
  local n=$1 hand=--remove-label
  [ "${4:-}" != hand ] || hand=--add-label
  if ! gh issue edit "$n" --repo "${repo:?}" --remove-label tehas:question --remove-label tehas:planning \
    --remove-label tehas:plan "$hand" needs-human-attention >/dev/null 2>&1; then
    log "WARN: could not take the planning labels off #$n, so its session is not ended yet"
    return 1
  fi
  if ! plan_close "$n"; then
    [ "${3:-}" = lost ] && return 1
    say "$n" "$2"
    return 0
  fi
  say "$n" "$2"
  log "planning #$n ended"
}

# A session whose end the store would not take is still open without its
# labels, so the next poll ends it without a word.
# shellcheck disable=SC2154  # lib/store.sh, which the caller sources, sets the word
plan_close() {
  if ! plan_store plan-end "$1" --close; then
    log "WARN: the session of #$1 could not be ended on the state store ($tehas_store_word); the next poll tries again"
    return 1
  fi
  plan_files_rm "$1"
}

# The address of an issue a turn filed, as jq matches it: no line end, no
# space and nothing else a value on a command line could be split at.
plan_filed_re='^https://github\.com/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+/issues/[0-9]+\z'
# A session id the runner names and the store keeps: the store takes 128
# characters at most.
plan_sid_ok() { [[ $1 =~ ^[A-Za-z0-9-]{1,128}$ ]]; }

# plan_filed <file>: the issues a turn's outcome names as filed, as one line
# of addresses separated by ", ". An empty line when it names none. A filed
# that is not a list of strings is none.
plan_filed() {
  jq -r --arg plan_filed_re "$plan_filed_re" 'if (.filed | type) == "array" then
      [.filed[] | select(type == "string") | select(test($plan_filed_re))]
      | unique | join(", ")
    else "" end' "$1" 2>/dev/null
}

plan_post() {
  local n=$1 st turns comment failed label what
  st=$(plan_state "$n")
  if ! tehas_store plan-show "$n"; then
    log "WARN: the planning record of #$n could not be read ($tehas_store_word); the next poll tries again"
    return 1
  fi
  turns=$(tehas_store_get '.planning.turns // 0') || turns=0
  comment=$(tehas_store_get '.planning.comment_id // "" | tostring') || comment=""
  failed=$(tehas_store_get '.planning.failed // false') || failed=false
  label=$(tehas_store_get '.planning.label // ""') || label=""
  # What the turn filed is said once, under the agent's own words. The store
  # keeps it with the session, so the next turn's answer can name it: an issue
  # the agent never saw the result of would otherwise be filed again.
  what=$(plan_filed "$st.out") || what=""
  if [ -n "$what" ] && [ "$label" = tehas:refine ]; then
    plan_replace "$n" "$what"
    return
  fi
  # Step one: the question. Once its id is on record it is never posted again.
  if [ -z "$comment" ]; then
    if ! post_question plan "$n" "$turns" "$st.out" "${what:+Filed in this turn: $what}"; then
      log "WARN: the planning turn of #$n could not be posted; the next poll tries again"
      return 1
    fi
    plan_open_read=0
    if ! tehas_store_try 3 question-asked "$n" --id "${posted:?}" --kind plan; then
      log "WARN: the record of #$n could not be updated after its question was posted ($tehas_store_word)"
      return 1
    fi
  fi
  # Step two: show the question. Retried alone if it fails.
  if [ "$failed" = true ]; then
    set -- --remove-label tehas:planning --add-label tehas:question --add-label needs-human-attention
  else
    set -- --remove-label tehas:planning --add-label tehas:question
  fi
  if ! gh issue edit "$n" --repo "${repo:?}" "$@" >/dev/null 2>&1; then
    log "WARN: the labels of #$n could not be changed after its question was posted; the next poll tries again"
    return 1
  fi
  if ! plan_store plan-end "$n" --posted; then
    log "WARN: the record of #$n could not be updated ($tehas_store_word)"
    return 1
  fi
  rm -f "$st.out"
}

# A refine that filed is over: pi-rukas files once a session. The mark on the
# original is the step a poll that failed halfway does not repeat, once every
# filed issue says Replaces: a mark of an earlier session marks nothing here.
plan_replace() {
  local n=$1 url m body id have mark="" other later=0 mine="" said="" text new=0
  have=$(gh issue view "$n" --repo "$repo" --json labels 2>/dev/null | jq -r '[(.labels // [])[].name] | join(" ")') ||
    { log "WARN: the labels of #$n could not be read; the next poll replaces it"; return 1; }
  for url in ${2//,/ }; do
    m=${url##*/}
    body=${url%/issues/*}
    if [ "${body,,}" != "https://github.com/${repo,,}" ]; then
      said="$said, $url"
      later=1
      continue
    fi
    mine="$mine #$m"
    said="$said, #$m"
    if ! body=$(gh issue view "$m" --repo "$repo" --json body 2>/dev/null | jq -er '.body // ""'); then
      later=1
      log "WARN: could not read #$m, which refining #$n filed"
      continue
    fi
    plan_no_later_parts <<<"$body" || later=1
    [ "$(head -n 1 <<<"${body//$'\r'/}")" != "Replaces #$n" ] || continue
    new=1
    if ! gh issue edit "$m" --repo "$repo" --body "Replaces #$n"$'\n\n'"$body" >/dev/null 2>&1; then
      log "WARN: could not say on #$m that it replaces #$n; the next poll replaces it"
      return 1
    fi
    if ! id=$(gh api "repos/$repo/issues/$m" 2>/dev/null | jq -er .id) ||
      ! gh api -X POST "repos/$repo/issues/$n/sub_issues" -F sub_issue_id="$id" >/dev/null 2>&1; then
      log "WARN: could not link #$m as a sub-issue of #$n"
    fi
  done
  [ -n "$mine" ] || later=1
  [[ " $have " != *" tehas:refined "* ]] || mark=tehas:refined
  [[ " $have " != *" tehas:replaced "* ]] || mark=tehas:replaced
  if [ "$new" = 1 ] || [ -z "$mark" ]; then
    mark=tehas:replaced other=tehas:refined
    [ "$later" = 0 ] || { mark=tehas:refined other=tehas:replaced; }
    if ! gh issue edit "$n" --repo "$repo" --remove-label tehas:question --remove-label tehas:planning \
      --remove-label "$other" --add-label "$mark" >/dev/null 2>&1; then
      log "WARN: could not mark #$n $mark; the next poll replaces it"
      return 1
    fi
    text=$(jq -r '.text // "" | strings' "$(plan_state "$n").out" 2>/dev/null) || text=""
    say "$n" "Replaced by ${said#, }.${text:+

$text}"
  fi
  if [ "$mark" = tehas:replaced ]; then
    gh issue close "$n" --repo "$repo" --reason "not planned" >/dev/null 2>&1 || log "WARN: could not close #$n"
  fi
  plan_close "$n" || return 1
  log "planning #$n ended: $mark by $2"
}

# plan_no_later_parts: 0 only when the plan on stdin has a Later parts
# section (a heading, a bold line or "Later parts:") of nothing but None.
plan_no_later_parts() {
  tr -d '\r' | awk '
    function none(t) { t = tolower(t); gsub(/[-*_.:()[:space:]]/, "", t); return t == "" || t == "none" }
    {
      line = tolower($0); head = line
      sub(/^[#*_[:space:]]+/, "", head)
      if (head ~ /^later parts/) {
        seen = 1; in_it = 1; rest = substr(head, 12)
        sub(/^[*_[:space:]]+/, "", rest); sub(/^\([^)]*\)/, "", rest)
        if (!none(rest)) more = 1
        next
      }
      if (in_it && (line ~ /^#/ || line ~ /^\*\*[^*]+\*\*[[:space:]]*:?[[:space:]]*$/)) in_it = 0
      if (in_it && !none($0)) more = 1
    }
    END { exit (seen && !more) ? 0 : 1 }'
}

# A session Pi no longer has cannot go on: what was said in it is gone.
# plan_refined <issue> <label> <outcome>: one line in state/refined.jsonl for
# each issue a refining session filed, {at, source, filed, url}, so the
# analytics report can follow what became of refined issues. Only a session
# tehas:refine started counts. The ledger keeps the last 500 lines, as
# tehas_size_record keeps sizes.jsonl: the report reads the file whole within a
# bound, so a writer that never trims it eventually writes past what the report
# can read. A line that cannot be written is the report's loss only, and says so.
plan_refined() {
  local n=$1 label=$2 out=$3 lines ledger=$TEHAS_HOME/state/refined.jsonl
  [ "$label" = tehas:refine ] || return 0
  lines=$(jq -c --arg plan_filed_re "$plan_filed_re" --arg at "$(date -u +%FT%TZ)" --argjson n "$n" '
      (.filed // []) | if type == "array" then .[] else empty end | strings
      | select(test($plan_filed_re))
      | {at: $at, source: $n, filed: (capture("/issues/(?<k>[0-9]+)$").k | tonumber), url: .}' "$out" 2>/dev/null) || lines=""
  [ -n "$lines" ] || return 0
  printf '%s\n' "$lines" >>"$ledger" ||
    log "WARN: could not note in $ledger what refining #$n filed"
  # The trim of the other ledger: a trim that cannot be made leaves the file
  # longer, which is no loss of a filed issue.
  [ "$(wc -l <"$ledger" 2>/dev/null)" -gt 500 ] || return 0
  if tail -n 500 "$ledger" >"$ledger.new" 2>/dev/null; then
    mv "$ledger.new" "$ledger" 2>/dev/null ||
      log "WARN: could not trim $ledger to its last 500 lines"
  else
    rm -f "$ledger.new" 2>/dev/null
  fi
}

plan_lost() {
  plan_end "$1" "Tehas ended this planning session: pi-rukas has no record of it on \`$(hostname)\` any more, so the last answer was not given to the planner. Apply \`tehas:plan\` again to start a new session." lost "$(plan_hand)"
}

# plan_ended <issue> <args of plan-end>: the turn's end on record, and its
# exit status then taken off. 1, the status kept, when the store would not
# take it: the next poll collects the turn again.
plan_ended() {
  local n=$1
  shift
  if ! plan_store plan-end "$n" "$@"; then
    log "WARN: the end of the planning turn of #$n could not be recorded ($tehas_store_word); the next poll tries again"
    return 1
  fi
  rm -f "$(plan_state "$n").rc"
}

plan_finish() {
  local n=$1 rc=$2 turns=$(($3 + 1)) sid=$4 st out why label commit session=""
  local -a how=() more=()
  st=$(plan_state "$n")
  out=$st.out
  if ! tehas_store plan-show "$n"; then
    log "WARN: the planning record of #$n could not be read ($tehas_store_word); the next poll tries again"
    return 1
  fi
  label=$(tehas_store_get '.planning.label // ""') || label=""
  # In the factory's analytics, before the end is on record: how the unit
  # ended, as one word, and the image's pi-rukas commit.
  if [ "${TEHAS_ANALYTICS:-}" = on ]; then
    commit=$(tehas_rukas_commit)
    tehas_analytics_plan "$n" --rc "${rc// /-}" ${commit:+--rukas-image "$commit"}
  fi
  case "$rc" in
    0) how=(--outcome finished --rc 0) ;;
    interrupted) how=(--outcome interrupted) ;;
    "not started") how=(--outcome not-started) ;;
    "not readable") how=(--outcome not-readable) ;;
    *) how=(--outcome failed --rc "$rc") ;;
  esac
  if [ "$rc" = interrupted ]; then
    why="was cut off, for example by a restart of the host"
  elif [ "$rc" = "not started" ]; then
    why="could not be started as a unit"
  elif [ "$rc" = "not readable" ]; then
    why="left an exit status that cannot be read"
  else
    why="exited with status $rc"
  fi

  # On record first, so a poll that cannot end it yet ends it next time.
  if [ "$rc" != 0 ] && jq -e '.lost == true' "$out" >/dev/null 2>&1; then
    [[ ! $rc =~ ^[0-9]+$ ]] || more=(--rc "$rc")
    plan_ended "$n" --turn "$turns" --outcome lost "${more[@]}" || return 1
    log "planning #$n: Pi no longer has session $sid"
    plan_lost "$n" || return 1
    return 0
  fi

  if [ "$rc" = 0 ]; then
    session=$(jq -r '.session | if type == "string" then . else "" end' "$out" 2>/dev/null) || session=""
    plan_sid_ok "$session" || session=""
  fi
  if [ -n "$session" ]; then
    # The options on record are the ones q_render posts, in the same order.
    # What the turn filed is added to what the session filed before. Each
    # value ends in a NUL, so no text of the runner's is split into words.
    mapfile -d '' -t more < <(jq -j --arg plan_filed_re "$plan_filed_re" --argjson max "${Q_MAX_OPTIONS:?}" '
        ((.question.options // []) | if type == "array" then .[0:$max][] else empty end
          | "--option", ((if type == "object" then .label else null end) // "-" | tostring | gsub("[\r\n]"; " ") | gsub("\u0000"; ""))),
        ((.filed // []) | if type == "array" then .[] else empty end | strings
          | select(test($plan_filed_re)) | "--filed", .)
        | . + "\u0000"' "$out" 2>/dev/null)
    plan_ended "$n" --turn "$turns" "${how[@]}" --session "$session" "${more[@]}" --out "$out" || return 1
    plan_brief_rm "$n"
    plan_refined "$n" "$label" "$out"
    plan_post "$n" || true
    log "done planning #$n turn=$turns rc=$rc"
    return 0
  fi
  # A first turn that failed may still have made its session: the runner
  # then leaves the id, and nothing else, where the outcome would be. A turn
  # that exited 0 with no session in its outcome is a failed turn of the
  # session on record.
  if [ -z "$sid" ]; then
    sid=$(jq -r '.session | if type == "string" then . else "" end' "$out" 2>/dev/null) || sid=""
  fi
  plan_sid_ok "$sid" || sid=""
  if [ -n "$sid" ]; then
    # The session exists, so a reply can run the turn again. Its brief, what
    # it filed and who answered stay on record.
    [ "$rc" = 0 ] || more=(--session "$sid")
    plan_ended "$n" --turn "$turns" "${how[@]}" "${more[@]}" --out "$out" || return 1
    plan_brief_rm "$n"
    jq -n --arg text "This planning turn did not finish: it $why. The log is \`~/tehas/logs/dispatch-plan-$n.log\` on \`$(hostname)\`. Reply in a comment to run the turn again with your reply as the next message, or end the session and apply \`tehas:plan\` to start a new one." \
      '{text: $text, question: null}' >"$out"
    plan_post "$n" || true
    log "done planning #$n turn=$turns rc=$rc"
    return 0
  fi
  # No session came into being: there is nothing to continue. The session
  # stays running on record until the issue is visibly handed to a person. A
  # turn cut off before the issue was marked still carries tehas:plan, which
  # would start a session again with nobody asking for it.
  if ! gh issue edit "$n" --repo "${repo:?}" --remove-label tehas:plan --remove-label tehas:planning --add-label needs-human-attention >/dev/null 2>&1; then
    log "WARN: could not hand #$n to a person after a planning turn that left no session; the next poll tries again"
    return 1
  fi
  # An end with no session ends the session on record. It stays readable until
  # the next tehas:plan, so an export of this turn still holds it.
  plan_ended "$n" --turn "$turns" "${how[@]}" || return 1
  say "$n" "The planning session could not start (the runner $why). The log is \`~/tehas/logs/dispatch-plan-$n.log\` on \`$(hostname)\`. Apply \`tehas:plan\` again to retry."
  plan_files_rm "$n"
  log "done planning #$n turn=$turns rc=$rc, no session"
}

plan_recover() {
  local n=$1 st
  st=$(plan_state "$n")
  rm -f "$st.out"
  if ! tehas_store plan-show "$n"; then
    log "WARN: the planning record of #$n could not be read ($tehas_store_word); the next poll tries again"
    return 1
  fi
  if [ "$(tehas_store_get '.planning.lost // false')" = true ]; then
    plan_lost "$n"
    return
  fi
  log "planning #$n: a turn was cut off"
  plan_finish "$n" interrupted "$(tehas_store_get '.planning.turns // 0')" "$(tehas_store_get '.planning.session // ""')"
}

# plan_refused <issue> <what>: a turn the store would not begin. A refusal
# (a turn running on record, or one of another number) is this issue's to
# wait out, and returns 1; busy ends the poll for the next one to try again;
# a store that cannot be used ends it with 78.
# shellcheck disable=SC2154  # lib/store.sh, which the caller sources, sets these
plan_refused() {
  if [ "$tehas_store_rc" = 1 ]; then
    case $tehas_store_word in
      busy | timeout)
        log "ERROR: could not write the record of #$1, not starting a turn ($tehas_store_word)"
        exit 1
        ;;
    esac
    log "skip #$1: $2: $tehas_store_line"
    return 1
  fi
  log "ERROR: the state store cannot be used: $tehas_store_line"
  exit 78
}

plan_turn() {
  local n=$1 mode=$2 input=$3 comment=${4:-} by=${5:-} st out keep turns=0 sid="" what text
  local -a args undo
  st=$(plan_state "$n")
  out=$st.out
  keep=$(plan_keep "$n")
  # On record before the turn starts, and before the issue is marked, so that
  # a turn cut off at any point is found by the next poll. A mark without a
  # record would leave an issue that no poll picks up again. The question just
  # answered is answered on record in the same write, and none is open: a
  # reply to it must not be read a second time.
  if [ "$mode" = first ]; then
    if ! plan_store plan-begin "$n" --by "${who:?}" --label "${plabel:-tehas:plan}" --brief-file "$input" ${plan_auto:+--auto}; then
      rm -f "$input"
      plan_refused "$n" "its planning session could not begin"
      return
    fi
    # A session it replaced takes its files with it. The brief stays for the
    # turn's unit.
    rm -rf "$out" "$st.rc" "$keep"
    undo=(plan-begin "$n" --undo)
  else
    if ! tehas_store plan-show "$n"; then
      rm -f "$input"
      plan_refused "$n" "its planning record could not be read"
      return
    fi
    turns=$(tehas_store_get '.planning.turns // 0') || turns=0
    sid=$(tehas_store_get '.planning.session // ""') || sid=""
    if [ "$turns" -ge "$TEHAS_PLAN_MAX_TURNS" ]; then
      rm -f "$input"
      plan_end "$n" "This planning session reached its limit of $TEHAS_PLAN_MAX_TURNS turns and was ended. Apply \`tehas:plan\` to start a new one." "" "$(plan_hand)"
      return 0
    fi
    args=(plan-answer "$n" --turn "$((turns + 1))" --by "$by" --text-file "$input")
    ! tehas_millwright_is "$by" || args+=(--fixer)
    if ! plan_store "${args[@]}"; then
      rm -f "$input"
      plan_refused "$n" "the answer could not begin a turn"
      return
    fi
    rm -f "$input"
    undo=(plan-answer "$n" --turn "$((turns + 1))" --undo)
    # The answer the turn is given is the one on record. The issues the session
    # has filed go at the top of it: a filing whose result never reached the
    # model would otherwise be done again.
    what=$(tehas_store_get '(.planning.filed // []) | map(strings) | unique | join(", ")') || what=""
    text=$(tehas_store_get '.planning.answer // ""') || text=""
    if ! { plan_keep_make "$keep" && if [ -n "$what" ]; then
      printf 'Already filed in this session: %s\n\n%s\n' "$what" "$text"
    else
      printf '%s\n' "$text"
    fi >"$keep/answer.md"; }; then
      plan_store "${undo[@]}" || log "WARN: could not put back the record of #$n ($tehas_store_word)"
      log "ERROR: could not write the answer for #$n, not starting a turn"
      exit 1
    fi
    input=$keep/answer.md
  fi
  # The turn runs only once the issue is marked as taken, as a cycle does.
  # Only the label that started the session is taken off: removing a label the
  # repository does not have yet would fail the edit.
  if ! gh issue edit "$n" --repo "${repo:?}" --add-label tehas:planning --remove-label "${plabel:-tehas:plan}" \
    --remove-label tehas:question --remove-label needs-human-attention >/dev/null 2>&1; then
    # Nothing started: the record goes back to what it was, so a first turn
    # leaves none and a later one can still be answered.
    plan_store "${undo[@]}" || log "WARN: could not put back the record of #$n ($tehas_store_word)"
    [ "$mode" != first ] || rm -f "$input"
    log "ERROR: could not mark #$n as planning, not starting a turn"
    exit 1
  fi
  if [ -n "${plan_auto:-}" ] && [ "$mode" = first ]; then
    say "$n" "Tehas planning session started on \`$(hostname)\` at $(date -u +'%F %H:%M') UTC to refine this issue: its cycle parked as \`${plan_auto}\`, and the start of that cycle by @${who:?} authorizes one refine. Each turn ends in a comment here. Answer by ticking a box or by replying. To stop it, take \`tehas:question\` off while a question waits, or close the issue."
    args=(--brief "$input")
  elif [ "$mode" = first ]; then
    say "$n" "Tehas planning session started on \`$(hostname)\` at $(date -u +'%F %H:%M') UTC (authorized by @${who:?}'s \`${plabel:-tehas:plan}\` label). Each turn ends in a comment here. Answer by ticking a box or by replying."
    args=(--brief "$input")
  else
    args=(--session "$sid" --answer "$input")
    [ -z "$comment" ] || args+=(--comment "$comment")
  fi
  refresh_checkout
  rm -f "$out" "$st.rc"
  if ! plan_launch "$n" "${plan_cmd:?}" "$n" --out "$out" --keep "$keep" "${args[@]}"; then
    log "ERROR: could not start the planning turn of #$n"
    plan_finish "$n" "not started" "$turns" "$sid"
    return 0
  fi
  plan_collect "$n" || log "planning #$n: turn $((turns + 1)) runs as tehas-plan-$n.service"
}

# plan_launch <issue> <command>...: start the turn as its own unit and return.
# A poll is a oneshot service, and systemd stops what it leaves behind, so a
# turn started in the background would die with the poll. The unit holds the
# .lock until the turn has ended, writes the command's exit status to the .rc,
# and asks for a poll, which posts the outcome. A unit that finds the lock held
# runs nothing. systemd stops it a quarter of an hour after the turn's own
# limit. The poll's lock is not passed on, and the turn's lock stays with the
# unit's shell.
plan_launch() {
  local n=$1 st
  shift
  st=$(plan_state "$n")
  # shellcheck disable=SC2016  # expanded by the shell the unit runs
  set -- /bin/bash -c 'log=$1 rc=$2 again=$3 lock=$4; shift 4
    exec 9>>"$lock" && flock -n 9 || { echo "a planning turn of this issue is already running" >>"$log"; exit 75; }
    "$@" 9>&- >>"$log" 2>&1; echo "$?" >"$rc.new" && mv "$rc.new" "$rc"; : >"$again"' \
    plan-turn "$TEHAS_HOME/logs/dispatch-plan-$n.log" "$st.rc" "$TEHAS_HOME/state/dispatch.again" "$st.lock" "$@"
  if [ -n "${TEHAS_PLAN_START_CMD:-}" ]; then
    "$TEHAS_PLAN_START_CMD" "$n" "$@" 8>&-
    return
  fi
  # The unit has the user manager's environment, not the poll's: it loads
  # env.sh as the tehas units do.
  # shellcheck disable=SC2016  # expanded by the shell the unit runs
  systemd-run --user --quiet --collect --unit "tehas-plan-$n" -p "RuntimeMaxSec=$((TEHAS_PLAN_TIMEOUT + 900))" \
    /bin/bash -c '[ ! -r "$HOME/tehas/env.sh" ] || . "$HOME/tehas/env.sh"; exec "$@"' env-sh "$@" 8>&- >/dev/null 2>&1 || return 1
  # A moment for the unit to take its lock, so the status this poll writes
  # next shows the turn.
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    [ ! -f "$st.rc" ] && flock -n "$st.lock" true 8>&- 2>/dev/null || return 0
    sleep 0.5
  done
}

# plan_in_flight <issue>: 0 while the issue's turn holds its lock, or its unit
# is running: a turn started before the lock was kept holds none. A turn that
# left its exit status has ended, though systemd may still show its unit for
# the instant it asks for a poll.
plan_in_flight() {
  local st state
  st=$(plan_state "$1")
  [ ! -f "$st.rc" ] || return 1
  if [ -f "$st.lock" ] && ! flock -n "$st.lock" true 8>&- 2>/dev/null; then return 0; fi
  state=$(systemctl --user show -p ActiveState --value "tehas-plan-$1.service" 2>/dev/null) || return 1
  case "$state" in active | activating | deactivating | reloading) return 0 ;; esac
  return 1
}

# plan_collect <issue>: finish a turn whose unit wrote its exit status, and
# post its outcome. Returns 1, changing nothing, when there is no status yet.
# A turn whose end could not be recorded keeps its status for the next poll.
plan_collect() {
  local n=$1 st rc
  st=$(plan_state "$n")
  [ -f "$st.rc" ] || return 1
  rc=$(cat "$st.rc" 2>/dev/null) || rc=""
  [[ $rc =~ ^[0-9]+$ ]] || rc="not readable"
  if ! tehas_store plan-show "$n"; then
    log "WARN: the planning record of #$n could not be read ($tehas_store_word); the next poll tries again"
    return 0
  fi
  plan_finish "$n" "$rc" "$(tehas_store_get '.planning.turns // 0')" "$(tehas_store_get '.planning.session // ""')" || true
  return 0
}
