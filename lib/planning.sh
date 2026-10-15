# shellcheck shell=bash
# The steps of a planning session. Sourced by libexec/dispatch, and written
# for it: these functions use the dispatcher's own helpers (log, say,
# post_question, refresh_checkout, wait_for), its variables (repo, plan_cmd,
# who, posted) and Q_MAX_OPTIONS of lib/questions.sh.
#
# A session is a Pi session that lives across polls. Its record is
# $TEHAS_HOME/state/plan-<issue>.json:
#   session   the Pi session id, to reopen it
#   turns     how many turns have run
#   options   the labels of the choices posted in the open question, in order
#   pending   true while a turn's outcome has not been posted yet
#   failed    true when the last turn did not finish
#   running   true while a turn is in flight
#   comment   the id of the open question comment, once it is posted
#   lost      true when Pi no longer has the session; it is then ended
# While pending, the outcome is kept next to the record as <record>.out. What
# the runner carries from one turn to the next is in <record>.keep, and the
# approved text in <record>.brief; both go with the record.
#
# A turn is written down as running before it starts, and before the issue is
# marked as planning. The dispatcher holds the one dispatch lock, so a running
# record found at the start of a poll is a turn that was cut off, for example
# by a restart of the host. plan_recover turns it into a failed turn that asks
# again, or ends a first turn that never produced a session.
#
# Each change on GitHub is checked, and the record only moves on once it has
# worked, so a poll that failed halfway is repeated by the next one without
# posting twice or announcing twice.
#
#   plan_state <issue>             print the path of the record
#   plan_record_number <file>      put the issue of a record file in $plan_n, or
#                                  return 1 for a file that is not a readable record
#   plan_turn <issue> first|next <file> [<comment id>]
#                                  run one turn and post its outcome; the id
#                                  is that of the reply <file> holds, whose
#                                  attachments the turn is given
#   plan_finish <issue> <rc> <turns before> <session before>
#                                  record how a turn went and post it; a failed
#                                  first turn takes its session from the
#                                  outcome file, if the runner left one there
#   plan_recover <issue>           finish a turn that was cut off, or end a
#                                  session found lost that could not be ended
#   plan_post <issue>              post the outcome kept in the record
#   plan_end <issue> <text>        end the session and say why; returns 1,
#                                  having changed nothing, if the labels stay

plan_state() { printf '%s/state/plan-%s.json\n' "$TEHAS_HOME" "$1"; }

# Files in the state directory that are not a session record are left alone.
# One warning per poll for an unreadable record.
plan_bad=" "
plan_n=""
plan_record_number() {
  plan_n=${1##*/plan-}
  plan_n=${plan_n%.json}
  case "$plan_n" in '' | *[!0-9]*) return 1 ;; esac
  if ! jq -e . "$1" >/dev/null 2>&1; then
    case "$plan_bad" in
      *" $plan_n "*) ;;
      *)
        log "WARN: the planning record of #$plan_n is not readable, leaving it alone"
        plan_bad="$plan_bad$plan_n "
        ;;
    esac
    return 1
  fi
}

plan_end() {
  local n=$1 st
  st=$(plan_state "$n")
  if ! gh issue edit "$n" --repo "${repo:?}" --remove-label tehas:question --remove-label tehas:planning \
    --remove-label tehas:plan --remove-label needs-human-attention >/dev/null 2>&1; then
    log "WARN: could not take the planning labels off #$n, so its session is not ended yet"
    return 1
  fi
  say "$n" "$2"
  rm -rf "$st" "$st.out" "$st.answer" "$st.brief" "$st.keep"
  log "planning #$n ended"
}

