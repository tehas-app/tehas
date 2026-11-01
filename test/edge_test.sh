#!/usr/bin/env bash
# Tests for the router's files in host/edge.
# shellcheck disable=SC2016  # snippets and patterns are expanded later, or never
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

caddyfile=$T_ROOT/host/edge/router.Caddyfile
compose=$T_ROOT/host/edge/router.compose.yaml

t_case "the router's file names no domain and takes numbers only"
t_eq "$(grep -c -E 'tehas\.app|example\.' "$caddyfile")" 0
t_contains "$(cat "$caddyfile")" '@preview header_regexp preview Host ^uat-([1-9][0-9]{0,8})\.'
t_contains "$(cat "$caddyfile")" '@main header_regexp main Host ^uat-main\.'
t_contains "$(cat "$caddyfile")" 'reverse_proxy tehas-preview-main:8080'
t_eq "$(awk '/handle @main \{/ {a = NR} /handle @odd \{/ {b = NR} END {print (a && a < b) ? "before" : "after"}' "$caddyfile")" before
t_contains "$(cat "$T_ROOT/host/docs/dashboard.frag")" 'const isMain = !!it && it.name === "main";'
t_contains "$(cat "$caddyfile")" '@ticket header_regexp ticket Host ^lab-([1-9][0-9]{0,8})\.'
t_contains "$(cat "$caddyfile")" 'reverse_proxy tehas-preview-{re.preview.1}:8080'
t_contains "$(cat "$caddyfile")" 'root * /srv/protolab/{re.ticket.1}'

t_case "every answer of the router says noindex, a preview's own header is dropped"
t_contains "$(cat "$caddyfile")" 'header X-Robots-Tag "noindex, nofollow"'
t_contains "$(cat "$caddyfile")" 'header_down -X-Robots-Tag'

t_case "a preview that is not there is a 404, not a gateway error"
t_contains "$(cat "$caddyfile")" '{err.status_code} in [502, 503, 504]'

t_case "the router is hardened and on the previews' network"
for want in 'name: tehas-edge' 'read_only: true' '- ALL' 'no-new-privileges:true' \
  'name: tehas-previews' 'external: true' './router.Caddyfile:/etc/caddy/Caddyfile:ro' \
  '${TEHAS_HOST_ADDR:-127.0.0.1}:8081:8081'; do
  t_contains "$(cat "$compose")" "$want"
done
t_lacks "$(cat "$compose")" "docker.sock"

t_case "the router keeps its protections"
t_contains "$(cat "$caddyfile")" 'respond @kept 404'
t_contains "$(cat "$caddyfile")" '@odd header_regexp odd Host ^(uat|lab)-'
t_contains "$(cat "$caddyfile")" 'header Content-Security-Policy "sandbox allow-scripts allow-forms allow-modals allow-pointer-lock"'
t_contains "$(cat "$caddyfile")" "header Content-Security-Policy \"default-src 'none'; style-src 'self'"

t_case "the Caddyfile is valid, where there is a caddy to ask"
if command -v caddy >/dev/null 2>&1; then
  t_run "caddy validate --adapter caddyfile --config '$caddyfile'"
  t_eq "$t_rc" 0
fi

recipe=$T_ROOT/preview/Dockerfile
# shellcheck source=host/versions.env
. "$T_ROOT/host/versions.env"

t_case "every caddy container is named by the digest versions.env records"
for f in "$T_ROOT/host/edge/docs.compose.yaml" "$T_ROOT/host/edge/router.compose.yaml" "$recipe"; do
  t_contains "$(cat "$f")" "caddy@$CADDY_DIGEST"
  t_contains "$(cat "$f")" "caddy $CADDY_VERSION"
done
t_eq "$(grep -cE '^CADDY_VERSION=[0-9]+\.[0-9]+\.[0-9]+$' "$T_ROOT/host/versions.env")" 1
t_eq "$(grep -cE '^CADDY_DIGEST=sha256:[0-9a-f]{64}$' "$T_ROOT/host/versions.env")" 1
# A tag can be moved, so nothing here may name caddy by tag.
t_eq "$(grep -l -E 'caddy:[0-9]' "$T_ROOT/host/edge/docs.compose.yaml" \
  "$T_ROOT/host/edge/router.compose.yaml" "$recipe" | wc -l)" 0

t_case "the preview of this repository serves the manual on 8080 and takes nothing from a host"
t_contains "$(cat "$recipe")" "COPY host/edge/docs.Caddyfile /etc/caddy/Caddyfile"
t_contains "$(cat "$recipe")" "COPY host/docs /srv/docs"
t_contains "$(cat "$recipe")" "COPY host/fun /srv/fun"
t_contains "$(cat "$recipe")" "COPY preview/live /srv/live"
t_contains "$(grep -c ':8080' "$T_ROOT/host/edge/docs.Caddyfile")" 1
t_eq "$(grep -c -i -E '^(RUN|ADD|ARG|VOLUME)' "$recipe")" 0

t_case "the preview's sample data is what the dashboard reads"
for f in status running health previews; do
  t_run "jq -e . '$T_ROOT/preview/live/$f.json' >/dev/null; echo rc=\$?"
  t_eq "$t_out" "rc=0"
done
t_eq "$(jq -r '[.queued, .awaiting, .stopped, .completed, .running] | map(type) | unique | join(",")' \
  "$T_ROOT/preview/live/status.json")" "array"
