# shellcheck shell=bash
# Which cycles are running, and where a parked cycle's question is on record.
# Sourced by the dispatcher, by tehasbot cycle and by tehas index.
#
# tehasbot cycle holds $TEHAS_HOME/state/cycles/<issue>.lock for as long as it
# lives. A lock nobody holds is a cycle that ended, however it ended, so a
# cycle that died is never counted.
#
#   tehas_cycle_running <issue>   0 if that issue's cycle holds its lock
#   tehas_cycles_running          the issues with a running cycle, one per line
#   park_id <issue>               the path of the file that holds the id of
#                                 the park question this host posted
#
# Needs TEHAS_HOME. Reads only: it takes each lock for an instant and lets go.

tehas_cycle_running() {
  local lock=$TEHAS_HOME/state/cycles/$1.lock
  [ -e "$lock" ] && ! flock -n "$lock" true
}

tehas_cycles_running() {
  local lock n
  for lock in "$TEHAS_HOME"/state/cycles/*.lock; do
    [ -e "$lock" ] || continue
    n=${lock##*/}
    n=${n%.lock}
    if tehas_cycle_running "$n"; then printf '%s\n' "$n"; fi
  done
  return 0
}

# An agent in a container holds the bot's token: it can post a comment with a
# marker, edit the bot's comments and apply labels. It cannot write here, so
# this file is what makes a comment the factory's own question. One line, the
# comment's number. tehasbot cycle writes it, the dispatcher reads it.
park_id() { printf '%s/state/park-%s.id\n' "$TEHAS_HOME" "$1"; }
