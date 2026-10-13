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

header=$(awk '/<header class="topbar">/,/<\/header>/' "$page")
doors=$(awk '/<nav class="doors"/,/<\/nav>/' "$page")

t_case "the meter stands in the header band, once on the page"
t_contains "$header" 'class="meter"'
t_contains "$header" 'id="meter"'
t_eq "$(grep -c 'id="meter"' "$page")" 1

t_case "the doors row holds only the six door links"
t_lacks "$doors" meter
t_eq "$(printf '%s\n' "$doors" | grep -c 'class="door"')" 6

t_case "the meter still opens Energy use and keeps the ids the script writes"
t_contains "$header" 'href="#energy"'
for id in meter-watts meter-watts-gauge meter-tokens meter-tokens-row meter-tokens-gauge; do
  t_eq "$(grep -c "id=\"$id\"" "$page")" 1
done

t_case "the script reads the moved element and not the door"
t_eq "$(grep -c '$("meter")' "$js")" 2
t_lacks "$(cat "$js")" 'door-meter'

t_case "no door-shaped rule for the meter is left in the stylesheets"
t_eq "$(cat "$T_ROOT"/host/fun/*.css | grep -c '\.door\.meter')" 0
t_eq "$(cat "$T_ROOT"/host/fun/*.css | grep -c ':has(\.meter')" 0
t_contains "$(cat "$css")" '.topbar > .meter'
t_contains "$(cat "$css")" '.meter-figure i { font-style: normal; }'

t_case "the header band keeps the three heights the page counts on"
for h in 80 128 120; do
  t_eq "$(grep -c -- "--header: $h" "$game")" 1
done

t_case "the meter shows only what the band has room for, band by band"
# The rules read across lines, so compare them as one line of text.
flat=$(tr '\n' ' ' < "$css" | tr -s ' ')
for rule in \
  '@media (min-width: 700px) and (max-width: 1279px), (min-width: 1440px) { #meter-tokens-row { display: flex; } }' \
  '@media (min-width: 700px) and (max-width: 1279px), (min-width: 1650px) { .meter-gauge { display: flex; } }' \
  '@media (min-width: 1280px) and (max-width: 1439px) { .topbar > .meter { display: none; } }' \
  '@media (max-width: 399px) { .meter-row b { display: none; } }' \
  '@media (max-width: 343px) { .topbar > .meter { display: none; } .topbar:has(> .meter:not([hidden])) > .gate { margin-left: auto; } }'; do
  t_contains "$flat" "$rule"
done

t_case "below 1280px the meter takes the slack of the line, not the state tag"
for rule in \
  '@media (max-width: 1279px) { .topbar > .meter { margin-left: auto; } .topbar:has(> .meter:not([hidden])) > .gate { margin-left: 0; } }' \
  '@media (max-width: 519px) { .topbar:has(.source:not([data-kind="live"])) > .meter { display: none; } .topbar:has(.source:not([data-kind="live"])) > .gate { margin-left: auto; } }'; do
  t_contains "$flat" "$rule"
done

t_case "a data-source tag on the line makes the meter narrow, then step out"
for rule in \
  '@media (min-width: 700px) and (max-width: 797px) { .topbar:has(.source:not([data-kind="live"])) .meter-gauge { display: none; } }' \
  '@media (min-width: 700px) and (max-width: 731px) { .topbar:has(.source:not([data-kind="live"])) #meter-tokens-row { display: none; } }' \
  '@media (min-width: 1440px) and (max-width: 1571px) { .topbar:has(.source:not([data-kind="live"])) > .meter { display: none; } }'; do
  t_contains "$flat" "$rule"
done

t_case "the design note and the manual state those widths"
design=$(cat "$T_ROOT/DESIGN.md")
manual=$(cat "$T_ROOT/host/docs/operating.md")
for w in 731px 798px 520px 1571px 344px; do
  t_contains "$design" "$w"
done
for w in 1439px 798px 520px 1571px; do
  t_contains "$manual" "$w"
done

t_done
