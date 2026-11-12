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
t_contains "$(cat "$T_ROOT/host/fun/dashboard-view.js")" 'const isMain = !!it && it.name === "main";'
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

# What the docs server serves under /live/ is the one list "Files under live/"
# of api.md holds: the file is read as text here because the gate has no caddy.
docs_caddy=$T_ROOT/host/edge/docs.Caddyfile
live_docs=$T_ROOT/host/docs/api.md

# The names of the table, one per line, as the Caddyfile matches them: /<name>,
# with the number of a cycle's file a *. Only the rows under the section's own
# heading count: no line of the page from its heading to the next heading that
# is not one, so a field table elsewhere on the page holds no name here. A row
# begins with a pipe, a space and a backquote; the heading's text holds that
# shape, so the backquote is what tells a row from a heading.
table_names() {
  awk '/^## / {in_section = ($0 == "## Files under live/")}
    in_section && /^\| `/ {split(substr($0, 3), cell, "`"); print cell[2]}' "$live_docs" \
    | sed -e 's|^|/|' -e 's|<n>|*|'
}
# The paths of the @unnamed matcher, which is the edge's copy of the same list:
# the words of its own line. The respond of the matcher is no path, so it is
# not read to: a path on a line of its own is no path of the matcher at all,
# and the case on the list of the matcher says so by finding too few.
unnamed_paths() {
  awk '/^[[:space:]]*@unnamed not path / {print}' "$docs_caddy" | tr -s '[:space:]' '\n' | grep '^/'
}

t_case "the list the docs server answers 404 for is the list api.md names, both ways"
table=$(table_names)
unnamed=$(unnamed_paths)
# A case that looks at nothing passes by finding nothing, so both sides are
# named here before they are compared.
[ -n "$table" ] || t_bad "no row read from the table of Files under live/ in api.md"
[ -n "$unnamed" ] || t_bad "no path read from @unnamed of the docs server's Caddyfile"
t_eq "$(comm -23 <(printf '%s\n' "$table" | LC_ALL=C sort) <(printf '%s\n' "$unnamed" | LC_ALL=C sort) | tr '\n' ' ')" ""
t_eq "$(comm -13 <(printf '%s\n' "$table" | LC_ALL=C sort) <(printf '%s\n' "$unnamed" | LC_ALL=C sort) | tr '\n' ' ')" ""
# Both sides are sets: a name written twice on one side and once on the other
# is an agreement no comm above can see. Neither list may hold a name twice.
t_eq "$(printf '%s\n' "$table" | LC_ALL=C sort -u | tr '\n' ' ')" \
  "$(printf '%s\n' "$table" | LC_ALL=C sort | tr '\n' ' ')"
t_eq "$(printf '%s\n' "$unnamed" | LC_ALL=C sort -u | tr '\n' ' ')" \
  "$(printf '%s\n' "$unnamed" | LC_ALL=C sort | tr '\n' ' ')"

