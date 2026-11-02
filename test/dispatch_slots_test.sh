#!/usr/bin/env bash
# Tests for libexec/dispatch: slots, the CPU ceiling, urgent order and the cap
# on unreviewed work.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

t_case "two slots: two issues are started in one poll, in order"
reset
slots 2
ready 7 8
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 "
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready"
t_contains "$(t_calls gh)" "issue edit 8 --repo acme/widget --add-label tehas:running --remove-label tehas:ready"

t_case "the index is refreshed once, before the first start"
t_eq "$(t_calls fake-index)" "--before-cycle"
t_eq "$(find "$T/calls/index.late" 2>/dev/null | wc -l)" 0

t_case "a ceiling of one: the second issue waits"
reset
ready 7 8
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"

# nproc is stubbed to 8 above, and reset()'s slice has room for ten.
t_case "the ceiling defaults to the stubbed host's CPUs minus one: seven"
reset
t_instance "${T_MINIMAL[@]}"
ready 7 8 9 10 11 12 13 14
for n in 20 21 22 23 24 25 26; do hold "$n"; done
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "holding: 7 cycles are running (limit 7)"
release

t_case "below the ceiling, no more cycles start than the containers' ceilings fit in the slice"
reset
t_instance "${T_MINIMAL[@]}"
ready 7 8 9 10 11 12 13 14
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 9 10 "
reset
t_instance "${T_MINIMAL[@]}"
for d in a b c d; do slice_child "$(slice_id "$d")" "$gib" 0 0; done
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "holding: tehas.slice holds as many containers as their ceilings fit (4 of 5120m)"
t_eq "$(jq -c '[.why, .room]' "$dispatch_json")" '["full",0]'

t_case "an urgent issue is started first"
reset
slots 2
ready 7 8! 9
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "8 7 "

# The order is what an urgent label buys, so who applied it is checked as the
# labelling of tehas:ready is.
t_case "urgent from an allowed account moves an issue ahead of a lower number"
reset
slots 2
ready 3 8!
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start | tr '\n' ' ')" "8 3 "

t_case "urgent applied by the bot is ignored, and the poll says so"
reset
slots 2
ready 3 8!
urgent_by 8 tehas-bot
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start | tr '\n' ' ')" "3 8 "
t_contains "$t_out" "ignore #8: tehas:urgent was applied by 'tehas-bot', not an allowed account"

t_case "urgent applied by an account outside the list is ignored"
reset
slots 2
ready 3 8!
urgent_by 8 mallory
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start | tr '\n' ' ')" "3 8 "
t_contains "$t_out" "ignore #8: tehas:urgent was applied by 'mallory', not an allowed account"

t_case "an urgent label with no account in its event is ignored"
reset
slots 2
ready 3 8!
urgent_by 8 ""
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start | tr '\n' ' ')" "3 8 "
t_contains "$t_out" "ignore #8: tehas:urgent was applied by 'unknown', not an allowed account"

t_case "an urgent label with no labelling event is ignored"
reset
slots 2
ready 3 8!
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}}]' >"$fx/events-8.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start | tr '\n' ' ')" "3 8 "
t_contains "$t_out" "ignore #8: tehas:urgent was applied by 'unknown', not an allowed account"

t_case "the most recent urgent labelling decides"
reset
slots 2
ready 3 8!
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}},
       {"event":"labeled","label":{"name":"tehas:urgent"},"actor":{"login":"mallory"}},
       {"event":"unlabeled","label":{"name":"tehas:urgent"},"actor":{"login":"mallory"}},
       {"event":"labeled","label":{"name":"tehas:urgent"},"actor":{"login":"alice"}}]' >"$fx/events-8.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start | tr '\n' ' ')" "8 3 "

t_case "an ignored urgent label leaves the issue ready on its own label"
reset
slots 2
ready 3 8!
urgent_by 8 mallory
t_run "$dispatch"
t_contains "$(t_calls gh)" "issue edit 8 --repo acme/widget --add-label tehas:running --remove-label tehas:ready"

