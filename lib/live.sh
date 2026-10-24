# shellcheck shell=bash
# The live view under $TEHAS_HOME/live, as the factory floor, the plain
# dashboard, tehasbot health, tehas dev and the MCP server read it:
#
#   cycles/<issue>.json  each running cycle and the one that finished last
#   running.json         {"cycles": [<each running cycle, oldest first>],
#                         "last": <the cycle that finished most recently, or null>}
#   endings.json         the last ten ends, oldest first, each
#                        {issue, status, step, startedAt, updatedAt}
#   dispatch.json        the poll's report, when the poll gives it
#   health.json          tehasbot health's report, when health gives it, and
#                        health.down while that report says fail
#
# They have one writer, the state store's publish verb
# (console/store/src/live.rs). It renders the cycle views from the heartbeats
# a running cycle records every ten seconds (cycle-heartbeat) and from the
# ends cycle-end records, holds state/publish.lock while it writes, and puts
# each file in place by renaming a hidden temp file. A running cycle not heard
# from for two minutes whose issue lock (state/cycles/<issue>.lock) is free is
# shown as lost.
#
#   tehas_live_publish [--dispatch <file>] [--health <file>]
#       Publishes the views, and the reports given. Returns the store's
#       status (lib/store.sh): 0 published; otherwise tehas_store_word and
#       tehas_store_line say why. Tries again while the store is busy.
#
# Needs lib/store.sh and TEHAS_HOME.

tehas_live_publish() {
  tehas_store_try 3 publish "$@"
}
