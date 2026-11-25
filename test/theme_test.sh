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
# The stylesheets that draw the pages, and the files that write their markup.
sheets=(manual.css pages.css)
markup=("$T_ROOT/host/docs/_layout.html" "$T_ROOT/host/docs/dashboard.frag" "$T_ROOT/console/items/src/page.rs")

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
# A stylesheet may also set layout tokens of its own (--header) and a script
# may set some on an element: those are not the theme's, and are allowed where
# something sets them.
own=$( (cd "$fun" && cat "${sheets[@]}" && grep -ohE "setProperty\\(\"--[a-z0-9-]+" ./*.js | sed 's/setProperty("//') |
  grep -oE -- '--[a-z0-9-]+(:|$)' | tr -d ':' | sort -u)
used=$(cd "$fun" && cat "${sheets[@]}" | grep -oE 'var\(--[a-z0-9-]+' | sed 's/var(//' | sort -u)
for v in $used; do
  grep -qxF -- "$v" <<<"$pixel" && continue
  grep -qxF -- "$v" <<<"$own" && continue
  t_bad "$v is used and neither a theme nor the stylesheets set it"
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
f=$T_ROOT/host/docs/_layout.html
t_contains "$(cat "$f")" '{{- $design := "beesknees26" }}{{ if eq (env "TEHAS_DESIGN") "pixel" }}{{ $design = "pixel" }}{{ end -}}'
first=$(grep -oE 'href="/fun/theme-[^"]+\.css' "$f" | head -1 | sed 's/.*"//')
t_eq "$first" '/fun/theme-{{ $design }}.css'
t_eq "$(sed -n '1,/theme-{{ \$design }}\.css/p' "$f" | grep -c 'rel="stylesheet"')" 1

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
t_contains "$(cat "$fun/manual.css")" '.brand img { width: auto; height: 40px; }'
t_contains "$(cat "$fun/theme-beesknees26.css")" '.brand .wordmark { width: auto; height: 28px; }'
layout=$(cat "$T_ROOT/host/docs/_layout.html")
t_contains "$layout" '<img src="/fun/factory.svg" width="100" height="50" alt="">'
t_contains "$layout" '<img src="/fun/favicon.svg" width="50" height="50" alt="">'
t_contains "$layout" '<img class="wordmark" src="/fun/tehas-logotype.svg" width="99" height="28" alt="TEHAS">'
t_contains "$layout" '<small>FACTORY</small>'
t_contains "$layout" '/fun/share.png'
t_contains "$layout" '/fun/share-logo.png'

t_case "pages.css holds the 17 rules other pages pick by selector, each once, and no page of tehas links it"
heads=$(sed 's:/\*.*\*/::' "$fun/pages.css" | grep -oE '^[^ /].*\{' | sed 's/ *{$//')
t_eq "$(wc -l <<<"$heads" | tr -d ' ')" 17
t_eq "$(sort <<<"$heads" | uniq -d)" ""
for want in '*' '[hidden]' html body a 'a:focus-visible, button:focus-visible' 'h1, h2, h3, p' b \
  .topbar .mark '.mark img' '.mark small' .panel .panel-head '.panel-head h2' '.panel-head > span' .foot; do
  grep -qxF -- "$want" <<<"$heads" || t_bad "pages.css has no rule for $want"
done
t_eq "$(grep -lF 'pages.css' "${markup[@]}" | xargs)" ""

t_case "no page asks which theme the instance picked"
for f in "${markup[@]}"; do
  t_eq "$(grep -c 'TEHAS_THEME' "$f")" 0
done

t_case "the router strips /fun for the shared assets and rewrites nothing"
look=$(sed -n '/handle @look {/,/file_server/p' "$T_ROOT/host/edge/router.Caddyfile")
t_contains "$look" 'uri strip_prefix /fun'
t_eq "$(grep -c 'rewrite' <<<"$look")" 0

t_case "the docs server sends /floor to the front page, and has no business page"
caddy=$(cat "$T_ROOT/host/edge/docs.Caddyfile")
t_contains "$caddy" 'redir /floor / permanent'
t_eq "$(grep -c 'index.html' <<<"$caddy")" 0
t_eq "$(grep -ciE 'business|TEHAS_DESIGN:' <<<"$caddy")" 0

t_case "no footer links to the other frontend"
foot=$(sed -n '/<footer/,/<\/footer>/p' "$T_ROOT/host/docs/_layout.html")
t_eq "$(grep -cE 'href="/floor"|href="/dashboard"|href="/"' <<<"$foot")" 0

t_case "the plain dashboard loads the host bridge before its islands and the dock after them"
frag=$T_ROOT/host/docs/dashboard.frag
order=$(grep -oE 'src="/fun/[a-z/-]+\.js"' "$frag" | sed 's/src="\/fun\///; s/"//' | xargs)
t_eq "$order" "host-bridge.js factory/tehas-factory-state.js factory/tehas-factory-console.js factory/tehas-factory-issues.js factory/tehas-factory-readonly.js factory/tehas-factory-days.js factory/tehas-factory-previews.js dom.js epics-view.js versions.js dock.js"
t_contains "$(cat "$frag")" '<script type="module" src="/fun/factory/tehas-factory-state.js"></script>'
t_eq "$(grep -c '<script type="module">' "$frag")" 0

t_done
