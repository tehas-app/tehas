# shellcheck shell=bash
# The steps of a planning session. Sourced by bin/tehas-dispatch, and written
# for it: these functions use the dispatcher's own helpers (log, say, label,
# post_question, refresh_checkout, wait_for) and its variables (repo,
# plan_cmd, who, posted).
#
# A session is a Pi session that lives across polls. Its record is
# $TEHAS_HOME/state/plan-<issue>.json:
#   session   the Pi session id, to reopen it
#   turns     how many turns have run
#   options   the labels of the choices in the open question, in order
#   pending   true while a turn's outcome has not been posted yet
#   failed    true when the last turn did not finish
#   comment   the id of the open question comment
# While pending, the outcome is kept next to the record as <record>.out.
#
#   plan_state <issue>             print the path of the record
#   plan_turn <issue> first|next <file>
#                                  run one turn and post its outcome
#   plan_post <issue>              post the outcome kept in the record
#   plan_end <issue> <text>        end the session and say why

plan_state() { printf '%s/state/plan-%s.json\n' "$TEHAS_HOME" "$1"; }

plan_end() {
  local n=$1 st
  st=$(plan_state "$n")
  label "$n" --remove-label tehas:question --remove-label tehas:planning --remove-label tehas:plan
  say "$n" "$2"
  rm -f "$st" "$st.out" "$st.answer" "$st.brief"
  log "planning #$n ended"
}

plan_post() {
  local n=$1 st turns
  st=$(plan_state "$n")
  turns=$(jq -r '.turns // 0' "$st" 2>/dev/null) || turns=0
  if ! post_question plan "$n" "$turns" "$st.out"; then
    log "WARN: the planning turn of #$n could not be posted; the next poll tries again"
    return 1
  fi
  if [ "$(jq -r '.failed // false' "$st" 2>/dev/null)" = true ]; then
    label "$n" --remove-label tehas:planning --add-label tehas:question --add-label needs-human-attention
  else
    label "$n" --remove-label tehas:planning --add-label tehas:question
  fi
  if jq -c --argjson c "${posted:?}" '.comment = $c | .pending = false' "$st" >"$st.new"; then
    mv "$st.new" "$st"
  fi
  rm -f "$st.out"
}

plan_turn() {
  local n=$1 mode=$2 input=$3 st out turns sid rc
  local -a args
  st=$(plan_state "$n")
  out=$st.out
  turns=$(jq -r '.turns // 0' "$st" 2>/dev/null) || turns=0
  sid=$(jq -r '.session // ""' "$st" 2>/dev/null) || sid=""
  if [ "$turns" -ge "$TEHAS_PLAN_MAX_TURNS" ]; then
    plan_end "$n" "This planning session reached its limit of $TEHAS_PLAN_MAX_TURNS turns and was ended. Apply \`tehas:plan\` to start a new one."
    return 0
  fi
  # The turn runs only once the issue is marked as taken, as a cycle does.
  if ! gh issue edit "$n" --repo "${repo:?}" --add-label tehas:planning --remove-label tehas:plan \
    --remove-label tehas:question --remove-label needs-human-attention >/dev/null 2>&1; then
    log "ERROR: could not mark #$n as planning, not starting a turn"
    exit 1
  fi
  if [ "$mode" = first ]; then
    say "$n" "Tehas planning session started on \`$(hostname)\` at $(date -u +'%F %H:%M') UTC (authorized by @${who:?}'s \`tehas:plan\` label). Each turn ends in a comment here. Answer by ticking a box or by replying."
    args=(--brief "$input")
  else
    args=(--session "$sid" --answer "$input")
  fi
  refresh_checkout
  rm -f "$out"
  "${plan_cmd:?}" "$n" --out "$out" "${args[@]}" >>"$TEHAS_HOME/logs/dispatch-plan-$n.log" 2>&1 &
  wait_for $!
  rc=$?
  turns=$((turns + 1))

  if [ "$rc" = 0 ] && jq -e '(.session // "") != ""' "$out" >/dev/null 2>&1; then
    jq -c --argjson turns "$turns" \
      '{session, turns: $turns, options: [.question.options[]?.label], pending: true, failed: false}' "$out" >"$st.new" &&
      mv "$st.new" "$st"
  elif [ -n "$sid" ]; then
    # The session exists, so a reply can run the turn again.
    jq -cn --arg session "$sid" --argjson turns "$turns" \
      '{session: $session, turns: $turns, options: [], pending: true, failed: true}' >"$st.new" && mv "$st.new" "$st"
    jq -n --arg text "This planning turn did not finish (runner exit $rc). The log is \`~/tehas/logs/dispatch-plan-$n.log\` on \`$(hostname)\`. Reply in a comment to run the turn again with your reply as the next message, or end the session and apply \`tehas:plan\` to start a new one." \
      '{text: $text, question: null}' >"$out"
  else
    # No session came into being: there is nothing to continue.
    label "$n" --remove-label tehas:planning --add-label needs-human-attention
    say "$n" "The planning session could not start (runner exit $rc). The log is \`~/tehas/logs/dispatch-plan-$n.log\` on \`$(hostname)\`. Apply \`tehas:plan\` again to retry."
    rm -f "$st" "$out" "$st.brief"
    log "done planning #$n turn=$turns rc=$rc, no session"
    return 0
  fi
  plan_post "$n" || true
  log "done planning #$n turn=$turns rc=$rc"
}