t_eq "$(jq -r '.links | keys | join(",")' "$T_ROOT/preview/live/status.json")" "all,awaiting,completed,queued,stopped"
t_eq "$(jq -r '[(.previews | type), (.disk | type)] | join(",")' "$T_ROOT/preview/live/previews.json")" "array,object"

# The sample files stand where a host has what the store's publish and
# tehasbot health write. Their keys are pinned to those two, so a change there
# is seen here.
t_case "the preview's sample running.json is the live view, and its cycle has the keys the store publishes, but for the console"
written=$(sed -n '/fn pairs/,/^    }/p' "$T_ROOT/console/store/src/live.rs" | grep -oE '\("[A-Za-z]+", j\(' | cut -d'"' -f2 | LC_ALL=C sort | tr '\n' ' ')
t_eq "$written" "console issue running startedAt status statusText step updatedAt "
# The view: no cycle runs in a preview, and one finished last.
t_eq "$(jq -c '[keys, .cycles]' "$T_ROOT/preview/live/running.json")" '[["cycles","last"],[]]'
# A preview has no console file to show, so the sample names none.
t_eq "$(jq -r '.last | keys_unsorted | join(" ")' "$T_ROOT/preview/live/running.json" | tr ' ' '\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "${written#console }"

t_case "the preview's sample health.json has the keys tehasbot health writes from the samples beside it"
sample=$T/home/sample
mkdir -p "$sample/live"
# The state store health reads the planning turns from, with none on record.
t_store_home "$sample"
# A backup of today, as a host has one.
TEHAS_HOME=$sample "$T_STORE" backup >/dev/null || t_bad "the store made no backup of the sample"
# The cycle that finished last, as the sample shows it; health publishes the
# view from the store before it reads it.
t_live "$sample" "$(jq .last.issue "$T_ROOT/preview/live/running.json")" false \
  "$(jq -r .last.status "$T_ROOT/preview/live/running.json")" review
# The sample health file was written when its snapshot was: a fresh one.
jq '.generatedAt = (now | todate)' "$T_ROOT/preview/live/status.json" >"$sample/live/status.json"
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_health || t_bad "no console: $t_console_why"
# The wrapper loads the settings, as its siblings do, so the case gives it the
# instance a host has: the minimal one, whose budget is the default the sample
# health file carries.
T_ENV=(TEHAS_INSTANCE_DIR="$T/instance")
t_instance "${T_MINIMAL[@]}"
# A systemd with no failed unit, so the run says nothing on a machine with none.
t_stub systemctl 'exit 0'
t_run "TEHAS_HOME='$sample' '$T_HEALTH'; echo rc=\$?"
t_eq "$t_out" "rc=0"
T_ENV=()
shape='[paths | map(tostring) | join(".")] | sort | join(" ")'
t_eq "$(jq -r "$shape" "$T_ROOT/preview/live/health.json")" "$(jq -r "$shape" "$sample/live/health.json")"
t_eq "$(jq -c '[.status, .checks.queue.inLine, .checks.queue.review, .checks.cycle.status]' "$T_ROOT/preview/live/health.json")" \
  "$(jq -c '[.status, .checks.queue.inLine, .checks.queue.review, .checks.cycle.status]' "$sample/live/health.json")"

t_case "the dashboard's Previews heading takes the one rule of the lists above it"
css=$T_ROOT/host/fun/manual.css
t_eq "$(grep -c -F '.page .cols h2, .page #previews h2 {' "$css")" 1
t_eq "$(grep -c -F '#previews h2' "$css")" 1
t_eq "$(grep -c -F '<section id="previews" hidden>' "$T_ROOT/host/docs/dashboard.frag")" 1

t_case "the dashboard says when the list of previews was written"
t_eq "$(grep -c -F 'note.push("List written " + ago(p.generatedAt) + ".");' "$T_ROOT/host/docs/dashboard.frag")" 1

# A refused volume and a build that failed are both "build-failed": only the
# reason the run wrote into the state says which. The reason is text this
# command wrote, never the pull request's.
t_case "the dashboard says why a preview is not serving, with the reason from its state"
frag=$(cat "$T_ROOT/host/docs/dashboard.frag")
t_eq "$(grep -c -F '"build-failed": "unable to create: the build failed"' "$T_ROOT/host/docs/dashboard.frag")" 0
t_contains "$frag" 'const reason = typeof it.reason === "string" ? it.reason.trim() : "";'
t_contains "$frag" 'return reason ? label + ": " + reason : label;'
t_contains "$frag" 'el("span", "when", previewWhy(it))'
t_eq "$(grep -c -F 'el("span", "when", PREVIEW_STATES[it.state]' "$T_ROOT/host/docs/dashboard.frag")" 0

t_case "the pages take an address only with a number of one to nine digits and no leading zero"
t_eq "$(grep -c -F '/^https:\/\/uat-([1-9][0-9]{0,8}|main)\.[a-z0-9.-]+\/$/.test(it.address)' "$T_ROOT/host/docs/dashboard.frag")" 1
t_eq "$(grep -c -F '/^https:\/\/lab-[1-9][0-9]{0,8}\.[a-z0-9.-]+\/$/.test(t.address)' "$T_ROOT/host/fun/game-view.js")" 1
t_eq "$(grep -c -e 'uat-\[0-9\]+' -e 'lab-\[0-9\]+' "$T_ROOT/host/docs/dashboard.frag" "$T_ROOT/host/fun/game-view.js" | grep -c -v ':0$')" 0

t_done
