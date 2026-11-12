#!/usr/bin/env bash
# Tests for tehasbot dispatch: what the
# slots and the room under the review cap reach in live/dispatch.json.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1
t_case "the slots and the room under the review cap reach live/dispatch.json"
reset
slots 3
echo '[{"number":20},{"number":21}]' >"$fx/awaiting.json"
echo '[]' >"$fx/ready.json"
hold 5
t_run "$dispatch"
release
t_eq "$(jq -c '{free, awaitRoom}' "$dispatch_json")" '{"free":2,"awaitRoom":2}'
reset
hold 5
t_run "$dispatch"
release
t_eq "$(jq -c '{free, awaitRoom}' "$dispatch_json")" '{"free":0,"awaitRoom":null}'

t_done
