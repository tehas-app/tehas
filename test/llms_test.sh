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

# The floor's page moved with the split of the commands page: what the floor
# reads is the health page, so the address it is given here has to be a page of
# the manual, and the health page has to link the floor at it.
t_case "the floor's address in llms.txt is a page of the manual"
floor=$(grep -oE '\[Factory floor\]\(\{\{ \$base \}\}/[a-z]+\)' "$T_ROOT/host/docs/llms.txt" |
  sed -e 's/.*}\}//' -e 's#)##g')
[ -n "$floor" ] || t_bad "llms.txt gives the floor no address"
[ "$floor" = /floor ] || t_bad "llms.txt gives the floor the address $floor, which no page answers"
t_case "the health page links the floor at the address llms.txt gives it"
t_contains "$(cat "$T_ROOT/host/docs/tools-health.md")" "($floor)"

t_case "the pages that name the floor link it at /floor"
for page in operating.md overview.md pi-software-factory.md tools-health.md; do
  grep -qiE -- '\[factory floor\]\(/floor\)' "$T_ROOT/host/docs/$page" \
    || t_bad "$page has no link to the floor at /floor"
done

# The commands page is the one llms.txt names for the two commands, and the
# pages by tool are named beside it: a page that left the list left the manual.
t_case "llms.txt names the commands page and the pages by tool"
llms=$(cat "$T_ROOT/host/docs/llms.txt")
t_contains "$llms" '[Commands]({{ $base }}/tools.md)'
t_contains "$llms" '[Commands]({{ $base }}/tools.md): `tehas`, `tehasbot`'
for page in tools-dispatch tools-work tools-plan tools-dev tools-setup \
  tools-control tools-status tools-tehasbot tools-health tools-protolab \
  tools-github tools-clean tools-update tools-releases; do
  t_contains "$llms" "](/$page)"
done

# The line an agent follows for a hand session is the one that names `tehas dev`,
# and it names the page the command is on now.
t_case "llms.txt names the dev page for what tehas dev accepts"
line=$(grep -F -- 'accepts from each kind of account' "$T_ROOT/host/docs/llms.txt")
t_contains "$line" 'tools-dev.md#tehas-dev'

t_done
