#!/usr/bin/env bash
# Tests for the themes: host/fun/theme-<name>.css. Every theme sets the same
# tokens, the pages use no token a theme does not set, and nothing outside a
# theme names a colour, so a theme is the whole of the look. Every page names
# the theme file of the instance's design, TEHAS_DESIGN: beesknees26 unless it
# says pixel. It reads the files as they are in the checkout: no browser, no
# server.
# shellcheck disable=SC2016  # the templates and the compose file are matched as text
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

fun=$T_ROOT/host/fun
themes=$(cd "$fun" && ls theme-*.css)
# The stylesheets that draw the pages, and the files that write their markup or
# paint their canvases.
sheets=(game-view.css manual.css protolab.css energy-view.css host-view.css business.css)
markup=("$fun/index.html" "$fun/business.html" "$T_ROOT/host/docs/_layout.html" "$T_ROOT/host/docs/dashboard.frag" "$T_ROOT/console/src/protolab.rs")

# The tokens a file sets: every custom property declared in it.
sets() { grep -oE '^ *--[a-z0-9-]+:' "$1" | tr -d ' :' | sort -u; }

t_case "there is one theme for each frontend, pixel among them"
t_eq "$(xargs <<<"$themes")" "theme-beesknees26.css theme-pixel.css"

t_case "every theme sets the same tokens"
pixel=$(sets "$fun/theme-pixel.css")
for th in $themes; do
  t_eq "$(sets "$fun/$th" | xargs)" "$(xargs <<<"$pixel")"
done

t_case "every theme draws an outline at its edge width, as the layout counts"
for th in $themes; do
  t_contains "$(cat "$fun/$th")" '--frame: var(--edge) solid var(--line);'
done

