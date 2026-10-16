# shellcheck shell=bash
# The steps of a planning session. Sourced by libexec/dispatch, and written
# for it: these functions use the dispatcher's own helpers (log, say,
# post_question, refresh_checkout), its variables (repo, plan_cmd, who,
# posted), Q_MAX_OPTIONS of lib/questions.sh and tehas_fixer_is of
# lib/fixer.sh.
#
# A session is a Pi session that lives across polls. Its record is
# $TEHAS_PROJECT_HOME/state/plan-<issue>.json:
#   session   the Pi session id, to reopen it
#   turns     how many turns have run
#   options   the labels of the choices posted in the open question, in order
#   pending   true while a turn's outcome has not been posted yet
#   failed    true when the last turn did not finish
#   running   true while a turn is in flight
#   comment   the id of the open question comment, once it is posted
#   lost      true when Pi no longer has the session; it is then ended
#   by        the account whose tehas:plan started the session
#   answers   one {turn, by, fixer} per answer that ran a turn: the turn it
#             started, who gave it, and whether that was the fixer
#   filed     the issues the session's turns filed, oldest first: the addresses
#             of the issues the agent made, read from the turn's stream.
#             Kept when the record is rewritten, and named to the next turn.
# A record written before by, answers and filed were kept has none of them.
# While pending, the outcome is kept next to the record as <record>.out. What
# the runner carries from one turn to the next is in <record>.keep, and the
# approved text in <record>.brief; both go with the record.
#
# A turn is written down as running before it starts, and before the issue is
# marked as planning. It runs as its own unit, tehas-plan-<issue>.service, which
# leaves the turn's exit status in <record>.rc. A running record found at the
# start of a poll is a turn still in flight, one that ended (plan_collect posts
# it), or, with no unit and no status, one that was cut off, for example by a
# restart of the host. plan_recover turns that into a failed turn that asks
# again, or ends a first turn that never produced a session.
#
# Each change on GitHub is checked, and the record only moves on once it has
# worked, so a poll that failed halfway is repeated by the next one without
# posting twice or announcing twice.
#
#   plan_state <issue>             print the path of the record
#   plan_record_number <file>      put the issue of a record file in $plan_n, or
#                                  return 1 for a file that is not a readable record
#   plan_turn <issue> first|next <file> [<comment id>] [<login>]
#                                  start one turn as its own unit; its outcome
#                                  is posted when it has ended; the id
#                                  is that of the reply <file> holds, whose
#                                  attachments the turn is given, and the
#                                  login is who answered
#   plan_finish <issue> <rc> <turns before> <session before>
#                                  record how a turn went and post it; a failed
#                                  first turn takes its session from the
#                                  outcome file, if the runner left one there
#   plan_launch <issue> <command>...
#                                  start the turn's unit and return
#   plan_in_flight <issue>         0 while the turn's unit runs
#   plan_collect <issue>           post a turn that ended; 1 when none did
#   plan_recover <issue>           finish a turn that was cut off, or end a
#                                  session found lost that could not be ended
#   plan_post <issue>              post the outcome kept in the record
#   plan_end <issue> <text>        end the session and say why; returns 1,
#                                  having changed nothing, if the labels stay

plan_state() { printf '%s/state/plan-%s.json\n' "$TEHAS_PROJECT_HOME" "$1"; }

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

# plan_filed <record>: the issues its turns filed, as one line of addresses
# separated by ", ". An empty line when the session has filed none, which is
# what a record of a turn that filed nothing, or one written before this was
# kept, gives. A filed that is not a list of strings is none.
plan_filed() {
  jq -r 'if (.filed | type) == "array" then
      [.filed[] | select(type == "string") | select(test("^https://github\\.com/[^/]+/[^/]+/issues/[0-9]+$"))]
      | unique | join(", ")
    else "" end' "$1" 2>/dev/null
}

