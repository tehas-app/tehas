#!/usr/bin/env bash
# Tests for the contract of the host bridge, /host/bridge.json:
# api/host-bridge.schema.json and its two samples, the docs server's 404 under
# /host/, and the pages that read it. What the reading module makes of an
# answer is in test/host_bridge_test.mjs. See "The host bridge" in
# host/docs/api.md.
# shellcheck disable=SC2016  # jq programs and Markdown are matched as text
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

api=$T_ROOT/api
schema=$api/host-bridge.schema.json
caddyfile=$T_ROOT/host/edge/docs.Caddyfile

check() { # <json file>: the lines test/schema.jq prints for it
  jq -r --slurpfile schema "$schema" -f "$T_ROOT/test/schema.jq" "$1"
}

t_case "the schema is JSON Schema 2020-12 and allows unknown fields at the top and in each element"
t_eq "$(jq -r '."$schema"' "$schema")" "https://json-schema.org/draft/2020-12/schema"
t_eq "$(jq -c '[.. | objects | select(has("additionalProperties"))] | length' "$schema")" 0
jq '.later = 1 | .ticketNotes[0].later = 1 | .menu[0].later = {}' "$api/host-bridge.full.json" >"$T/more.json"
t_eq "$(check "$T/more.json")" ""

for name in full bare; do
  t_case "api/host-bridge.$name.json fits the schema"
  t_eq "$(check "$api/host-bridge.$name.json")" ""
done