t_case "the pages use no token that a theme does not set"
# A stylesheet may also set layout tokens of its own (--header, --meter-column)
# and a script may set some on an element (--fold-pad): those are not the
# theme's, and are allowed where something sets them.
own=$( (cd "$fun" && cat "${sheets[@]}" && grep -ohE "setProperty\\(\"--[a-z0-9-]+" ./*.js | sed 's/setProperty("//') |
  grep -oE -- '--[a-z0-9-]+(:|$)' | tr -d ':' | sort -u)
used=$(cd "$fun" && cat "${sheets[@]}" | grep -oE 'var\(--[a-z0-9-]+' | sed 's/var(//' | sort -u)
for v in $used; do
  grep -qxF -- "$v" <<<"$pixel" && continue
  grep -qxF -- "$v" <<<"$own" && continue
  t_bad "$v is used and neither a theme nor the stylesheets set it"
done

t_case "a canvas paints only with tokens a theme sets"
# energy-view.js names its bands, brick-wall.js its pieces, and both draw
# through paint("<token>").
painted=$(cd "$fun" && {
  grep -oE 'paint\("[a-z0-9-]+"' energy-view.js game-view.js | sed 's/.*paint("//; s/"$//'
  grep -oE '"(game|piece)-[a-z0-9-]+"' game-view.js | tr -d '"'
  sed -n 's/^export const BANDS = \[\(.*\)\];$/\1/p' energy-view.js | tr -d '", ' | tr '[:space:]' ' ' | sed 's/band/ band/g'
  sed -n 's/^ *const COLORS = \[\(.*\)\];$/\1/p' brick-wall.js | tr -d '",' | sed 's/piece/ piece/g'
} | xargs -n1 | sort -u)
t_eq "$(wc -l <<<"$painted" | tr -d ' ')" "$(grep -cE '^(band|piece|game)-|^(screen|screen-quiet|surface|font-code)$' <<<"$painted")"
t_contains "$painted" band-8
t_contains "$painted" piece-4
for v in $painted; do
  grep -qxF -- "--$v" <<<"$pixel" || t_bad "--$v is painted and no theme sets it"
done

t_case "nothing outside a theme names a colour"
for f in "${sheets[@]}"; do
  t_eq "$(grep -cE '[:,( ]#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(|:\s*(white|black|red|green|blue|gr[ae]y)\b' "$fun/$f")" 0
done
for f in "$fun"/*.js "${markup[@]}"; do
  t_eq "$(grep -cE '"#[0-9a-fA-F]{3,8}"|rgba?\(|hsla?\(|theme-color' "$f")" 0
done
# Nor a typeface: Topaz is pixel's.
t_eq "$(cd "$fun" && grep -lE '(font[a-z-]*:|font =)[^;]*Topaz' "${sheets[@]}" ./*.js | xargs)" ""

t_case "pixel keeps the Twelve Bit Rule"
t_eq "$(grep -oE '#[0-9a-fA-F]+\b' "$fun/theme-pixel.css" | grep -cvE '^#[0-9a-f]{3}$')" 0

t_case "every page names the theme of the instance's design, beesknees26 unless it says pixel, before its own styles"
for f in "$fun/index.html" "$fun/business.html" "$T_ROOT/host/docs/_layout.html"; do
  t_contains "$(cat "$f")" '{{- $design := "beesknees26" }}{{ if eq (env "TEHAS_DESIGN") "pixel" }}{{ $design = "pixel" }}{{ end -}}'
  first=$(grep -oE 'href="/fun/theme-[^"]+\.css' "$f" | head -1 | sed 's/.*"//')
  t_eq "$first" '/fun/theme-{{ $design }}.css'
  t_eq "$(sed -n '1,/theme-{{ \$design }}\.css/p' "$f" | grep -c 'rel="stylesheet"')" 1
done
# The Protolab's pages are written by the console: its look() names both.
look=$(sed -n '/^fn look(design: &str)/,/^}/p' "$T_ROOT/console/src/protolab.rs")
for want in 'design == "pixel"' '"theme-pixel.css"' '"factory.svg' '"theme-beesknees26.css"' '"favicon.svg'; do
  t_contains "$look" "$want"
done
order=$(grep -oE 'src="/fun/[a-z-]+\.js"' "$fun/index.html" | sed 's/src="\/fun\///; s/"//' | xargs)
t_contains "$order" "theme.js sample-issues.js brick-wall.js"
t_contains "$order" "game-view.js energy-view.js"

t_case "no page and no server asks for a theme to be chosen"
for f in "${markup[@]}" "$T_ROOT/host/edge/docs.Caddyfile" "$T_ROOT/host/edge/router.Caddyfile"; do
  t_eq "$(grep -cE '/fun/theme\.css|rewrite /theme\.css' "$f")" 0
done

t_case "the docs server is given the design, beesknees26 unless the instance says pixel, and the router none"
edge=$T_ROOT/host/edge
t_eq "$(cat "$edge"/*.Caddyfile "$edge"/*.compose.yaml | grep -cE 'TEHAS_THEME|TEHAS_LAYOUT')" 0
t_contains "$(cat "$edge/docs.compose.yaml")" 'TEHAS_DESIGN: "${TEHAS_DESIGN:-beesknees26}"'
t_eq "$(cat "$edge/router.Caddyfile" "$edge/router.compose.yaml" | grep -c 'TEHAS_DESIGN')" 0

t_case "pixel shows the animated factory and its share card, beesknees26 the logo and its card"
t_contains "$(cat "$fun/favicon.svg")" 'tehas factory icon'
t_contains "$(cat "$fun/factory.svg")" 'turbine-rotor'
t_contains "$(cat "$fun/theme-beesknees26.css")" '.brand img { width: 50px; height: 50px; }'
t_contains "$(cat "$fun/game-view.css")" '.brand img { width: auto; height: 40px; }'
for f in "$fun/index.html" "$T_ROOT/host/docs/_layout.html"; do
  t_contains "$(cat "$f")" '<img src="/fun/factory.svg" width="100" height="50" alt="">'
  t_contains "$(cat "$f")" '<img src="/fun/favicon.svg" width="50" height="50" alt="">'
done
for f in "$fun/index.html" "$fun/business.html" "$T_ROOT/host/docs/_layout.html"; do
  t_contains "$(cat "$f")" '/fun/share.png'
  t_contains "$(cat "$f")" '/fun/share-logo.png'
done

t_case "the business page's header shows the logo, not the old mark"
mark=$(sed -n '/<a class="mark"/,/<\/a>/p' "$fun/business.html")
t_contains "$mark" '<img src="/fun/favicon.svg" width="26" height="26" alt="">'
t_contains "$mark" 'aria-label="Tehas dashboard, top of page"'
t_eq "$(grep -cE 'm-(box|glyph|bar)|<svg' <<<"$mark")" 0
t_eq "$(grep -cE 'm-(box|glyph|bar)|\.mark svg' "$fun/business.css")" 0
t_contains "$(cat "$fun/business.css")" '.mark img { display: block; width: 26px; height: 26px; }'

t_case "no page asks which theme the instance picked"
for f in "${markup[@]}"; do
  t_eq "$(grep -c 'TEHAS_THEME' "$f")" 0
done

t_case "the router strips /fun for the shared assets and rewrites nothing"
look=$(sed -n '/handle @look {/,/file_server/p' "$T_ROOT/host/edge/router.Caddyfile")
t_contains "$look" 'uri strip_prefix /fun'
t_eq "$(grep -c 'rewrite' <<<"$look")" 0

t_case "the docs server answers / with the business page unless the design is pixel, and keeps /floor and /business"
caddy=$(cat "$T_ROOT/host/edge/docs.Caddyfile")
t_contains "$caddy" "@business expression \`path('/business') || (path('/') && \"{\$TEHAS_DESIGN:beesknees26}\" != \"pixel\")\`"
t_contains "$caddy" 'rewrite * /business.html'
t_contains "$caddy" '@front path / /floor'
t_eq "$(awk '/handle @business \{/ {a = NR} /handle @front \{/ {b = NR} END {print (a && a < b) ? "before" : "after"}' "$T_ROOT/host/edge/docs.Caddyfile")" before

t_case "no footer links to the other frontend"
for f in "$fun/index.html" "$fun/business.html" "$T_ROOT/host/docs/_layout.html"; do
  foot=$(sed -n '/<footer/,/<\/footer>/p' "$f")
  t_eq "$(grep -cE 'href="/floor"|href="/dashboard"|href="/"' <<<"$foot")" 0
done

t_case "the business page reads the dispatcher's files and loads its sample first"
order=$(grep -oE 'src="/fun/[a-z-]+\.js"' "$fun/business.html" | sed 's/src="\/fun\///; s/"//' | xargs)
t_eq "$order" "mode.js sample-issues.js console-tabs.js business-view.js epics-view.js versions.js panels.js"
t_contains "$(cat "$fun/business.html")" '<script type="module" src="/fun/business-view.js">'
t_eq "$(grep -oE '"/live/[a-z]+\.json"' "$fun/business-view.js" | tr -d '"' | sort -u | xargs)" '/live/analytics.json /live/energy.json /live/protolab.json /live/running.json /live/status.json'

t_case "the plain dashboard loads the tabs before its module, which holds the page's script"
frag=$T_ROOT/host/docs/dashboard.frag
order=$(grep -oE 'src="/fun/[a-z-]+\.js"' "$frag" | sed 's/src="\/fun\///; s/"//' | xargs)
t_eq "$order" "console-tabs.js dashboard-view.js"
t_contains "$(cat "$frag")" '<script type="module" src="/fun/dashboard-view.js"></script>'
t_eq "$(grep -c '<script type="module">' "$frag")" 0

t_done
