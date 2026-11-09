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
jq '.later = 1 | .notices[0].later = 1 | .ticketNotes[0].later = {}' "$api/host-bridge.full.json" >"$T/more.json"
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
jq '.notices[0].id = "Big" | .notices[1].level = "loud" | .notices[2].text = ""
    | .notices[0].link = "http://x.example/" | .ticketNotes[0].issue = 0
    | .ticketNotes[1].issue = 1000000000 | .ticketNotes[2].text = ("t" * 61)
    | .ticketNotes[2].link = "javascript:1"' "$api/host-bridge.full.json" >"$T/bad.json"
out=$(check "$T/bad.json")
t_contains "$out" '.notices[0].id: "Big" does not match'
t_contains "$out" '.notices[1].level: "loud" is not one of'
t_contains "$out" ".notices[2].text: shorter than 1"
t_contains "$out" '.notices[0].link: "http://x.example/" does not match'
t_contains "$out" ".ticketNotes[0].issue: 0 is under 1"
t_contains "$out" ".ticketNotes[1].issue: 1000000000 is over 999999999"
t_contains "$out" ".ticketNotes[2].text: longer than 60"
t_contains "$out" '.ticketNotes[2].link: "javascript:1" does not match'

t_case "the check finds more notices or notes than a list holds"
jq '.notices = [range(6) | {id: "n-\(.)", level: "info", text: "x"}]
    | .ticketNotes = [range(201) | {issue: (. + 1), text: "x"}]' "$api/host-bridge.full.json" >"$T/bad.json"
out=$(check "$T/bad.json")
t_contains "$out" ".notices: more than 5 items"
t_contains "$out" ".ticketNotes: more than 200 items"

t_case "the check finds a token day or model that breaks a rule, and lists too long"
jq '.tokens.days[0].date = "7 Nov" | .tokens.days[1].input = -1 | .tokens.models[0].model = ""
    | .tokens.models[0].output = 1.5 | .tokens.unknown = 1000000000000001' "$api/host-bridge.full.json" >"$T/bad.json"
out=$(check "$T/bad.json")
t_contains "$out" '.tokens.days[0].date: "7 Nov" does not match'
t_contains "$out" ".tokens.days[1].input: -1 is under 0"
t_contains "$out" ".tokens.models[0].model: shorter than 1"
t_contains "$out" ".tokens.unknown: 1000000000000001 is over 1000000000000000"
jq '.tokens.days = [range(11) | {date: "2026-11-01", requests: 0, input: 0, output: 0, cached: 0}]
    | .tokens.models = [range(21) | {model: "m\(.)", requests: 0, input: 0, output: 0, cached: 0}]' \
  "$api/host-bridge.full.json" >"$T/bad.json"
out=$(check "$T/bad.json")
t_contains "$out" ".tokens.days: more than 10 items"
t_contains "$out" ".tokens.models: more than 20 items"

t_case "the business front page draws token use at the foot of the Performance section, hidden until there is some"
t_eq "$(awk '/id="period"/ {p = 1} p && /id="host-tokens"[^>]* hidden><\/div>/ {print "in"; exit} p && /<\/section>/ {print "out"; exit}' \
  "$T_ROOT/host/fun/business.html")" in

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

t_case "the notices sit beside the alerts banner on both front pages, hidden until there is one"
for page in "$T_ROOT/host/fun/index.html" "$T_ROOT/host/fun/business.html"; do
  t_eq "$(grep -A1 'id="industry-alerts"' "$page" | grep -c 'id="host-notices"[^>]* hidden></section>')" 1
done

t_case "tehas api lists the features host-bridge and host-bridge-tokens, and the manual names them"
t_contains "$(cat "$T_ROOT/console/src/api.rs")" '"host-bridge",'
t_contains "$(cat "$T_ROOT/console/src/api.rs")" '"host-bridge-tokens",'
t_contains "$(cat "$T_ROOT/host/docs/api.md")" '## The host bridge'
t_contains "$(cat "$T_ROOT/host/docs/api.md")" '`host-bridge` ([the host bridge](#the-host-bridge))'
t_contains "$(cat "$T_ROOT/host/docs/api.md")" '`host-bridge-tokens` (the dashboard draws `tokens`'

t_done
