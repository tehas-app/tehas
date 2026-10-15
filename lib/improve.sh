# shellcheck shell=bash
# The improvement loop's shared parts: the switch, the record a cycle leaves
# for the loop, and how a whole log is packed for upload. Sourced by
# libexec/improve and by the script that ends a cycle.
#
#   tehas_improve_on       is the loop switched on for this instance?
#   tehas_improve_dir      print where records wait
#   tehas_improve_record <issue> <started> <rc> <status> <step> [<pr> [<kills>]]
#                          write the record of a cycle that has ended. <kills>
#                          is how many processes in its container were killed
#                          for memory, oomKills in cycle.json; anything but a
#                          number counts as 0
#   tehas_improve_secret <file>
#                          does the file hold something shaped like a token?
#   tehas_improve_pack <file> <dir> <name> <limit>
#                          gzip <file> into <dir> as <name>.gz, or in numbered
#                          pieces when that would be over <limit> bytes
#
# Source lib/config.sh and call tehas_load first.
#
# A record is a directory, window/<issue>-<stamp>, and the only thing the loop
# reads of a cycle. It is built beside the window and renamed into it, so a
# run never sees half of one and cycles that end together do not collide.

tehas_improve_on() { [ -n "${TEHAS_IMPROVE_REPO:-}" ]; }

tehas_improve_dir() { printf '%s\n' "$TEHAS_HOME/tmp/improve"; }

# The stream as a model can read it. A line that is not JSON is dropped. The
# id of a status event differs every time, so it goes, and uniq then keeps a
# status repeated in a row once.
# shellcheck disable=SC2016  # a jq program: the variables are jq's
_tehas_improve_condense='
  fromjson? | select(type == "object" and .type != "message_update")
  | if .type == "extension_ui_request" then del(.id) else . end
  | walk(if type == "string" and length > 2000 then .[0:2000] + " [cut]" else . end)'

tehas_improve_record() { # <issue> <started> <rc> <status> <step> [<pr> [<kills>]]
  tehas_improve_on || return 0
  local issue=$1 started=$2 rc=$3 status=$4 step=$5 pr=${6:-} kills=${7:-0} base raw rec tmp rukas
  [[ $kills =~ ^(0|[1-9][0-9]{0,8})$ ]] || kills=0
  base=$(tehas_improve_dir)
  raw=$(find "$TEHAS_HOME/logs" -maxdepth 1 -type f -name "work-$issue-*.jsonl" \
    -newermt "@$((started - 5))" 2>/dev/null | sort | tail -n 1)
  if [ -z "$raw" ]; then
    tehas_err "improve: no log of issue $issue since the cycle began, no record written"
    return 1
  fi
  rec=$(basename "$raw" .jsonl)
  rec=${rec#work-}
  mkdir -p "$base/window" || return 1
  tmp=$(mktemp -d "$base/.new.XXXXXX") || return 1
  # The pi-rukas this cycle ran with, as tehasbot cycle noted it at the start:
  # what a fix in it is checked against.
  rukas=$(head -c 64 "$TEHAS_HOME/state/cycles/$issue.rukas" 2>/dev/null | head -n 1)
  [[ $rukas =~ ^[0-9a-f]{7,40}$ ]] || rukas=""
  if ! jq -n --argjson issue "$issue" --argjson started "$started" --argjson rc "$rc" \
    --argjson ended "$(date +%s)" --arg status "$status" --arg step "$step" --arg pr "$pr" \
    --arg log "$(basename "$raw")" --argjson kills "$kills" --arg rukas "$rukas" '
      {issue: $issue, started: $started, seconds: ($ended - $started), status: $status,
       step: $step, rc: $rc, pr: (if $pr == "" then null else $pr end), log: $log,
       oomKills: $kills, rukas: (if $rukas == "" then null else $rukas end)}' \
    >"$tmp/cycle.json"; then
    rm -rf "$tmp"
    return 1
  fi
  jq -c -R "$_tehas_improve_condense" "$raw" 2>/dev/null | uniq >"$tmp/stream.jsonl"
  cp "$TEHAS_HOME/live/console-$issue.txt" "$tmp/console.txt" 2>/dev/null || : >"$tmp/console.txt"
  cp "$raw.err" "$tmp/err.txt" 2>/dev/null || : >"$tmp/err.txt"
  cp "$raw.preflight" "$tmp/preflight.txt" 2>/dev/null || : >"$tmp/preflight.txt"
  rm -rf "${base:?}/window/$rec"
  # A run may have taken the window a moment ago.
  if ! { mkdir -p "$base/window" && mv "$tmp" "$base/window/$rec"; }; then
    rm -rf "$tmp"
    return 1
  fi
}

# Logs should hold no secret. This is for the day one does: a log that matches
# is not uploaded.
tehas_improve_secret() { # <file>
  grep -E -q -e 'gh[pousr]_[A-Za-z0-9]{36,}' -e 'github_pat_[A-Za-z0-9_]{22,}' \
    -e '(^|[^A-Za-z0-9])sk-[A-Za-z0-9_-]{20,}' -e 'xox[abprs]-[A-Za-z0-9-]{10,}' -e 'AKIA[0-9A-Z]{16}' \
    -e '-----BEGIN [A-Z ]*PRIVATE KEY-----' "$1"
}

# Pieces are split at line boundaries and compressed one by one, so each opens
# on its own. The number of pieces doubles until every piece fits.
tehas_improve_pack() { # <file> <dir> <name> <limit>
  local file=$1 dir=$2 name=$3 limit=$4 n lines parts i f big
  mkdir -p "$dir" || return 1
  gzip -9 -c "$file" >"$dir/$name.gz" || return 1
  n=$(stat -c %s "$dir/$name.gz")
  [ "$n" -gt "$limit" ] || return 0
  rm -f "$dir/$name.gz"
  n=$((n / limit + 1))
  lines=$(wc -l <"$file")
  parts=$dir/.parts
  while [ "$lines" -ge 2 ]; do
    [ "$n" -le "$lines" ] || n=$lines
    rm -rf "$parts" "$dir/$name".*.gz
    mkdir -p "$parts" || return 1
    split -n "l/$n" -a 5 -d "$file" "$parts/p." || break
    i=0
    big=0
    for f in "$parts"/p.*; do
      [ -s "$f" ] || continue
      i=$((i + 1))
      gzip -9 -c "$f" >"$dir/$name.$i.gz" || big=1
      [ "$(stat -c %s "$dir/$name.$i.gz")" -le "$limit" ] || big=1
    done
    rm -rf "$parts"
    [ "$big" = 1 ] || return 0
    [ "$n" -lt "$lines" ] || break
    n=$((n * 2))
  done
  rm -rf "$parts" "$dir/$name".*.gz
  return 1
}
