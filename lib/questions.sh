# shellcheck shell=bash
# Questions the factory asks on an issue, and the answers it accepts. Text in,
# text out: nothing here calls GitHub, so every function can be tested with a
# string. Sourced by libexec/cycle.
#
#   q_clean [max characters]         stdin to stdout: an agent's words, safe to
#                                    post, cut at the limit [60000]
#   q_render <kind> <issue> <turn> <result.json> [line]
#                                    the body of a question comment (a park's
#                                    has no retry box when .retry is false, and
#                                    a plan's leaves out .text: the poll says it
#                                    in a comment before this one); a "marker"
#                                    of pi-rukas's handoff on top, a "park"
#                                    marker above the question's own; <line> is
#                                    the host's own, above the boxes
#   q_park_repeat <count> [step] [cap]
#                                    the host's line of a park that repeats the
#                                    one before it, given as <line> of q_render.
#                                    Nothing for a count of 0 or an unreadable one
#
# <kind> is plan (a planning turn) or park (a cycle that stopped).

# What an agent wrote goes to a public issue. Tokens, database passwords,
# private IPv4 and IPv6 unique-local addresses and tailnet names are masked: a copy in sed of the
# console's rules for those (console/core/src/mask.rs), so this file needs no
# program beside it. Three more things are taken out because the comment is
# parsed again later or read by people: a comment marker, a task-list box, and
# an @ that would notify somebody. A box becomes a glyph: a backslash in front
# of it is what makes GitHub lose count of the real boxes. Carriage returns go
# first: the web interface turns a lone one into a line end, and the box rules
# only look at line starts, so a lone one would start a box.
q_clean() {
  local zw=$'\xe2\x80\x8b' max=${1:-60000}
  sed -E -e 's/\r//g' \
    -e 's/(^|[^A-Za-z0-9_])(gh[pousr]_|github_pat_)[A-Za-z0-9_]{16,}/\1\2***/g' \
    -e 's#(postgres(ql)?://[^:[:space:]/]+:)[^@[:space:]]+@#\1***@#g' \
    -e 's/(^|[^A-Z0-9_])([A-Z0-9_]*(TOKEN|SECRET|PASSWORD|API_KEY)[A-Z0-9_]*=)[^[:space:]]+/\1\2***/g' \
    -e 's/(^|[^0-9.])(10\.[0-9]{1,3}|192\.168|172\.(1[6-9]|2[0-9]|3[01])|169\.254|100\.(6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7]))\.[0-9]{1,3}\.[0-9]{1,3}/\1[private address]/g' \
    -e 's/(^|[^0-9A-Fa-f:])[fF][cCdD][0-9A-Fa-f]{2}(:[0-9A-Fa-f]{0,4}){2,}/\1[private address]/g' \
    -e 's/[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)*\.ts\.net/[private name]/g' \
    -e 's/<!--/<\\!--/g' \
    -e 's/^([[:space:]>]*(([-*+]|[0-9]+[.)])[[:space:]>]+)+)\[ \]/\1☐/' \
    -e 's/^([[:space:]>]*(([-*+]|[0-9]+[.)])[[:space:]>]+)+)\[[xX]\]/\1☑/' \
    -e "s/@([A-Za-z0-9])/@$zw\\1/g" |
    jq -Rrs --argjson max "$max" '.[0:$max] | sub("\n+$"; "")'
}

# GitHub refuses a comment of more than 65 536 characters, and a question that
# cannot be posted would be tried again on every poll for ever. So each part
# has a limit of its own and the whole always fits: the message 40 000, the
# question 4 000, and at most Q_MAX_OPTIONS options of 300 for the label and
# 600 for the description. That is under 63 000 with the fixed lines. The
# record of a planning session lists the same first Q_MAX_OPTIONS options, so
# a ticked box and its option keep the same place.
Q_MAX_OPTIONS=20

# A park's retry boxes; test/questions_test.sh holds them to console/core.
Q_BOX_CLEAN='Retry from a clean start'
Q_BOX_ALONE='Retry from the issue alone'