plan_post() {
  local n=$1 st turns
  st=$(plan_state "$n")
  turns=$(jq -r '.turns // 0' "$st" 2>/dev/null) || turns=0
  # Step one: the question. Once its id is on record it is never posted again.
  if [ -z "$(jq -r '.comment // ""' "$st" 2>/dev/null)" ]; then
    if ! post_question plan "$n" "$turns" "$st.out"; then
      log "WARN: the planning turn of #$n could not be posted; the next poll tries again"
      return 1
    fi
    if ! { jq -c --argjson c "${posted:?}" '.comment = $c' "$st" >"$st.new" && mv "$st.new" "$st"; }; then
      log "WARN: the record of #$n could not be updated after its question was posted"
      return 1
    fi
  fi
  # Step two: show the question. Retried alone if it fails.
  if [ "$(jq -r '.failed // false' "$st" 2>/dev/null)" = true ]; then
    set -- --remove-label tehas:planning --add-label tehas:question --add-label needs-human-attention
  else
    set -- --remove-label tehas:planning --add-label tehas:question
  fi
  if ! gh issue edit "$n" --repo "${repo:?}" "$@" >/dev/null 2>&1; then
    log "WARN: the labels of #$n could not be changed after its question was posted; the next poll tries again"
    return 1
  fi
  if jq -c '.pending = false' "$st" >"$st.new" && mv "$st.new" "$st"; then
    rm -f "$st.out"
  else
    log "WARN: the record of #$n could not be updated"
    return 1
  fi
}

# A session Pi no longer has cannot go on: what was said in it is gone.
plan_lost() {
  plan_end "$1" "Tehas ended this planning session: its conversation is no longer on \`$(hostname)\` (\`~/.pi/agent/sessions\`), so the last answer was not sent to the agent. Apply \`tehas:plan\` again to start a new session."
}

plan_finish() {
  local n=$1 rc=$2 turns=$(($3 + 1)) sid=$4 st out why ok
  st=$(plan_state "$n")
  out=$st.out
  if [ "$rc" = interrupted ]; then
    why="was cut off, for example by a restart of the host"
  else
    why="exited with status $rc"
  fi

  # Noted first, so a poll that cannot end it yet ends it next time.
  if [ "$rc" != 0 ] && jq -e '.lost == true' "$out" >/dev/null 2>&1; then
    if ! { jq -c '.lost = true' "$st" >"$st.new" && mv "$st.new" "$st"; }; then
      log "WARN: could not note in the record of #$n that its session is lost"
    fi
    log "planning #$n: Pi no longer has session $sid"
    plan_lost "$n" || return 1
    return 0
  fi

  if [ "$rc" = 0 ] && jq -e '(.session // "") != ""' "$out" >/dev/null 2>&1; then
    ok=1
  else
    ok=0
    # A first turn that failed may still have made its session: the runner
    # then leaves the id, and nothing else, where the outcome would be.
    if [ -z "$sid" ]; then
      sid=$(jq -r '.session | if type == "string" then . else "" end' "$out" 2>/dev/null) || sid=""
    fi
  fi
  if [ "$ok" = 1 ]; then
    # The options on record are the ones q_render posts, in the same order.
    jq -c --argjson turns "$turns" --argjson max "${Q_MAX_OPTIONS:?}" \
      '{session, turns: $turns, options: [(.question.options // [])[0:$max][]?.label], pending: true, failed: false, running: false}' "$out" >"$st.new" &&
      mv "$st.new" "$st"
  elif [ -n "$sid" ]; then
    # The session exists, so a reply can run the turn again.
    jq -cn --arg session "$sid" --argjson turns "$turns" \
      '{session: $session, turns: $turns, options: [], pending: true, failed: true, running: false}' >"$st.new" && mv "$st.new" "$st"
    jq -n --arg text "This planning turn did not finish: it $why. The log is \`~/tehas/logs/dispatch-plan-$n.log\` on \`$(hostname)\`. Reply in a comment to run the turn again with your reply as the next message, or end the session and apply \`tehas:plan\` to start a new one." \
      '{text: $text, question: null}' >"$out"
  else
    # No session came into being: there is nothing to continue. The record
    # stays, still running, until the issue is visibly handed to a person.
    # A turn cut off before the issue was marked still carries tehas:plan,
    # which would start a session again with nobody asking for it.
    if ! gh issue edit "$n" --repo "${repo:?}" --remove-label tehas:plan --remove-label tehas:planning --add-label needs-human-attention >/dev/null 2>&1; then
      log "WARN: could not hand #$n to a person after a planning turn that left no session; the next poll tries again"
      return 1
    fi
    say "$n" "The planning session could not start (the runner $why). The log is \`~/tehas/logs/dispatch-plan-$n.log\` on \`$(hostname)\`. Apply \`tehas:plan\` again to retry."
    rm -rf "$st" "$out" "$st.answer" "$st.brief" "$st.keep"
    log "done planning #$n turn=$turns rc=$rc, no session"
    return 0
  fi
  plan_post "$n" || true
  log "done planning #$n turn=$turns rc=$rc"
}

