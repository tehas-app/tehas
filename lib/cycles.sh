# shellcheck shell=bash
# Which cycles are running, where a parked cycle's question is on record, what
# a number on GitHub is, and what the sandbox image says about itself. Sourced
# by the dispatcher, by tehasbot cycle, by tehas index, by install.sh, and by
# the two commands that give a cycle its issue's images.
#
# tehasbot cycle holds $TEHAS_HOME/state/cycles/<issue>.lock for as long as it
# lives. A lock nobody holds is a cycle that ended, however it ended, so a
# cycle that died is never counted.
#
#   tehas_cycle_running <issue>   0 if that issue's cycle holds its lock
#   tehas_cycles_running          the issues with a running cycle, one per line
#   park_id <issue>               the path of the file that holds the id of
#                                 the park question this host posted
#   tehas_rukas_commit            the pi-rukas commit a cycle started now runs
#   tehas_rukas_proven <commit>   0 if a cycle on this host ended well on it
#   tehas_rukas_reads_images      whether that pi-rukas reads the issue image
#                                 manifest: 1, 0, or nothing when the image
#                                 does not say
#
# Needs TEHAS_HOME. Reads only: it takes each lock for an instant and lets go.
# The two tehas_rukas_ functions ask Docker about PI_ENSEMBLE_IMAGE, once per
# command.

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

# The factory works on open issues only: a closed issue is never taken, and a
# pull request is never taken, open, merged or closed. What GitHub answers for
# a number decides it, not which labels that number carries.
#
#   tehas_work_kind <the answer for that number>
#     0  the answer is an open issue, which the factory may take
#     1  it is anything else, with the reason in $tehas_work_why: pr (a pull
#        request), closed, or unreadable (no answer, or one that is not the
#        answer for a single issue). A number is never taken on a guess.
tehas_work_why=""
tehas_work_kind() {
  local kind
  kind=$(jq -r 'if type == "object" and ((.pull_request // null) != null) then "pr"
                elif type == "object" and ((.state // "") | ascii_downcase) == "open" then "open"
                elif type == "object" and ((.state // "") | ascii_downcase) == "closed" then "closed"
                else "unreadable" end' <<<"${1:-}" 2>/dev/null) || kind=""
  case "$kind" in
    open) tehas_work_why=open ;;
    pr | closed) tehas_work_why=$kind ;;
    *) tehas_work_why=unreadable; return 1 ;;
  esac
  [ "$tehas_work_why" = open ]
}

# The sandbox image answers two questions about itself, in its labels, and one
# docker call answers both. tehas.rukas is the pi-rukas commit it was built
# with; tehas.rukas.images is whether that pi-rukas reads the issue image
# manifest, and host/sandbox/build.sh writes what it found when it builds.
# Asked once, because a command that asks one of them usually wants the other.
# The answer is kept for the rest of that command: every command that asks here
# asks, acts and exits, so nothing is carried into a later poll or across a
# rollback of the image while a command runs. A label the image does not carry,
# or a value that is not one of the expected ones, reads as nothing.
tehas_rukas_asked=""
tehas_rukas_commit_label=""
tehas_rukas_images_label=""
tehas_rukas_ask() {
  local line commit="" images=""
  if [ -n "$tehas_rukas_asked" ]; then return 0; fi
  tehas_rukas_asked=1
  line=$(docker image inspect --format '{{index .Config.Labels "tehas.rukas"}}|{{index .Config.Labels "tehas.rukas.images"}}' "${PI_ENSEMBLE_IMAGE:-}" 2>/dev/null) || line=""
  IFS='|' read -r commit images <<<"$line" || true
  [[ $commit =~ ^[0-9a-f]{7,40}$ ]] || commit=""
  case "$images" in 1 | 0) ;; *) images="" ;; esac
  tehas_rukas_commit_label=$commit
  tehas_rukas_images_label=$images
}

# The commit, also for an image put back for a rollback. An image built before
# the label has the commit versions.env names. Nothing when neither says.
tehas_rukas_commit() {
  local c
  tehas_rukas_ask
  c=$tehas_rukas_commit_label
  [ -n "$c" ] ||
    c=$(sed -n 's/^PI_RUKAS_COMMIT=\([0-9a-f]\{7,40\}\)$/\1/p' "$TEHAS_HOME/versions.env" 2>/dev/null | head -n 1)
  printf '%s\n' "$c"
}

# Whether the pi-rukas in the sandbox image reads the manifest tehas writes
# beside the images it mounts for a cycle. 1 reads them, 0 does not, nothing
# when the image does not say: an image built before the label existed, or not
# built by tehas at all. Whoever is asked whether a cycle saw a picture has to
# answer that it does not know.
tehas_rukas_reads_images() {
  tehas_rukas_ask
  printf '%s\n' "$tehas_rukas_images_label"
}

# state/rukas-proven: one commit a line, each one a cycle ended well on.
tehas_rukas_proven() { [ -n "$1" ] && grep -qxF -e "$1" "$TEHAS_HOME/state/rukas-proven" 2>/dev/null; }