t_case "the live list holds the 21 names the dashboard, the console and the server read"
t_eq "$(printf '%s\n' "$table" | wc -l)" 21
for want in /status.json /running.json /endings.json /epics.json /health.json /health.down \
  /dispatch.json /demand.json /energy.json /analytics.json /orders.json /top.json \
  /panels.json /disk.json /platform.json /rukas.json /previews.json /protolab.json \
  /cycles/*.json /console-*.txt /console-*.txt.now; do
  printf '%s\n' "$table" | grep -qxF -- "$want" || t_bad "the list does not name $want"
done

t_case "a new file under live/ is served only once both lists name it"
# A name in the table but not the matcher, or the other way about, is a name no
# reader reaches: the case above is the one that fails, and it fails both ways.
without=$(printf '%s\n' "$unnamed" | grep -vx '/disk.json' | LC_ALL=C sort)
t_eq "$(comm -23 <(printf '%s\n' "$table" | LC_ALL=C sort) <(printf '%s\n' "$without") | tr '\n' ' ')" "/disk.json "
extra=$(printf '%s\n%s\n' "$unnamed" /secret.json | LC_ALL=C sort)
t_eq "$(comm -13 <(printf '%s\n' "$table" | LC_ALL=C sort) <(printf '%s\n' "$extra") | tr '\n' ' ')" "/secret.json "

t_case "in the live handler a half-written file, a dotfile and an unnamed name are all a 404 before the files are served"
live=$(awk '/^        handle_path \/live\/\* \{/,/^        \}/' "$docs_caddy")
# The case reads the handler; an empty extraction would pass by finding nothing.
[ -n "$live" ] || t_bad "the live handler was not read from $docs_caddy"
for want in '@unfinished path' '@unnamed not path'; do
  # A respond naming a matcher nothing defines is a Caddyfile caddy cannot
  # parse, and the gate has no caddy to ask: the line defining the matcher is
  # held there by its own shape - the name whole as the first word of the line
  # and the rule after it - so no respond can answer on its own. The paths of
  # the list are words too, so they may not be read from this line's tail.
  t_eq "$(printf '%s\n' "$live" | awk -v want="$want" '\
    $1 == substr(want, 1, index(want " ", " ") - 1) && substr($0, index($0, want), length(want)) == want && \
      substr($0, index($0, want) + length(want), 1) == " " {n++} END {print n + 0}')" 1
done
# The half of the rule that keeps a half-written file and a dotfile out names
# both of them: a matcher of the right shape missing one of the two answers 200
# for a file no reader may see, and nothing else in this file would say so.
t_contains "$(printf '%s\n' "$live" | grep '@unfinished path')" '*.new'
t_contains "$(printf '%s\n' "$live" | grep '@unfinished path')" '/.*'
for want in 'respond @unfinished 404' 'respond @unnamed 404'; do
  # The rule matched exactly, spaces and all: where a respond of the handler
  # sits in the file is the order this case is about.
  t_eq "$(printf '%s\n' "$live" | awk -v want="$want" '$0 ~ ("^[[:space:]]*" want "$") {n++} END {print n + 0}')" 1
  # The matcher's name as its own word, not a line that holds it among other
  # text, which is what ties this respond to the rule above it.
  matcher=${want#respond }; matcher=${matcher% 404}
  t_eq "$(printf '%s\n' "$live" | awk -v m="$matcher" '{for (i = 2; i <= NF; i++) if ($i == m) {n++; break}} END {print n + 0}')" 1
  # The first of each, rather than the last of one and the first of the other:
  # a second respond of the name in front of this one is not read as the rule
  # that comes before the files are served.
  respond_line=$(printf '%s\n' "$live" | grep -n -m1 "^[[:space:]]*$want$")
  server_line=$(printf '%s\n' "$live" | grep -n -m1 'file_server')
  [ -n "$respond_line" ] && [ -n "$server_line" ] ||
    t_bad "no respond of $want or no file_server in the live handler"
  t_eq "$((10#${respond_line%%:*} < 10#${server_line%%:*} ? 1 : 0))" 1
done
# The list of the names the handler serves holds health.json and health.down,
# so the matcher's own line is the one line of the handler that may name
# health, and no line of it may name the matcher kept for indexing: that one
# is for the pages, and @unnamed is a matcher of its own.
t_eq "$(printf '%s\n' "$live" | grep -v 'not path' | grep -v '^[[:space:]]*#' | grep -c 'health\|unlisted')" 0
# @unlisted is the indexing matcher of line 13, and its name stays its own:
# this handler answers with a matcher of another name, so the two cannot be
# read as one rule.
t_eq "$(awk '/^    @unlisted expression / {n++} END {print n + 0}' "$docs_caddy")" 1
t_contains "$live" '@unnamed'

t_case "the two health addresses answer 200 with the health file under root /srv/live"
# Outside /live/*, so @unnamed never sees them; the rewrite names the file.
t_eq "$(awk '/handle @health \{/ {h = NR} /root \* \/srv\/live/ && h && !r {r = NR} /rewrite \* \/health\.json/ && r && !w {w = NR}
  END {print (h && r && w) ? "live root, rewritten" : "no"}' "$docs_caddy")" "live root, rewritten"
# The 404 of an unnamed name comes before this handler, so a name the list
# does not hold cannot reach it by a path under /live/ either.
t_eq "$(awk '/^        handle @health \{[[:space:]]*$/ {a = NR} /^[[:space:]]*respond @unnamed 404$/ {u = NR} END {print (u && a && u < a) ? "before" : "after"}' "$docs_caddy")" before
health=$(awk '/^        handle @health \{/,/^        \}/' "$docs_caddy")
t_eq "$(printf '%s\n' "$health" | grep -c 'application/health+json')" 1
t_eq "$(printf '%s\n' "$health" | grep -c 'file_server')" 1
# 200: no status line of its own, where the down handler gives one.
t_eq "$(printf '%s\n' "$health" | grep -c 'status ')" 0

t_case "while live/health.down is there the two health addresses answer 503, and that handler comes first"
# The marker is read off the same root, by try_files, and it is the handler
# that answers before @health - the one whose file_server says 503.
t_contains "$(cat "$docs_caddy")" 'try_files /health.down'
t_eq "$(awk '/@health_down \{/ {d = NR} /handle @health \{/ {h = NR} END {print (d && h && d < h) ? "before" : "after"}' "$docs_caddy")" before
handler=$(awk '/^        handle @health_down \{/,/^        \}/' "$docs_caddy")
t_eq "$(printf '%s\n' "$handler" | grep -c 'status 503')" 1
t_eq "$(printf '%s\n' "$handler" | grep -c 'rewrite \* /health.json')" 1
t_eq "$(printf '%s\n' "$handler" | grep -c 'root \* /srv/live')" 1

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
# The cleaner's record of the upkeep that made it.
printf '{"at":%s,"ok":true}\n' "$(date +%s)" >"$sample/state/upkeep.json"
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
t_eq "$(grep -c -F 'note.push("List written " + ago(p.generatedAt) + ".");' "$T_ROOT/host/fun/dashboard-view.js")" 1

# A refused volume and a build that failed are both "build-failed": only the
# reason the run wrote into the state says which. The reason is text this
# command wrote, never the pull request's.
t_case "the dashboard says why a preview is not serving, with the reason from its state"
view=$(cat "$T_ROOT/host/fun/dashboard-view.js")
t_eq "$(grep -c -F '"build-failed": "unable to create: the build failed"' "$T_ROOT/host/fun/dashboard-view.js")" 0
t_contains "$view" 'const reason = typeof it.reason === "string" ? it.reason.trim() : "";'
t_contains "$view" 'return reason ? label + ": " + reason : label;'
t_contains "$view" 'el("span", "when", previewWhy(it))'
t_eq "$(grep -c -F 'el("span", "when", PREVIEW_STATES[it.state]' "$T_ROOT/host/fun/dashboard-view.js")" 0

t_case "the pages take an address only with a number of one to nine digits and no leading zero"
t_eq "$(grep -c -F '/^https:\/\/uat-([1-9][0-9]{0,8}|main)\.[a-z0-9.-]+\/$/.test(it.address)' "$T_ROOT/host/fun/dashboard-view.js")" 1
t_eq "$(grep -c -F '/^https:\/\/lab-[1-9][0-9]{0,8}\.[a-z0-9.-]+\/$/.test(t.address)' "$T_ROOT/host/fun/game-view.js")" 1
t_eq "$(grep -c -e 'uat-\[0-9\]+' -e 'lab-\[0-9\]+' "$T_ROOT/host/fun/dashboard-view.js" "$T_ROOT/host/fun/game-view.js" | grep -c -v ':0$')" 0

# The dashboard's files are compressed on their way out and cached by the
# browser: the scripts and stylesheets under a name that holds their own bytes,
# and the pages by no name at all, so a page is asked about every time. The gate
# has no caddy to ask, so the rules are read as they are written.
t_case "the docs server compresses what it serves, but not what a Range request reads"
# The two addresses and the tree the dashboard's console is read from by
# offsets into the file's own bytes: an encoded body has none of those, so these
# three must be left out of the matcher of encode, not merely served as well.
encode=$(awk '/^[[:space:]]*@compress not path / {print}' "$docs_caddy")
[ -n "$encode" ] || t_bad "no @compress matcher in $docs_caddy"
# The words of the line, as the matcher's own list of paths: the matcher's name
# and its two words of rule are no paths, so the count says the list holds just
# the three the console and the health page are read from.
paths=$(printf '%s\n' "$encode" | tr -s '[:space:]' '\n' | grep '^/')
for want in '/live/*' '/health' '/healthz'; do
  printf '%s\n' "$paths" | grep -qxF -- "$want" ||
    t_bad "@compress does not leave out $want: $encode"
done
# The matcher names nothing else: a path taken out of it is a file that is
# compressed for no reason and asked for with an offset it no longer has.
t_eq "$(printf '%s\n' "$paths" | wc -l)" 3
t_eq "$(printf '%s\n' "$paths" | LC_ALL=C sort -u | wc -l)" 3
t_eq "$(grep -c -E '^[[:space:]]*encode @compress zstd gzip$' "$docs_caddy")" 1
t_eq "$(grep -c -E '^[[:space:]]*encode ' "$docs_caddy")" 1

t_case "a hashed script or style is cached for ever, and only a hashed one"
fun=$(awk '/handle_path \/fun\/[*] [{]/ {on = 1} on {print} on && /^[[:space:]]*\}[[:space:]]*$/ {exit}' "$docs_caddy")
# An empty extraction would pass by finding nothing.
[ -n "$fun" ] || t_bad "the /fun handler was not read from $docs_caddy"
t_eq "$(printf '%s\n' "$fun" | grep -c -F 'header @hashed Cache-Control "public, max-age=31536000, immutable"')" 1
# The matcher of the immutable header names the form console update assets
# makes: the stem, the 12 hex digits of the file's sha256, and the extension.
hashed=$(printf '%s\n' "$fun" | grep '@hashed ')
[ -n "$hashed" ] || t_bad "no @hashed matcher of the /fun handler"
t_contains "$hashed" '[0-9a-f]{12}'
t_contains "$hashed" '\.(js|css)$'
# A name of the plain form asks for the file, and gets no such header: nothing
# here may promise a browser that theme-beesknees26.css never changes. The
# header is the one line of its kind, asked for with the word before it.
t_eq "$(printf '%s\n' "$fun" | grep -c -E '^[[:space:]]*header ')" 1
t_eq "$(printf '%s\n' "$fun" | grep -c '@hashed')" 2
# The 404 of a backup comes first: install.sh keeps a replaced file beside the
# one it replaced, and an immutable header on the name of a backup would cache
# the bytes of the file before it.
t_eq "$(printf '%s\n' "$fun" | awk '/respond @backup 404/ {r = NR} /header @hashed / {h = NR} END {print (r && h && r < h) ? "before" : "after"}')" before

t_case "the pages are never cached, so a new install is seen at once"
for handler in '@business' '@front'; do
  body=$(awk -v want="        handle $handler {" 'index($0, want) == 1 {on = 1}
    on {print} on && /^        \}$/ {exit}' "$docs_caddy")
  [ -n "$body" ] || t_bad "no handler of $handler in $docs_caddy"
  t_eq "$(printf '%s\n' "$body" | grep -c -F 'header Cache-Control "no-cache"')" 1
done
# The closing handler is the layout every manual page is read through, and it
# answers after the rewrite of the path, where a matcher of the page's own name
# would no longer match.
layout=$(awk '/^        rewrite \* \/_layout\.html$/ {on = 1} on {print} on && /^    \}$/ {exit}' "$docs_caddy")
[ -n "$layout" ] || t_bad "no layout handler at the end of $docs_caddy"
t_eq "$(printf '%s\n' "$layout" | grep -c -F 'header Cache-Control "no-cache"')" 1
t_eq "$(printf '%s\n' "$layout" | grep -c 'rewrite \* /_layout.html')" 1

t_done
