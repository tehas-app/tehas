#!/usr/bin/env bash
# Tests for the structured data of the pages: the article, breadcrumb and FAQ
# the manual layout writes.
# It reads the templates as text: no server, no rendering.
# shellcheck disable=SC2016  # the template text is a literal
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

layout=$(cat "$T_ROOT/host/docs/_layout.html")

t_case "every manual page is an article under the front page"
t_contains "$layout" '{{- if not $isFrag }}'
t_contains "$layout" '(dict "@type" "TechArticle" "headline" $title "description" $description "url" $url "inLanguage" "en" "isPartOf" (dict "@type" "WebSite" "name" "Tehas" "url" $home))'
t_contains "$layout" '(dict "@type" "ListItem" "position" 1 "name" "Tehas" "item" $home)'
t_contains "$layout" '(dict "@type" "ListItem" "position" 2 "name" $title "item" $url)'

t_case "the data is written by toJson, which escapes what would close the script"
t_contains "$layout" '<script type="application/ld+json">{{ dict "@context" "https://schema.org" "@graph" $graph | toJson }}</script>'
t_eq "$(grep -c 'application/ld+json' "$T_ROOT/host/docs/_layout.html")" 1

t_case "the URLs are built from the \$base the layout already builds"
t_contains "$layout" '{{- $url := printf "%s/%s" $base $path -}}'
t_contains "$layout" '{{- $home := printf "%s/" $base -}}'

t_case "the FAQ is read from the section the front matter names, one question per ###"
t_contains "$layout" '{{- with $doc.Meta.faq -}}'
t_contains "$layout" '{{- $heading := printf "\n## %s\n" . -}}'
t_contains "$layout" '{{- range rest (splitList "\n### " $faq) -}}'
t_contains "$layout" '(dict "@type" "Question" "name" (trim $qa._0) "acceptedAnswer" (dict "@type" "Answer" "text" $answer))'
t_contains "$layout" '{{- $graph = append $graph (dict "@type" "FAQPage" "url" $url "mainEntity" $items) -}}'

t_case "the Pi software factory page names its Questions section, which has questions"
page=$T_ROOT/host/docs/pi-software-factory.md
t_contains "$(sed -n '1,/^---$/{p}' "$page" | sed -n '2,$p')" 'faq: Questions'
t_contains "$(cat "$page")" $'\n## Questions\n'
questions=$(awk '/^## / {on = ($0 == "## Questions")} on && /^### / {n++} END {print n + 0}' "$page")
[ "$questions" -ge 1 ] || t_bad "the Questions section of pi-software-factory.md has no ### question"

t_case "the Lean Software Factory page names its Questions section, which has questions"
page=$T_ROOT/host/docs/lean-software-factory.md
t_contains "$(sed -n '1,/^---$/{p}' "$page" | sed -n '2,$p')" 'faq: Questions'
t_contains "$(cat "$page")" $'\n## Questions\n'
questions=$(awk '/^## / {on = ($0 == "## Questions")} on && /^### / {n++} END {print n + 0}' "$page")
[ "$questions" -ge 1 ] || t_bad "the Questions section of lean-software-factory.md has no ### question"

t_done