q_render() {
  local kind=$1 issue=$2 turn=$3 file=$4 line=${5:-} n i label desc marker
  # A park may carry pi-rukas's handoff marker, put back as its first line.
  # Only that marker of only this issue: the text is an agent's, the marker
  # is made by the caller.
  if [ "$kind" != plan ]; then
    marker=$(jq -r '.marker // "" | strings' "$file") || return 1
    if [[ $marker =~ ^\<!--\ pi-rukas:handoff\ issue=$issue\ cap=[A-Za-z0-9_.:-]{1,80}\ --\>$ ]]; then
      printf '%s\n' "$marker"
    fi
    jq -r '.text // ""' "$file" | q_clean 40000 || return 1
    printf '\n'
  fi
  if [ "$(jq -r '.question != null' "$file")" = true ]; then
    jq -r '.question.question // ""' "$file" | q_clean 4000
    printf '\n'
  fi
  # The host's own line, so it is not run through the masking an agent's words
  # go through. It holds no address of the host and no box.
  if [ -n "$line" ]; then
    printf '%s\n\n' "$line"
  fi
  n=$(jq '(.question.options // []) | length' "$file") || return 1
  if [ "$n" -gt "$Q_MAX_OPTIONS" ]; then
    printf 'Options left out: %s. A reply can name another choice.\n\n' "$((n - Q_MAX_OPTIONS))"
    n=$Q_MAX_OPTIONS
  fi
  for ((i = 0; i < n; i++)); do
    label=$(jq -r --argjson i "$i" '.question.options[$i].label // "" | gsub("[\r\n]+"; " ")' "$file" | q_clean 300)
    desc=$(jq -r --argjson i "$i" '.question.options[$i].description // "" | gsub("[\r\n]+"; " ")' "$file" | q_clean 600)
    printf -- '- [ ] %s\n' "$label"
    [ -z "$desc" ] || printf '  %s\n' "$desc"
  done
  case "$kind" in
    plan)
      printf -- '- [ ] End this planning session\n\n'
      printf 'Tick one box, or reply in a comment. Only an answer from an account that may authorise work counts.\n'
      ;;
    park)
      [ "$(jq '.retry != false' "$file")" = false ] || printf -- '- [ ] %s\n- [ ] %s\n' "$Q_BOX_CLEAN" "$Q_BOX_ALONE"
      printf -- '- [ ] Leave it parked\n\n'
      printf 'Tick one box. Only a tick from an account that may authorise work counts.\n'
      ;;
    *) return 1 ;;
  esac
  printf '\n'
  marker=$(jq -r '.park // "" | strings' "$file") || return 1
  if [[ $marker =~ ^\<!--\ tehas:park\ issue=$issue\ step=[a-z0-9-]{0,40}\ kind=[a-z0-9-]{0,40}\ provider=[01]\ sig=[0-9a-f]{64}\ --\>$ ]]; then
    printf '%s\n' "$marker"
  fi
  printf '<!-- tehas:question kind=%s issue=%s turn=%s -->\n' "$kind" "$issue" "$turn"
}

# The one line a repeating park says of itself, in the host's own words: the
# count of the run, the step and the cap the cycle already has, each reduced as
# the park marker reduces them, so nothing of a failure's free text reaches the
# comment. It asks nothing: the tick of a retry stays the park's own ask.
q_park_repeat() { # <count> [step] [cap]
  local n=$1 step=${2:-} cap=${3:-} line tick
  [[ $n =~ ^[1-9][0-9]{0,3}$ ]] || return 0
  step=${step//[!a-z0-9-]/} cap=${cap//[!A-Za-z0-9_.:-]/}
  step=${step:0:40} cap=${cap:0:80}
  tick='`'
  line=" This park repeats the one before it: number ${n} of the parks in a row that stopped"
  [ -z "$step" ] || line+=" at step ${tick}${step}${tick}"
  [ -z "$cap" ] || line+=" on the cap ${tick}${cap}${tick}"
  printf '%s. A retry on the same text will likely stop the same way and spend another slot: change the issue first, or ask for a smaller change with %stehas:plan%s.\n' \
    "$line" "$tick" "$tick"
}