plan_recover() {
  local n=$1 st
  st=$(plan_state "$n")
  rm -f "$st.out"
  if [ "$(jq -r '.lost // false' "$st" 2>/dev/null)" = true ]; then
    plan_lost "$n"
    return
  fi
  log "planning #$n: a turn was cut off"
  plan_finish "$n" interrupted "$(jq -r '.turns // 0' "$st" 2>/dev/null)" "$(jq -r '.session // ""' "$st" 2>/dev/null)"
}

plan_turn() {
  local n=$1 mode=$2 input=$3 comment=${4:-} st out turns sid rc was written
  local -a args
  st=$(plan_state "$n")
  out=$st.out
  turns=$(jq -r '.turns // 0' "$st" 2>/dev/null) || turns=0
  sid=$(jq -r '.session // ""' "$st" 2>/dev/null) || sid=""
  if [ "$turns" -ge "$TEHAS_PLAN_MAX_TURNS" ]; then
    plan_end "$n" "This planning session reached its limit of $TEHAS_PLAN_MAX_TURNS turns and was ended. Apply \`tehas:plan\` to start a new one."
    return 0
  fi
  # Written down before the turn starts, and before the issue is marked, so
  # that a turn cut off at any point is found by the next poll. A mark without
  # a record would leave an issue that no poll picks up again. The record has
  # no question comment: the next one is not posted yet, and the one just
  # answered must not be read a second time.
  written=1
  if [ "$mode" = first ]; then
    was=""
    echo '{"session":"","turns":0,"options":[],"pending":false,"failed":false,"running":true}' >"$st.new" || written=0
  else
    was=$(cat "$st" 2>/dev/null) || was=""
    jq -c '.running = true | .pending = false | del(.comment)' "$st" >"$st.new" || written=0
  fi
  # An empty record would read as a session with no turns and no question.
  if ! { [ "$written" = 1 ] && [ -s "$st.new" ] && mv "$st.new" "$st"; }; then
    log "ERROR: could not write the record of #$n, not starting a turn"
    exit 1
  fi
  # The turn runs only once the issue is marked as taken, as a cycle does.
  if ! gh issue edit "$n" --repo "${repo:?}" --add-label tehas:planning --remove-label tehas:plan \
    --remove-label tehas:question --remove-label needs-human-attention >/dev/null 2>&1; then
    # Nothing started: the record goes back to what it was, so a first turn
    # leaves none and a later one can still be answered.
    if [ -n "$was" ]; then
      printf '%s\n' "$was" >"$st" || log "WARN: could not put back the record of #$n"
    else
      rm -f "$st"
    fi
    log "ERROR: could not mark #$n as planning, not starting a turn"
    exit 1
  fi
  if [ "$mode" = first ]; then
    say "$n" "Tehas planning session started on \`$(hostname)\` at $(date -u +'%F %H:%M') UTC (authorized by @${who:?}'s \`tehas:plan\` label). Each turn ends in a comment here. Answer by ticking a box or by replying."
    args=(--brief "$input")
  else
    args=(--session "$sid" --answer "$input")
    [ -z "$comment" ] || args+=(--comment "$comment")
  fi
  refresh_checkout
  rm -f "$out"
  # The poll's lock is not passed on: a turn's children that outlive a poll
  # killed outright must not stop every later poll.
  "${plan_cmd:?}" "$n" --out "$out" --keep "$st.keep" "${args[@]}" >>"$TEHAS_HOME/logs/dispatch-plan-$n.log" 2>&1 8>&- &
  wait_for $!
  rc=$?
  plan_finish "$n" "$rc" "$turns" "$sid"
}