plan_post() {
  local n=$1 st turns what
  st=$(plan_state "$n")
  turns=$(jq -r '.turns // 0' "$st" 2>/dev/null) || turns=0
  # What the turn filed is said once, under the agent's own words, and kept on
  # the record so the next turn's answer can name it: an issue the agent never
  # saw the result of would otherwise be filed again.
  what=$(plan_filed "$st.out") || what=""
  # Step one: the question. Once its id is on record it is never posted again.
  if [ -z "$(jq -r '.comment // ""' "$st" 2>/dev/null)" ]; then
    if ! post_question plan "$n" "$turns" "$st.out" "${what:+Filed in this turn: $what}"; then
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
  local n=$1 rc=$2 turns=$(($3 + 1)) sid=$4 st out why ok kept
  st=$(plan_state "$n")
  out=$st.out
  # Rewritten below from the outcome, so the session's own fields are carried.
  kept=$(jq -c '{by, answers} | with_entries(select(.value != null))' "$st" 2>/dev/null) || kept=""
  [ -n "$kept" ] || kept='{}'
  if [ "$rc" = interrupted ]; then
    why="was cut off, for example by a restart of the host"
  elif [ "$rc" = "not started" ]; then
    why="could not be started as a unit"
  elif [ "$rc" = "not readable" ]; then
    why="left an exit status that cannot be read"
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
    # What the turn filed is noted with them, and the issues the session
    # filed before are noted too: the record is rewritten from the outcome
    # alone, and the next turn's answer is made from what is left on record
    # once the outcome file is posted and removed.
    before=$(plan_filed "$st") || before=""
    jq -c --argjson turns "$turns" --argjson max "${Q_MAX_OPTIONS:?}" --argjson kept "$kept" \
      --arg before "$before" \
      '{session, turns: $turns, options: [(.question.options // [])[0:$max][]?.label], pending: true, failed: false, running: false}
       + $kept
       + (( ($before | split(", ") | map(select(. != ""))) +
             ((.filed // []) | if type == "array" then . else [] end
               | map(select(type == "string") | select(test("^https://github\\.com/[^/]+/[^/]+/issues/[0-9]+$"))))
           | unique) as $f | if ($f | length) == 0 then {} else {filed: $f} end)' "$out" >"$st.new" &&
      mv "$st.new" "$st"
  elif [ -n "$sid" ]; then
    # The session exists, so a reply can run the turn again.
    jq -cn --arg session "$sid" --argjson turns "$turns" --argjson kept "$kept" \
      '{session: $session, turns: $turns, options: [], pending: true, failed: true, running: false} + $kept' >"$st.new" && mv "$st.new" "$st"
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
  local n=$1 mode=$2 input=$3 comment=${4:-} by=${5:-} st out turns sid rc was written fixer=false
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
    jq -cn --arg by "${who:?}" '{session: "", turns: 0, options: [], pending: false, failed: false, running: true, by: $by}' >"$st.new" || written=0
  else
    was=$(cat "$st" 2>/dev/null) || was=""
    ! tehas_fixer_is "$by" || fixer=true
    # Answers on record that are not a list stay so: the fixer's grant then
    # refuses it, where a fresh list would count its answers from none.
    jq -c --argjson turn "$((turns + 1))" --arg by "$by" --argjson fixer "$fixer" \
      '.running = true | .pending = false | del(.comment)
       | if (.answers // []) | type == "array" then .answers = (.answers // []) + [{turn: $turn, by: $by, fixer: $fixer}] else . end' \
      "$st" >"$st.new" || written=0
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
    # The issues the session has filed go at the top of the answer: a filing
    # whose result never reached the model would otherwise be done again.
    # Written beside the answer the caller made, so that file stays the text as
    # it came and the turn is given the one with the line on it.
    what=$(plan_filed "$st") || what=""
    if [ -n "$what" ] && ! printf 'Already filed in this session: %s\n\n%s\n' "$what" "$(cat "$input")" >"$st.answer.new"; then
      rm -f "$st.answer.new"
      log "ERROR: could not write the answer for #$n, not starting a turn"
      exit 1
    fi
    [ -z "$what" ] || input=$st.answer.new
    args=(--session "$sid" --answer "$input")
    [ -z "$comment" ] || args+=(--comment "$comment")
  fi
  refresh_checkout
  rm -f "$out" "$st.rc"
  if ! plan_launch "$n" "${plan_cmd:?}" "$n" --out "$out" --keep "$st.keep" "${args[@]}"; then
    log "ERROR: could not start the planning turn of #$n"
    plan_finish "$n" "not started" "$turns" "$sid"
    return
  fi
  plan_collect "$n" || log "planning #$n: turn $((turns + 1)) runs as tehas-plan-$n.service"
}

# plan_launch <issue> <command>...: start the turn as its own unit and return.
# A poll is a oneshot service, and systemd stops what it leaves behind, so a
# turn started in the background would die with the poll. The unit writes the
# command's exit status to <record>.rc when it ends, and asks for a poll, which
# posts the outcome. systemd stops it a quarter of an hour after the turn's own
# limit. The poll's lock is not passed on.
plan_launch() {
  local n=$1 st
  shift
  st=$(plan_state "$n")
  # shellcheck disable=SC2016  # expanded by the shell the unit runs
  set -- /bin/bash -c 'log=$1 rc=$2 again=$3; shift 3; "$@" >>"$log" 2>&1; echo "$?" >"$rc.new" && mv "$rc.new" "$rc"; : >"$again"' \
    plan-turn "$TEHAS_PROJECT_HOME/logs/dispatch-plan-$n.log" "$st.rc" "$TEHAS_PROJECT_HOME/state/dispatch.again" "$@"
  if [ -n "${TEHAS_PLAN_START_CMD:-}" ]; then
    "$TEHAS_PLAN_START_CMD" "$n" "$@" 8>&-
    return
  fi
  # The unit has the user manager's environment, not the poll's: it loads
  # env.sh as the tehas units do.
  # shellcheck disable=SC2016  # expanded by the shell the unit runs
  systemd-run --user --quiet --collect --unit "tehas-plan-$n" -p "RuntimeMaxSec=$((TEHAS_PLAN_TIMEOUT + 900))" \
    /bin/bash -c '[ ! -r "$HOME/tehas/env.sh" ] || . "$HOME/tehas/env.sh"; exec "$@"' env-sh "$@" 8>&- >/dev/null 2>&1
}

# plan_in_flight <issue>: 0 while the unit of the issue's turn is running. A
# turn that left its exit status has ended, though systemd may still show its
# unit for the instant it asks for a poll.
plan_in_flight() {
  local state
  [ ! -f "$(plan_state "$1").rc" ] || return 1
  state=$(systemctl --user show -p ActiveState --value "tehas-plan-$1.service" 2>/dev/null) || return 1
  case "$state" in active | activating | deactivating | reloading) return 0 ;; esac
  return 1
}

# plan_collect <issue>: finish a turn whose unit wrote its exit status, and
# post its outcome. Returns 1, changing nothing, when there is no status yet.
plan_collect() {
  local n=$1 st rc
  st=$(plan_state "$n")
  [ -f "$st.rc" ] || return 1
  rc=$(cat "$st.rc" 2>/dev/null) || rc=""
  [[ $rc =~ ^[0-9]+$ ]] || rc="not readable"
  rm -f "$st.rc"
  plan_finish "$n" "$rc" "$(jq -r '.turns // 0' "$st" 2>/dev/null)" "$(jq -r '.session // ""' "$st" 2>/dev/null)"
  return 0
}
