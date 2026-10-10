# shellcheck shell=bash
# The improvement loop's shared parts: the switch, the record a cycle leaves
# for the loop, and how a whole log is packed for upload. Sourced by
# libexec/improve and by the script that ends a cycle.
#
#   tehas_improve_on       is the loop switched on for this instance?
#   tehas_improve_valid    return 78 unless TEHAS_IMPROVE_REPO is owner/name
#   tehas_improve_dir      print where records wait
#   tehas_improve_record <issue> <started> <rc> <status> <step> [<pr>]
#                          write the record of a cycle that has ended
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

tehas_improve_valid() {
  [[ ${TEHAS_IMPROVE_REPO:-} =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] && return 0
  tehas_err "TEHAS_IMPROVE_REPO is not valid: expected owner/name (see ${TEHAS_INSTANCE_DIR:-?}/instance.env)"
  return 78
}

tehas_improve_dir() { printf '%s\n' "$TEHAS_HOME/tmp/improve"; }

# The stream as a model can read it. A line that is not JSON is dropped. The
# id of a status event differs every time, so it goes, and uniq then keeps a
# status repeated in a row once.
# shellcheck disable=SC2016  # a jq program: the variables are jq's
_tehas_improve_condense='
  fromjson? | select(type == "object" and .type != "message_update")
  | if .type == "extension_ui_request" then del(.id) else . end
  | walk(if type == "string" and length > 2000 then .[0:2000] + " [cut]" else . end)'

tehas_improve_record() { # <issue> <started> <rc> <status> <step> [<pr>]
  tehas_improve_on || return 0
  local issue=$1 started=$2 rc=$3 status=$4 step=$5 pr=${6:-} base raw rec tmp
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
  if ! jq -n --argjson issue "$issue" --argjson started "$started" --argjson rc "$rc" \
    --argjson ended "$(date +%s)" --arg status "$status" --arg step "$step" --arg pr "$pr" \
    --arg log "$(basename "$raw")" '
      {issue: $issue, started: $started, seconds: ($ended - $started), status: $status,
       step: $step, rc: $rc, pr: (if $pr == "" then null else $pr end), log: $log}' \
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
