#!/usr/bin/env bash
# Tests for the front page's own markup: where the live meter stands, what it
# links to, the widths it gives way at, and that nothing door-shaped is left
# around it. It reads the files; it runs no browser.
# shellcheck disable=SC2016  # the patterns searched for are literal file text
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

page=$T_ROOT/host/fun/index.html
css=$T_ROOT/host/fun/energy-view.css
game=$T_ROOT/host/fun/game-view.css
js=$T_ROOT/host/fun/energy-view.js
gamejs=$T_ROOT/host/fun/game-view.js

header=$(awk '/<header class="topbar">/,/<\/header>/' "$page")
doors=$(awk '/<nav class="doors"/,/<\/nav>/' "$page")
# Rules read across lines. Compare them as text, one rule at a time.
flat=$(tr '\n' ' ' < "$css" | tr -s ' ')
gameflat=$(tr '\n' ' ' < "$game" | tr -s ' ')

t_case "the meter stands in the header band, once on the page"
t_contains "$header" 'class="meter"'
t_contains "$header" 'id="meter"'
t_eq "$(grep -c 'id="meter"' "$page")" 1

t_case "the meter and the state tag are one group at the right end of the band"
t_contains "$header" 'class="topbar-end"'
# Every .topbar-end rule, read as declarations rather than as source lines.
endrules=$(printf '%s\n' "$gameflat" | grep -o '\.topbar-end[^{]*{[^}]*}' | tr '\n' ' ')
gaterules=$(printf '%s\n' "$gameflat" | grep -o '\.topbar-end > \.gate[^{]*{[^}]*}' | tr '\n' ' ')
t_contains "$endrules" 'display: flex'
t_contains "$endrules" 'gap: 0 16px'
# The slack of a wrapped line belongs to the group, so the state tag's place
# does not depend on whether the meter is on the line.
t_contains "$endrules" 'margin-left: auto'
t_contains "$gaterules" 'flex: none'
t_contains "$gaterules" 'white-space: nowrap'
t_eq "$(cat "$T_ROOT"/host/fun/*.css | grep -c ':has([^)]*\.meter')" 0

t_case "the doors row holds only the six door links"
t_lacks "$doors" meter
t_eq "$(printf '%s\n' "$doors" | grep -c 'class="door"')" 6

t_case "every element the script reaches by id is in the markup, once"
# The ids it names, and the gauge ids dial() builds from a figure's id.
check_id() { t_eq "$(grep -c "id=\"$1\"" "$page")" 1; }
while IFS= read -r id; do check_id "$id"; done <<< "$(grep -o '\$("[a-z0-9-]*")' "$js" | sed 's/\$("//; s/")//' | sort -u)"
while IFS= read -r id; do check_id "$id"; check_id "$id-gauge"; done <<< "$(grep -o 'dial("[a-z0-9-]*"' "$js" | sed 's/dial("//; s/"//' | sort -u)"

t_case "the script draws the meter in the header, not a door"
t_contains "$(cat "$js")" 'meter.hidden = !data'
t_contains "$(cat "$js")" 'drawMeter(meter)'
t_lacks "$(cat "$js")" 'door-meter'

t_case "no door rule reaches the meter, and the energy stylesheet says nothing about doors"
t_eq "$(printf '%s\n' "$gameflat" | grep -o '\.doors\?[^{]*{[^}]*}' | grep -c meter)" 0
t_lacks "$(cat "$css")" door

t_case "the header band keeps the three heights the page counts on"
for h in 80 128 120; do
  t_eq "$(grep -c -- "--header: $h" "$game")" 1
done

t_case "the meter steps out where the line has no room for it"
t_contains "$flat" '@media (min-width: 1280px) and (max-width: 1439px)'
t_contains "$flat" '@media (max-width: 343px)'
t_contains "$flat" '.topbar-end > .meter { display: none; }'

t_case "the meter's two lines and its gauges show only where they fit"
t_contains "$flat" '@media (min-width: 700px) and (max-width: 1279px), (min-width: 1440px)'
t_contains "$flat" '.meter-tokens { display: flex; }'
t_contains "$flat" '@media (min-width: 700px) and (max-width: 1279px), (min-width: 1650px)'
t_contains "$flat" '.meter-gauge { display: flex; }'
t_contains "$flat" '@media (max-width: 399px)'
t_contains "$flat" '.meter-row b { display: none; }'

t_case "a data-source tag on the line makes the meter narrow, then step out"
t_contains "$flat" '@media (min-width: 700px) and (max-width: 797px)'
t_contains "$flat" '.meter-gauge { display: none; }'
t_contains "$flat" '@media (min-width: 700px) and (max-width: 731px)'
t_contains "$flat" '.meter-tokens { display: none; }'
t_contains "$flat" '@media (min-width: 1440px) and (max-width: 1571px)'
t_contains "$flat" '@media (max-width: 519px)'
t_contains "$flat" ':has(> .source:not([data-kind="live"])) .meter { display: none; }'

t_case "the widths the meter's bands are worked out from are in the file that owns them"
for w in 'brand: 196' 'source tag: 128' 'menu: 792' 'state tag: 152'; do
  t_contains "$gameflat" "$w"
done

t_case "the data-source tag stays inside the width those bands budget"
# 128px is twelve characters of page type. A longer label moves every band.
longest=$(grep -o 'source\.textContent = [^;]*\|show(data, "[A-Z ]*"' "$gamejs" | grep -o '"[A-Z ]*"' | awk '{ n = length($0) - 2; if (n > m) m = n } END { print m }')
t_eq "$(( longest <= 12 ))" 1
# The kinds the stylesheet reads, as the page writes them.
kinds=$(grep -o 'dataset\.kind = [^;]*' "$gamejs" | grep -o '"[a-z]*"' | sed 's/"//g' | sort -u | tr '\n' ' ')
t_eq "$kinds" "export live sample stale "

t_case "the manual says how to reach Energy use where the meter steps out"
t_contains "$(cat "$T_ROOT/host/docs/operating.md")" 'ENERGY USE'
t_contains "$(cat "$T_ROOT/DESIGN.md")" 'energy-view.css'

t_done
