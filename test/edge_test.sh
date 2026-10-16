#!/usr/bin/env bash
# Tests for lib/edge.sh, what tehasbot protolab and tehasbot preview share,
# and for the router's files in host/edge.
# shellcheck disable=SC2016  # snippets and patterns are expanded later, or never
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

export TEHAS_PROJECT_HOME=$T/home/tehas
lib='. "$T_ROOT/lib/edge.sh"'
d=$T/home/tehas
mkdir -p "$d"

t_case "edge_put writes the whole file or leaves the old one"
echo old >"$d/list.json"
t_run "$lib; edge_put '$d/list.json' echo new; echo rc=\$?"
t_eq "$t_out" "rc=0"
t_eq "$(cat "$d/list.json")" "new"
t_run "$lib; edge_put '$d/list.json' bash -c 'echo half; exit 3'; echo rc=\$?"
t_eq "$t_out" "rc=1"
t_eq "$(cat "$d/list.json")" "new"
t_eq "$(find "$d" -mindepth 1 -maxdepth 1 -printf '%f ')" "list.json "

t_case "edge_place puts a directory in place of the old one"
mkdir -p "$d/site/.work/7" "$d/site/7"
echo fresh >"$d/site/.work/7/index.html"
echo stale >"$d/site/7/index.html"
t_run "$lib; edge_place '$d/site/.work/7' '$d/site/7'; echo rc=\$?"
t_eq "$t_out" "rc=0"
t_eq "$(cat "$d/site/7/index.html")" "fresh"
t_eq "$(find "$d/site" -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" ".work 7 "

t_case "edge_numbers lists the numbered directories only"
mkdir -p "$d/site/12" "$d/site/x1" "$d/site/3a" "$d/site/repo.git"
: >"$d/site/99"
t_run "$lib; edge_numbers '$d/site' | sort -n | tr '\n' ' '"
t_eq "$t_out" "7 12 "
t_run "$lib; edge_numbers '$d/nowhere'; echo rc=\$?"
t_eq "$t_out" "rc=0"

t_case "edge_lock is held by one process at a time"
t_run "$lib; edge_lock demo || exit 9; ( exec {edge_lock_fd}>&-; $lib; edge_lock demo; echo second=\$? ); echo first=ok"
t_eq "$t_out" "second=1
first=ok"
t_run "$lib; edge_lock demo; echo again=\$?"
t_eq "$t_out" "again=0"

# The descriptor is the one bash chose, named in edge_lock_fd. A fixed number
# is one every caller would have to guess at, and two of them would step on
# each other.
t_case "edge_lock names the descriptor it took, and closing that one releases the lock"
t_run "$lib; edge_lock demo || exit 9; fd=\$edge_lock_fd
case \$fd in ''|*[!0-9]*) echo nofd ;; *) [ \"\$fd\" -ge 10 ] && echo chosen ;; esac
exec {edge_lock_fd}>&-
flock -n \"$d/state/demo.lock\" echo free"
t_eq "$t_out" "chosen
free"
t_eq "$(grep -c -E 'flock (-n|-w [0-9]+) 9' "$T_ROOT/lib/edge.sh")" 0

t_case "a command the run starts keeps the lock unless it is told to drop the descriptor"
# What libexec/preview does for every command that can outlive its run: a
# descriptor a child inherits holds the lock after the parent has ended.
# The check waits up to a second: the forked subshell holds the descriptor
# until it starts sleep, and on a busy host that can come after the check.
# The first child keeps it for 2 seconds, so a second's wait still sees it.
t_run "$lib; edge_lock demo || exit 9; ( sleep 2 ) & child=\$!; exec {edge_lock_fd}>&-
if flock -w 1 \"$d/state/demo.lock\" true; then echo free-after-the-parent; else echo held-by-the-child; fi
wait \$child"
t_eq "$t_out" "held-by-the-child"
t_run "$lib; edge_lock demo || exit 9; ( sleep 2 {edge_lock_fd}>&- ) & child=\$!; exec {edge_lock_fd}>&-
if flock -w 1 \"$d/state/demo.lock\" true; then echo free-after-the-parent; else echo held-by-the-child; fi
wait \$child"
t_eq "$t_out" "free-after-the-parent"

# What the header tells a caller to write. Quoted, it is an argument and not a
# redirection, and the child keeps the descriptor and the lock.
t_case "the header names the redirection unquoted"
t_eq "$(grep -c -F '{edge_lock_fd}>&-' "$T_ROOT/lib/edge.sh")" 1
t_eq "$(grep -c -F '"$edge_lock_fd>&-"' "$T_ROOT/lib/edge.sh")" 0

caddyfile=$T_ROOT/host/edge/router.Caddyfile
compose=$T_ROOT/host/edge/router.compose.yaml

t_case "the router's file names no domain and takes numbers only"
t_eq "$(grep -c -E 'tehas\.app|example\.' "$caddyfile")" 0
t_contains "$(cat "$caddyfile")" '@preview header_regexp preview Host ^uat-([1-9][0-9]{0,8})\.'
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

# The sample files stand where a host has what tehas work and tehasbot health
# write. Their keys are pinned to those two, so a change there is seen here.
t_case "the preview's sample running.json is the live view, and its cycle has the keys tehas work writes, but for the console"
written=$(sed -n "/'{issue: \$issue/,/}'/p" "$T_ROOT/libexec/work" | grep -oE '\b[A-Za-z]+: ' | tr -d ': ' | LC_ALL=C sort | tr '\n' ' ')
t_eq "$written" "console issue running startedAt status statusText step updatedAt "
# The view of lib/live.sh: no cycle runs in a preview, and one finished last.
t_eq "$(jq -c '[keys, .cycles]' "$T_ROOT/preview/live/running.json")" '[["cycles","last"],[]]'
# A preview has no console file to show, so the sample names none.
t_eq "$(jq -r '.last | keys_unsorted | join(" ")' "$T_ROOT/preview/live/running.json" | tr ' ' '\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "${written#console }"

t_case "the preview's sample health.json has the keys tehasbot health writes from the samples beside it"
sample=$T/home/sample
mkdir -p "$sample/live"
cp "$T_ROOT/preview/live/running.json" "$sample/live/"
# The sample health file was written when its snapshot was: a fresh one.
jq '.generatedAt = (now | todate)' "$T_ROOT/preview/live/status.json" >"$sample/live/status.json"
t_run "TEHAS_HOME='$sample' '$T_ROOT/libexec/health'; echo rc=\$?"
t_eq "$t_out" "rc=0"
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
t_eq "$(grep -c -F '/^https:\/\/uat-[1-9][0-9]{0,8}\.[a-z0-9.-]+\/$/.test(it.address)' "$T_ROOT/host/docs/dashboard.frag")" 1
t_eq "$(grep -c -F '/^https:\/\/lab-[1-9][0-9]{0,8}\.[a-z0-9.-]+\/$/.test(t.address)' "$T_ROOT/host/fun/game-view.js")" 1
t_eq "$(grep -c -e 'uat-\[0-9\]+' -e 'lab-\[0-9\]+' "$T_ROOT/host/docs/dashboard.frag" "$T_ROOT/host/fun/game-view.js" | grep -c -v ':0$')" 0

t_done
