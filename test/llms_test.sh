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

# Since TEHAS_DESIGN the root is the factory floor only with TEHAS_DESIGN=pixel;
# with the default design it is the business page. /floor and /business are
# there in both designs, so a manual page names the page it means. The rule is
# about Markdown links; a page's own navigation is HTML and not a link a reader
# is told to follow.
root_re='\]\((\{\{ \$base \}\})?/\)'

# The pattern is checked against its samples first: a pattern that matches
# nothing would pass every page.
t_case "the root link pattern finds both forms it is for"
grep -qiE -- "$root_re" <<< 'a [factory floor](/) link' \
  || t_bad "the pattern finds no Markdown link to the root"
grep -qiE -- "$root_re" <<< 'a [Factory floor]({{ $base }}/) link in a template' \
  || t_bad "the pattern finds no template link to the root"
grep -qiE -- "$root_re" <<< '[factory floor](/floor) and [x]({{ $base }}/health) and [x](/dashboard)' \
  && t_bad "the pattern matches a link that is not the root"

t_case "no manual page links the root, which is the floor in one design only"
roots=$(grep -rinE -- "$root_re" "$T_ROOT/host/docs" || true)
[ -z "$roots" ] || t_bad "links the root: $roots"

t_case "llms.txt links the factory floor at its /floor"
t_contains "$(cat "$T_ROOT/host/docs/llms.txt")" '[Factory floor]({{ $base }}/floor)'

t_case "the pages that name the floor link it at /floor"
# tools-health.md is the page that took the two sections of tools.md that link
# the floor: `tehasbot status and the console` and the live view.
for page in operating.md overview.md pi-software-factory.md tools-health.md; do
  grep -qiE -- '\[factory floor\]\(/floor\)' "$T_ROOT/host/docs/$page" \
    || t_bad "$page has no link to the floor at /floor"
done

t_done