t_case "a dry run reports the order a real poll would take"
reset
slots 2
ready 3 8!
urgent_by 8 mallory
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "ignore #8: tehas:urgent was applied by 'mallory', not an allowed account"
t_eq "$(printf '%s\n' "$t_out" | sed -n 's/.*would run a cycle for #\([0-9]*\).*/\1/p' | tr '\n' ' ')" "3 8 "

t_case "the labelling of an urgent issue comes with the queue's page"
reset
slots 2
ready 3 8!
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(grep -c -e 'api repos/acme/widget/issues/8/events' <<<"$(t_calls gh)")" 0

t_case "an urgent issue this poll cannot start is not asked about"
reset
slots 2
ready 8!
hold 8
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(grep -c -e 'api repos/acme/widget/issues/8/events' <<<"$(t_calls gh)")" "0"
t_contains "$t_out" "wait #8: its cycle is still running"
release

t_case "urgent order survives an issue number that arrives as text"
reset
slots 2
ready 3 8
echo '[{"number":3,"labels":[{"name":"tehas:ready"}]},{"number":"8","labels":[{"name":"tehas:ready"},{"name":"tehas:urgent"}]}]' >"$fx/ready.json"
urgent_by 8 alice
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_eq "$(printf '%s\n' "$t_out" | sed -n 's/.*would run a cycle for #\([0-9]*\).*/\1/p' | tr '\n' ' ')" "8 3 "

t_case "label events that answer with an object end the poll too"
reset
slots 2
ready 3 8!
echo '{"message":"Bad credentials"}' >"$fx/events-8.json"
t_run "$dispatch"
t_eq "$t_rc" 1
# The page fails as a whole, with its own words.
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "a running cycle takes a slot"
reset
slots 2
ready 7 8
hold 9
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
release

t_case "with every slot taken nothing is started, and no ready issue is looked at"
reset
slots 2
ready 7 8
hold 9
hold 10
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
# The demand reads the list and who labelled each issue; nothing else is asked.
# One page, and nothing per issue.
t_eq "$(grep -c -x -e 'api repos/acme/widget/issues/[78]' -e 'issue view [78] .*' <<<"$(t_calls gh)")" 0
t_lacks "$(t_calls gh)" "awaiting-merge"
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: 2 cycles are running (limit 2)"
release

t_case "the index is not refreshed under a running cycle"
reset
slots 2
ready 7
hold 9
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(t_calls fake-index)" ""
t_contains "$t_out" "the code index stays as it is: a cycle is running"
release

t_case "running cycles count towards the cap on unreviewed work"
reset
slots 3 TEHAS_MAX_AWAITING=3
ready 7 8
echo '[{"number":5}]' >"$fx/awaiting.json"
hold 9
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
release

t_case "at the cap nothing is started, whatever slots are free"
reset
slots 3 TEHAS_MAX_AWAITING=2
ready 7 8
echo '[{"number":5}]' >"$fx/awaiting.json"
hold 9
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: 1 issues await merge and 1 cycles are running (limit 2)"
release

t_case "an issue that is not approved does not use a slot"
reset
slots 2
ready 7 8 9
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "8 9 "
t_lacks "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:running"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:ready"

t_case "a cycle that does not start does not stop the next one, and the poll ends with 1"
reset
slots 2
ready 7 8
# fake-start fails for issue 7 only.
t_stub fake-start '[ "$1" != 7 ] || exit 1
flock -w 5 "$TEHAS_HOME/state/cycles/$1.lock" "$T/store-bin" cycle-start "$1" >/dev/null'
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_lacks "$(t_calls gh)" "issue edit 8 --repo acme/widget --remove-label"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 "
default_start

t_case "a dry run names every issue it would start and how much room there is"
reset
slots 3
ready 7 8
hold 9
t_run "$dispatch --dry-run"
t_contains "$t_out" "1 cycles running, room for 2 more"
t_contains "$t_out" "would run a cycle for #7 (labelled by alice)"
t_contains "$t_out" "would run a cycle for #8 (labelled by alice)"
t_eq "$(t_calls fake-start)" ""
release

t_done
