#!/usr/bin/env bash
# Tests for the pages that tell an agent how to find the MCP server:
# shellcheck disable=SC2016  # the template text is a literal
# host/docs/llms.txt and host/docs/agents.md.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

tools="factory_status factory_issues factory_endings factory_console factory_stats factory_issue factory_sessions factory_session factory_log factory_failures factory_decisions"

# llms.txt is a Caddy template and builds the address from the request.
# agents.md is plain Markdown, so it holds a placeholder.
for page in llms.txt agents.md; do
  text=$(cat "$T_ROOT/host/docs/$page")
  case $page in
    llms.txt) address='{{ $base }}/mcp' ;;
    *) address='https://<domain>/mcp' ;;
  esac

  t_case "$page: has the MCP section and its address"
  t_contains "$text" "Ask this factory over MCP"
  t_contains "$text" "$address"
  t_contains "$text" "Authorization: Bearer <key>"

  t_case "$page: names all ten tools"
  for tool in $tools; do
    t_contains "$text" "$tool"
  done

  t_case "$page: holds no address of this host"
  t_lacks "$text" "https://tehas.app/mcp"
done

t_case "mcp-server.md: has a client config block"
t_contains "$(cat "$T_ROOT/host/docs/mcp-server.md")" '"headers"'

# The factory floor left core in 2.0: /floor only sends a reader to the front
# page, so no page links it. The pattern is checked against its samples first:
# a pattern that matches nothing would pass every page.
floor_re='\]\((\{\{ \$base \}\})?/floor\)'
t_case "no page of the manual or for agents links the factory floor"
grep -qE -- "$floor_re" <<< 'a [x](/floor) and a [y]({{ $base }}/floor) link' \
  || t_bad "the pattern finds no link to /floor"
grep -qE -- "$floor_re" <<< '[x](/floorplan) and [y](/dashboard)' \
  && t_bad "the pattern matches a link that is not /floor"
floors=$(grep -rnE -- "$floor_re" "$T_ROOT/host/docs" || true)
[ -z "$floors" ] || t_bad "links /floor: $floors"

t_case "llms-full.txt is llms.txt and then every page's body under its address"
full=$(cat "$T_ROOT/host/docs/llms-full.txt")
t_contains "$full" '{{ include "/llms.txt" }}'
t_contains "$full" '{{- range sortAlpha (listFiles "/") }}{{ if hasSuffix ".md" . }}'
t_contains "$full" '{{- $doc := splitFrontMatter (readFile .) }}'
t_contains "$full" 'Address: {{ $base }}/{{ trim'"Suffix"' ".md" . }}'
t_contains "$full" '{{ trim $doc.Body }}'

t_case "llms-full.txt builds its address only from a plain scheme and host"
t_contains "$full" '{{- if or (not (has $proto (list "http" "https"))) (not (regexMatch "^[A-Za-z0-9.-]+(:[0-9]+)?$" .Req.Host)) }}{{ httpError 400 }}{{ end -}}'

t_case "the docs server renders llms-full.txt as text and keeps it out of the index"
docs_caddy=$T_ROOT/host/edge/docs.Caddyfile
t_contains "$(cat "$docs_caddy")" '@llms path /llms.txt /llms-full.txt /robots.txt /sitemap.xml'
t_contains "$(cat "$docs_caddy")" '@data path /live/* /health /healthz *.md /llms-full.txt'

t_case "llms.txt, robots.txt and agents.md point to llms-full.txt"
t_contains "$(cat "$T_ROOT/host/docs/llms.txt")" 'The whole manual in one text is at {{ $base }}/llms-full.txt.'
t_contains "$(cat "$T_ROOT/host/docs/robots.txt")" '# The whole manual for agents, in one text: {{ $base }}/llms-full.txt'
t_contains "$(cat "$T_ROOT/host/docs/agents.md")" '| `/llms-full.txt`'

t_done
