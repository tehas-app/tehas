# shellcheck shell=bash
# What tehas reads of a cycle's progress, and whether a result is an error only.
# The rest of what tehas reads of pi-rukas's headless runner is tehas-cycle's
# (caps, runner). Only the contract the runner publishes is read here
# (docs/headless.md in pi-rukas), never its state file. Sourced by tehas work
# (libexec/work) and tehas review (libexec/review).
#
#   tehas_headless_progress <ndjson> [<result> <issue>]
#       one JSON object: roles, the per-role usage; reason, why the cycle
#       stopped at its cap; failure, the result's failure kind, step, role and
#       cause, or null; from, where roles and failure were read; reviewHeld, a
#       stable cap-hit ended the review on its round cap and sent the cycle on
#       to ci, so its findings were left open. roles and failure come from the
#       issue's entry of the result; with no such entry they come from the
#       progress: the stable dispatch-completed lines, each counted once by
#       its key, and for the reason lines the runner marks unstable.
#   tehas_headless_error_only <result>
#       whether <result> is a schema 5 result with no issue entry at all, as a
#       refusal that carries only an error may be.

# The result schema the bash reads; tehas-cycle holds the same one. Lower case:
# tehas_load unsets every TEHAS_* name.
tehas_headless_result=5

# The stable lines of a progress stream, each a JSON object. A line that is
# not JSON, or that the runner marks unstable, is not one of them.
_tehas_headless_stable() { # <ndjson>
  jq -R -c 'fromjson? | objects | select(.v == 1 and (.unstable // false) == false)' "$1" 2>/dev/null
}

tehas_headless_progress() { # <ndjson> [<result> <issue>]
  local legacy result=null
  legacy=$(_tehas_headless_progress_lines "$1") || legacy=""
  [ -n "$legacy" ] || legacy='{"roles":[],"reason":"","reviewHeld":false}'
  if [ -n "${3:-}" ] && [ ! -L "${2:-}" ]; then
    result=$(jq -c --argjson issue "$3" --argjson rv "$tehas_headless_result" '
      select(type == "object" and .schemaVersion == $rv)
      | [.issues[]? | objects | select(.issue == $issue)] | first // null' "$2" 2>/dev/null) || result=null
    [ -n "$result" ] || result=null
  fi
  jq -n -c --argjson legacy "$legacy" --argjson r "$result" '
    def n: if type == "number" and . >= 0 then . else 0 end;
    def plain: if type == "string" then gsub("[^A-Za-z0-9_.:-]"; "") else "" end;
    ($r != null) as $from_result
    | ($r.failure // null | objects // null) as $f
    | {roles: (if $from_result then
               [($r.usage | objects | .byRole | objects) // {} | to_entries[] | select(.value | type == "object")
                | (.key | plain) as $k
                | {role: (if $k == "" then "driver" else $k end), dispatches: (.value.dispatches | n),
                   turns: (.value.turns | n), input: (.value.input | n), output: (.value.output | n),
                   cacheRead: (.value.cacheRead | n)}]
             else $legacy.roles end),
       reason: (if $from_result then
                ([$f.providerMessage?, $f.message?] | map(strings | select(length > 0)) | first // "")
              else $legacy.reason end),
       failure: (if $from_result and $f != null then
                 {kind: ($f.kind | plain), step: ($f.step | plain), role: ($f.role | plain), cause: ($f.cause | plain)}
               else null end),
       reviewHeld: ($legacy.reviewHeld == true),
       from: {roles: (if $from_result then "result" else "progress" end),
              failure: (if $from_result then "result" else "progress" end)}}' 2>/dev/null
}

# The per-role usage and the reason of a run whose runner wrote no result: the
# stable dispatch-completed lines, and for the reason the lines the runner marks
# unstable. A schema 5 run always answers from its result, so this is read only
# for a runner that died before writing one.
_tehas_headless_progress_lines() { # <ndjson>
  {
    _tehas_headless_stable "$1"
    jq -R -c 'fromjson? | objects | select(.v == 1 and .unstable == true)
      | select(.kind == "dispatch-failed" or .kind == "dispatch-failed-provider")' "$1" 2>/dev/null
    jq -R -c 'fromjson? | objects | select(.v == 1 and .kind == "cap-hit")' "$1" 2>/dev/null
  } | jq -s -c '
    def n: tonumber? // 0;
    def plain: tostring | gsub("[^A-Za-z0-9_.-]"; "");
    (map(select(.kind == "dispatch-completed" and (.unstable // false) == false))
     | unique_by(.key) | map(.event | objects)
     | map({role: ((.role // "" | plain) as $r | if $r == "" then "driver" else $r end),
            u: (.usage // {} | {turns: (.turns | n), input: (.input | n), output: (.output | n),
                               cacheRead: (.cacheRead | n)})})
     | group_by(.role)
     | map({role: .[0].role, dispatches: length, turns: (map(.u.turns) | add),
            input: (map(.u.input) | add), output: (map(.u.output) | add),
            cacheRead: (map(.u.cacheRead) | add)})) as $roles
    | ([.[] | select(.kind == "cap-hit") | .event.evidence?] | map(strings | select(length > 0)) | last) as $evidence
    | any(.[]; .kind == "cap-hit" and (.unstable // false) == false
          and .event.cap? == "round-cap" and .event.nextStep? == "ci") as $held
    | ([.[] | select(.unstable == true) | .event | (.providerMessage?, .errorTail?)]
       | map(strings | select(length > 0)) | last) as $failure
    | {roles: $roles, reason: ($evidence // $failure // ""), reviewHeld: $held}' 2>/dev/null
}

tehas_headless_error_only() { # <result>
  jq -e --argjson rv "$tehas_headless_result" \
    'type == "object" and .schemaVersion == $rv and ((.issues // []) | type == "array" and length == 0)' \
    "$1" >/dev/null 2>&1 || return 1
}
