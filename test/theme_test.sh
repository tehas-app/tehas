#!/usr/bin/env bash
# Tests for the themes: host/fun/theme-<name>.css. Every theme sets the same
# tokens, the pages use no token a theme does not set, and nothing outside a
# theme names a colour, so a theme is the whole of the look. The servers answer
# /fun/theme.css with the theme the instance names. It reads the files as they
# are in the checkout: no browser, no server.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

fun=$T_ROOT/host/fun
themes=$(cd "$fun" && ls theme-*.css)
# The stylesheets that draw the pages, and the files that write their markup or
# paint their canvases.
sheets=(game-view.css manual.css protolab.css energy-view.css host-view.css)
markup=("$fun/index.html" "$T_ROOT/host/docs/_layout.html" "$T_ROOT/host/docs/dashboard.frag" "$T_ROOT/libexec/protolab")

# The tokens a file sets: every custom property declared in it.
sets() { grep -oE '^ *--[a-z0-9-]+:' "$1" | tr -d ' :' | sort -u; }

t_case "there are at least two themes, pixel among them"
t_contains "$themes" theme-pixel.css
t_eq "$(wc -w <<<"$themes" | tr -d ' ')" 2

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

t_case "every page loads the theme before its own styles, and the canvases' reader before their scripts"
for f in "${markup[@]}"; do
  [ "$f" = "$T_ROOT/host/docs/dashboard.frag" ] && continue
  first=$(grep -oE 'href=\\?"/fun/[a-z-]+\.css' "$f" | head -1 | sed 's/.*"//')
  t_eq "$first" /fun/theme.css
done
order=$(grep -oE 'src="/fun/[a-z-]+\.js"' "$fun/index.html" | sed 's/src="\/fun\///; s/"//' | xargs)
t_contains "$order" "theme.js sample-issues.js brick-wall.js"
t_contains "$order" "game-view.js energy-view.js"

t_case "both servers answer /fun/theme.css with the instance's theme, pixel without one"
# shellcheck disable=SC2016  # the server's and compose's own syntax, kept literal
for f in docs router; do
  t_contains "$(cat "$T_ROOT/host/edge/$f.Caddyfile")" 'rewrite /theme.css /theme-{$TEHAS_THEME:pixel}.css'
  t_contains "$(cat "$T_ROOT/host/edge/$f.compose.yaml")" 'TEHAS_THEME: "${TEHAS_THEME:-pixel}"'
done

t_done
