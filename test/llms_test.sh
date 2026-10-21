#!/usr/bin/env bash
# Tests for the pages that tell an agent how to find the MCP server:
# shellcheck disable=SC2016  # the template text is a literal
# host/docs/llms.txt and host/docs/agents.md.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

tools="factory_status factory_issues factory_endings factory_console factory_stats factory_sessions factory_session factory_log factory_failures"

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

  t_case "$page: names all nine tools"
  for tool in $tools; do
    t_contains "$text" "$tool"
  done

  t_case "$page: holds no address of this host"
  t_lacks "$text" "https://tehas.app/mcp"
done

t_case "mcp-server.md: has a client config block"
t_contains "$(cat "$T_ROOT/host/docs/mcp-server.md")" '"headers"'

t_done