t_case "the bare sample holds only bridge, host and time; the full one every field of the schema"
t_eq "$(jq -c 'keys' "$api/host-bridge.bare.json")" '["bridge","host","time"]'
fields() { # <jq paths filter> <file>
  jq -r "$1" "$2" | sort -u
}
schema_paths='def walk_props($p): (.properties // {}) | to_entries[]
  | ($p + .key) as $q | $q, (.value | (.items // .) | walk_props($q + "."));
  walk_props("")'
held='paths | map(strings) | join(".") | select(. != "")'
t_eq "$(comm -23 <(fields "$schema_paths" "$schema") <(fields "$held" "$api/host-bridge.full.json"))" ""

t_case "the check finds a bad version, a missing host, a bad time and a ticket link without {n}"
jq '.bridge = 2 | del(.host) | .time = "noon" | .ticketLink = "/host/ticket"' "$api/host-bridge.full.json" >"$T/bad.json"
out=$(check "$T/bad.json")
t_contains "$out" ".bridge: 2, not 1"
t_contains "$out" ".: host is missing"
t_contains "$out" '.time: "noon" is not a date-time'
t_contains "$out" '.ticketLink: "/host/ticket" does not match'

t_case "the check finds a ticket link off /host/ or with {n} twice, and a host too long"
for bad in '"https://x.example/{n}"' '"/host/{n}/{n}"' '"/other/{n}"'; do
  jq ".ticketLink = $bad" "$api/host-bridge.full.json" >"$T/bad.json"
  t_contains "$(check "$T/bad.json")" ".ticketLink: $bad does not match"
done
jq '.host = ("h" * 41)' "$api/host-bridge.full.json" >"$T/bad.json"
t_contains "$(check "$T/bad.json")" ".host: longer than 40"
jq '.host = "two\nlines"' "$api/host-bridge.full.json" >"$T/bad.json"
t_contains "$(check "$T/bad.json")" '.host: "two\nlines" does not match'

t_case "the check finds each element's breach"
jq '.ticketNotes[0].issue = 0
    | .ticketNotes[1].issue = 1000000000 | .ticketNotes[2].text = ("t" * 61)
    | .ticketNotes[2].link = "javascript:1"' "$api/host-bridge.full.json" >"$T/bad.json"
out=$(check "$T/bad.json")
t_contains "$out" ".ticketNotes[0].issue: 0 is under 1"
t_contains "$out" ".ticketNotes[1].issue: 1000000000 is over 999999999"
t_contains "$out" ".ticketNotes[2].text: longer than 60"
t_contains "$out" '.ticketNotes[2].link: "javascript:1" does not match'

t_case "the check finds more notes than a list holds"
jq '.ticketNotes = [range(201) | {issue: (. + 1), text: "x"}]' "$api/host-bridge.full.json" >"$T/bad.json"
out=$(check "$T/bad.json")
t_contains "$out" ".ticketNotes: more than 200 items"

t_case "an answer that still carries tokens fits the schema: the field is unknown now and ignored"
jq '.tokens = {days: [{date: "7 Nov", input: -1}], models: "x", unknown: 1.5}' "$api/host-bridge.full.json" >"$T/old.json"
t_eq "$(check "$T/old.json")" ""
t_eq "$(jq -c '.properties | has("tokens")' "$schema")" false

t_case "the business front page draws no token table"
t_eq "$(grep -c 'host-tokens\|token-use\|token-table' "$T_ROOT/host/fun/business.html" "$T_ROOT/host/fun/business.css" "$T_ROOT/host/fun/business-view.js" | awk -F: '{n += $2} END {print n}')" 0
t_eq "$(grep -c 'drawTokens\|tokensOf' "$T_ROOT/host/fun/host-bridge.js")" 0

t_case "the docs server answers every path under /host/ with 404, before anything else it serves"
t_contains "$(cat "$caddyfile")" "@host path /host /host/*"
t_contains "$(cat "$caddyfile")" "respond @host 404"
# The first thing in the route, so no handler below can answer under /host/.
t_eq "$(awk '/^    route \{/ {r = NR} /respond @host 404/ {h = NR} /handle|rewrite|file_server/ && r && !f {f = NR}
  END {print (r && h && h < f) ? "first" : "not first"}' "$caddyfile")" first
if command -v caddy >/dev/null 2>&1; then
  t_run "caddy validate --adapter caddyfile --config '$caddyfile'"
  t_eq "$t_rc" 0
fi

t_case "both front pages and /dashboard load the reading module before the views, and tehas registers no service worker"
for page in "$T_ROOT/host/fun/index.html" "$T_ROOT/host/fun/business.html" "$T_ROOT/host/docs/dashboard.frag"; do
  text=$(cat "$page")
  t_contains "$text" '<script src="/fun/host-bridge.js"></script>'
  t_eq "$(awk '/host-bridge\.js/ {b = NR} /type="module"/ && !m {m = NR} END {print (b && b < m) ? "before" : "after"}' "$page")" before
done
t_eq "$(grep -l 'serviceWorker.register\|navigator.serviceWorker.register' "$T_ROOT"/host/fun/*.js "$T_ROOT"/host/fun/*.html | wc -l)" 0
t_contains "$(cat "$T_ROOT/host/fun/host-bridge.js")" "r.unregister()"

t_case "an answer that still carries notices fits the schema, and no page or module draws them"
jq '.notices = [{id: "n-1", level: "loud", text: ""}]' "$api/host-bridge.full.json" >"$T/old.json"
t_eq "$(check "$T/old.json")" ""
t_eq "$(jq -c '.properties | has("notices")' "$schema")" false
t_eq "$(grep -c 'host-notice\|drawNotices\|notices' "$T_ROOT/host/fun/index.html" "$T_ROOT/host/fun/business.html" "$T_ROOT/host/fun/host-bridge.js" "$T_ROOT/host/fun/panels.css" | awk -F: '{n += $2} END {print n}')" 0

t_case "tehas api lists the feature host-bridge and no longer host-bridge-tokens, and the manual names it"
t_contains "$(cat "$T_ROOT/console/src/api.rs")" '"host-bridge",'
t_eq "$(grep -c 'host-bridge-tokens' "$T_ROOT/console/src/api.rs" "$T_ROOT/host/docs/api.md" | awk -F: '{n += $2} END {print n}')" 0
t_contains "$(cat "$T_ROOT/host/docs/api.md")" '## The host bridge'
t_contains "$(cat "$T_ROOT/host/docs/api.md")" '`host-bridge` ([the host bridge](#the-host-bridge))'

t_done
