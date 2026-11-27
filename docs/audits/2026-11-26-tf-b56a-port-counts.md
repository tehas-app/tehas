# TF-B56a: the adapter's unsupported port reads, counted by reach

Measured 2026-11-26 at the v2 tip bcae8d96a, on
`console/github/src/port/**.rs`: 62 port methods answer `unsupported()`
somewhere in the GitHub adapter, plus two inherent adapter helpers
(`GitHubSite::handle` at `console/github/src/port/site.rs:133`,
`GitHub::open_work_pages` at `console/github/src/port/board/work.rs:310`).
Each row is one trait method; a method unsupported on both the `GitHub`
and the `GitHubAnonymous` impl is marked `both`, and a method whose
`unsupported()` sits behind a guard is marked in the note. The verdict is
the receiver-typed reach: a call site counts only when the receiver's
static type is the port trait, its handle or the concrete adapter, so a
same-named verb on another type does not count.

Reach words: `production` means a shipped program can call it;
`test-only` means only tests, laws and fakes drive it; `unreached` means
no call site outside the trait, the adapter and the fake. A method
reached from production with the adapter answering `unsupported()` on the
path is answered first, port by port; the rest go as a written floor with
their cause, or stay until their program grows the call.

## CodeHost

| Method | Impls | Reached | Sites | Why |
| --- | --- | --- | --- | --- |
| `activity` | GitHub | test-only | console/github/tests/activity.rs:230, console/github/tests/activity.rs:236, console/github/tests/activity.rs:303 (+11) | Implemented on GitHub (console/github/src/port/code_host.rs:173, forwards to code_host/activity.rs); Anonymous answers unsupported (console/github/src/port/anonymous.rs:482-483) |
| `archive` | both | test-only | console/github/tests/anonymous.rs:261, console/github/tests/anonymous_download.rs:502, console/github/tests/releases.rs:396 | unsupported() on both impls: console/github/src/port/code_host.rs:234-235 (GitHub) and console/github/src/port/anonymous.rs:651-652 (Anonymous), so impl=both |
| `branch` | GitHub | production | console/dispatch/src/read/checks.rs:41, console/github/tests/activity.rs:354, console/github/tests/activity.rs:649 (+3) | Production: dispatch/src/read/checks.rs:41, receiver host: Option<&mut dyn CodeHost> (fn read param, line 28), reached from read/build.rs:618 (snapshot -> super::checks::read) via Sources.code_host (read/build.rs:30 pub code_host: Option<Box<dyn tehas_ports::CodeHost>>), wired in dispatch/src/main.r |
| `change` | GitHub | production | console/core/src/journal/settle.rs:134, console/github/tests/changes_bodies.rs:332, console/github/tests/change_read.rs:356 (+3) | GitHub impl implemented (code_host.rs:164, forwards to code_host/change.rs); Anonymous unsupported (anonymous.rs:474-475) |
| `changed` | both | unreached | none | No call site anywhere outside the trait, the two adapter impls and the fake |
| `changed_files` | both | test-only | console/github/tests/code_host.rs:572, console/ports/tests/laws.rs:4035 | unsupported() on both impls: console/github/src/port/code_host.rs:168-169 (GitHub) and console/github/src/port/anonymous.rs:478-479 (Anonymous) |
| `changes` | GitHub | production | console/core/src/journal/settle.rs:129, console/cycle/src/apply.rs:236, console/cycle/src/finish/run.rs:402 (+9) | GitHub impl implemented (code_host.rs:80); Anonymous unsupported (anonymous.rs:470-471) |
| `checks` | GitHub | production | console/dispatch/src/read/checks.rs:46, console/github/tests/code_host.rs:269, console/github/tests/code_host.rs:656 (+7) | GitHub impl implemented (code_host.rs:179, forwards to code_host/checks.rs); Anonymous unsupported (anonymous.rs:486-487) |
| `compare` | both | unreached | none | No call site anywhere outside the trait, both adapter impls and the fake |
| `delta` | both | test-only | console/ports/tests/laws.rs:1620, console/ports/src/laws.rs:2154 | unsupported() on both impls: console/github/src/port/code_host.rs:242-243 (GitHub) and console/github/src/port/anonymous.rs:659-660 (Anonymous) |
| `facts` | GitHub | test-only | console/github/tests/code_host.rs:232, console/github/tests/code_host.rs:957, console/github/tests/code_host.rs:963 (+3) | Implemented on GitHub (code_host.rs:42); Anonymous unsupported (anonymous.rs:454-455) |
| `file` | both | test-only | console/ports/tests/laws.rs:4108 | unsupported() on both impls: console/github/src/port/code_host.rs:59-60 (GitHub) and console/github/src/port/anonymous.rs:466-467 (Anonymous) |
| `recent_releases` | GitHub | test-only | console/github/tests/releases.rs:235 | Implemented on GitHub (code_host.rs:205, forwards to code_host/releases.rs); Anonymous unsupported (anonymous.rs:557-558) |
| `recent_tags` | GitHub | test-only | console/github/tests/releases.rs:155, console/github/tests/releases.rs:158, console/ports/tests/laws.rs:1061 | Implemented on GitHub (code_host.rs:190); Anonymous unsupported (anonymous.rs:516-517) |
| `release` | GitHub | test-only | console/github/tests/releases.rs:280, console/github/tests/releases.rs:286, console/github/tests/releases.rs:288 (+3) | Implemented on GitHub (code_host.rs:215, code_host/releases.rs); Anonymous unsupported (anonymous.rs:581-582, asserted in anonymous.rs:257) |
| `remote` | GitHub | production | console/cycle/src/finish/run.rs:437, console/cycle/src/finish/tests.rs:1117, console/cycle/src/finish/tests.rs:1160 (+8) | Implemented on GitHub (code_host.rs:247, code_host/remote.rs); Anonymous unsupported (anonymous.rs:663-664, asserted anonymous.rs:263) |
## CodeHostWrites

| Method | Impls | Reached | Sites | Why |
| --- | --- | --- | --- | --- |
| `close_change` | GitHub | test-only | console/ports/src/write.rs:610, console/ports/src/fake/faulty.rs:1560, console/ports/src/laws.rs:1140 (+4) | GitHub impl at console/github/src/port/code_host_writes.rs:122 returns unsupported(); Anonymous never implements CodeHostWrites (site.rs:246 hands Actor::Anonymous writes Forbidden) |
| `comment_change` | GitHub | production | console/ports/src/write.rs:608, console/ports/src/fake/faulty.rs:1556, console/ports/src/laws.rs:2051 (+18) | GitHub impl at console/github/src/port/code_host_writes.rs:96 (implemented; markers non-empty gives unsupported) |
| `edit_change` | GitHub | test-only | console/ports/src/write.rs:603, console/ports/src/fake/faulty.rs:1536, console/ports/tests/laws.rs:2568 | GitHub impl at console/github/src/port/code_host_writes.rs:56 returns unsupported(); no Anonymous impl |
| `merge` | GitHub | test-only | console/ports/src/write.rs:611, console/ports/src/fake/faulty.rs:1563, console/ports/src/laws.rs:812 (+12) | GitHub impl at console/github/src/port/code_host_writes.rs:126 returns unsupported(); no Anonymous impl |
| `new_release` | GitHub | test-only | console/ports/src/write.rs:620, console/ports/src/fake/faulty.rs:1588, console/ports/src/laws.rs:793 (+1) | GitHub impl at console/github/src/port/code_host_writes.rs:172 returns unsupported(); no Anonymous impl |
| `publish_release` | GitHub | test-only | console/ports/src/write.rs:626, console/ports/src/fake/faulty.rs:1603 | GitHub impl at console/github/src/port/code_host_writes.rs:180 returns unsupported(); no Anonymous impl |
| `tag` | GitHub | test-only | console/ports/src/write.rs:618, console/ports/src/fake/faulty.rs:1580, console/ports/src/laws.rs:2758 (+2) | GitHub impl at console/github/src/port/code_host_writes.rs:136 returns unsupported(); no Anonymous impl |
| `upload` | GitHub | test-only | console/ports/src/write.rs:621, console/ports/src/fake/faulty.rs:1598, console/ports/src/laws.rs:2742 | GitHub impl at console/github/src/port/code_host_writes.rs:176 returns unsupported(); no Anonymous impl |
## Tickets

| Method | Impls | Reached | Sites | Why |
| --- | --- | --- | --- | --- |
| `changed` | GitHub | unreached | console/ports/src/fake/faulty.rs:1206 | Tickets::changed is unsupported() on the GitHub adapter (console/github/src/port/tickets.rs:142-143) |
| `count` | GitHub | test-only | console/ports/src/laws.rs:182, console/ports/src/laws.rs:196, console/github/tests/port.rs:412 (+1) | Adapter: unsupported() at console/github/src/port/tickets.rs:55-56 (GitHub only; no Tickets impl for Anonymous) |
| `delta` | GitHub | test-only | console/ports/src/laws.rs:2049, console/ports/src/laws.rs:2058, console/ports/src/laws.rs:2130 (+4) | Adapter: unsupported() at console/github/src/port/tickets.rs:146-147 (GitHub impl only) |
| `filed_by` | GitHub | unreached | console/ports/src/fake/faulty.rs:1179 | unsupported() at console/github/src/port/tickets.rs:110-111 (GitHub impl; none for Anonymous) |
| `find_marked` | GitHub | unreached | console/ports/src/fake/faulty.rs:1190 | unsupported() at console/github/src/port/tickets.rs:114-121 (GitHub impl only) |
| `members` | GitHub | test-only | console/ports/src/board.rs:542, console/ports/src/fake/faulty.rs:1203 | unsupported() at console/github/src/port/tickets.rs:138-139 (GitHub impl only) |
| `note_edits` | GitHub | unreached | console/ports/src/fake/faulty.rs:1153 | unsupported() at console/github/src/port/tickets.rs:82-83 (GitHub impl only) |
| `notes` | GitHub | production | console/core/src/journal/settle.rs:115, console/cycle/src/apply.rs:668, console/ports/src/board.rs:502 (+5) | Production reach: (1) core/src/journal/settle.rs:115 h.tickets.notes where h: &mut tehas_core::journal::Handles (tickets: Box<dyn Tickets>, journal.rs:80); settle::read is called from cycle/src/apply.rs:358 and dispatch/src/apply/settle.rs:286 (found_write), both production journal-settle paths of t |
| `recent` | GitHub | test-only | console/ports/src/laws.rs:143, console/ports/src/laws.rs:146, console/ports/tests/laws.rs:485 (+3) | unsupported() at console/github/src/port/tickets.rs:47-48 (GitHub impl only) |
| `recently_closed` | GitHub | test-only | console/ports/src/laws.rs:161, console/ports/src/fake/faulty.rs:1123 | unsupported() at console/github/src/port/tickets.rs:51-52 (GitHub impl only) |
| `search` | GitHub | test-only | console/ports/tests/laws.rs:1897, console/ports/tests/laws.rs:1901, console/ports/tests/laws.rs:1906 (+2) | unsupported() at console/github/src/port/tickets.rs:100-107 (GitHub impl only) |
| `targets` | GitHub | test-only | console/ports/src/laws.rs:222, console/ports/src/fake/faulty.rs:1200 | unsupported() at console/github/src/port/tickets.rs:134-135 (GitHub impl only) |
## TicketWrites

| Method | Impls | Reached | Sites | Why |
| --- | --- | --- | --- | --- |
| `edit_target` | GitHub | unreached | console/ports/src/write.rs:570, console/ports/src/fake/faulty.rs:1314 | Only shipped-code site is the Writer::send dispatch arm Write::EditTarget => t.edit_target(id, edit) with t = &mut self.tickets, field pub tickets: Box<dyn TicketWrites + Send> (write.rs:513) |
| `file` | GitHub | unreached | console/ports/src/write.rs:552, console/ports/src/laws.rs:2812, console/ports/src/fake/faulty.rs:1268 (+6) | Shipped-code site is the Writer::send arm t.file(new) (write.rs:552); Write::FileTicket is never constructed in production (fake/tickets.rs:854 and tests only; store/src/journal.rs:173 is a match pattern in issue_of, not a construction; core/tests/ports_json.rs is a test), so the arm never runs at r |
| `new_target` | GitHub | unreached | console/ports/src/write.rs:569, console/ports/src/fake/faulty.rs:1311 | Only shipped site is Writer::send's Write::NewTarget arm t.new_target(title, *due); Write::NewTarget is constructed nowhere in production (fake/tickets.rs:1119 and ports tests only), so the arm never executes at runtime |
| `remove_part` | GitHub | unreached | console/ports/src/write.rs:568, console/ports/src/fake/faulty.rs:1307 | Only shipped site is Writer::send's Write::RemovePart arm t.remove_part(parent, child) |
| `reopen` | GitHub | unreached | console/ports/src/write.rs:546, console/ports/src/fake/faulty.rs:1283 | Only shipped site is Writer::send's Write::Reopen arm t.reopen(ticket) |
| `set_kind` | GitHub | unreached | console/ports/src/write.rs:562, console/ports/src/fake/faulty.rs:1286 | Only shipped site is Writer::send's Write::SetKind arm t.set_kind(ticket, kind.as_ref()); Write::SetKind is constructed nowhere in production (fake/tickets.rs:952 and tests only; store/src/journal.rs:153 is a match pattern in issue_of), so the arm never executes at runtime |
| `set_priority` | GitHub | unreached | console/ports/src/write.rs:563, console/ports/src/fake/faulty.rs:1289 | Only shipped site is Writer::send's Write::SetPriority arm t.set_priority(ticket, *priority); Write::SetPriority is constructed nowhere in production (fake/tickets.rs:970 and tests only; store/src/journal.rs:154 is a match pattern), so the arm never runs at runtime |
| `set_target` | GitHub | unreached | console/ports/src/write.rs:571, console/ports/src/fake/faulty.rs:1318 | Only shipped site is Writer::send's Write::SetTarget arm t.set_target(ticket, target.as_ref()); Write::SetTarget is constructed nowhere in production (fake/tickets.rs:1167 and tests only; store/src/journal.rs:158 is a match pattern), so the arm never runs at runtime |
| `stop_waiting` | GitHub | unreached | console/ports/src/write.rs:565, console/ports/src/fake/faulty.rs:1296 | Only shipped site is Writer::send's Write::StopWaiting arm t.stop_waiting(ticket, on); Write::StopWaiting is constructed nowhere in production (fake/tickets.rs:1022 and tests only; store/src/journal.rs:156 is a match pattern), so the arm never runs at runtime |
| `wait_on` | GitHub | unreached | console/ports/src/write.rs:564, console/ports/src/fake/faulty.rs:1292, console/ports/tests/laws.rs:3658 (+2) | Only shipped site is Writer::send's Write::WaitOn arm t.wait_on(ticket, on); Write::WaitOn is constructed nowhere in production (fake/tickets.rs:1004 and tests only; store/src/journal.rs:155 is a match pattern in issue_of), so the arm never runs at runtime |
## Blueprints+BlueprintWrites

| Method | Impls | Reached | Sites | Why |
| --- | --- | --- | --- | --- |
| `changed` | GitHub | test-only | console/github/tests/blueprints.rs:252, console/cycle/src/relay.rs:1741, console/cycle/tests/common/mod.rs:1315 (+1) | No production call |
| `close` | GitHub | unreached | console/ports/src/write.rs:595, console/cycle/src/relay.rs:1780, console/github/tests/blueprint_writes.rs:775 (+5) | Only port call outside tests is the dispatcher arm Write::CloseBlueprint { no, done } => done(b.close(*no, *d)) at console/ports/src/write.rs:595 (b = &mut self.blueprints, field pub blueprints: Box<dyn BlueprintWrites + Send> at write.rs:515) |
| `comment` | GitHub | production | console/cycle/src/relay.rs:595, console/ports/src/write.rs:585, console/cycle/src/relay.rs:1768 (+18) | Production: console/cycle/src/relay.rs:595 Ask::Comment { no, body } => writes.comment(*no, body, &[]) inside make(writes: &mut dyn BlueprintWrites, ...) (relay.rs:565), reached from relay.rs:548 make(&mut *writes, ...) where writes came from (via.writes)(Box::new(gh)); the handle is the GitHub adap |
| `comments_by_id` | GitHub | test-only | console/github/tests/blueprints.rs:249, console/cycle/src/relay.rs:1714, console/cycle/tests/common/mod.rs:1269 (+8) | No production call |
| `delta` | GitHub | test-only | console/github/tests/blueprints.rs:253, console/cycle/src/relay.rs:1744, console/cycle/tests/common/mod.rs:1322 (+4) | No production call |
| `edit` | GitHub | production | console/cycle/src/relay.rs:607, console/ports/src/write.rs:582, console/cycle/src/relay.rs:1760 (+35) | Production: console/cycle/src/relay.rs:607 let edited = writes.edit(*no, title, body, &MarkerEdit::default(), &read.revision) in make(writes: &mut dyn BlueprintWrites) (relay.rs:565), reached via relay.rs:548 with the GitHub adapter handle from console/cycle/src/main.rs:519, served by tehas-cycle re |
| `fetch` | GitHub | test-only | console/cycle/src/relay.rs:1738, console/cycle/tests/common/mod.rs:1309, console/ports/tests/laws.rs:4105 (+1) | No production call |
| `open` | GitHub | production | console/cycle/src/relay.rs:574, console/src/plan/file.rs:82, console/ports/src/write.rs:574 (+15) | Two production call sites |
## Board

| Method | Impls | Reached | Sites | Why |
| --- | --- | --- | --- | --- |
| `closed_work` | GitHub | test-only | console/github/tests/board.rs:310, console/ports/tests/join.rs:890, console/ports/src/fake/faulty.rs:1667 | GitHub impl (console/github/src/port/board.rs:118) returns unsupported() unconditionally; it sits on the GitHub impl only, the anonymous runner never implements Board |
| `handle` | GitHub | production | console/github/src/port/site.rs:187, console/github/src/port/site.rs:196, console/github/src/port/site.rs:205 (+12) | handle is console/github/src/port/site.rs:133, a fn on GitHubSite, shared by every port rather than Board-specific; the Board one is Site::board at site.rs:265-266 calling self.handle(by, caller, PortName::Board, false) |
| `open_changes` | GitHub | production | console/ops/src/plugin/ask.rs:159, console/github/tests/board.rs:146, console/github/tests/board.rs:147 (+35) | The one production site is console/ops/src/plugin/ask.rs:159, inside pub fn hook (ask.rs:111): `board(&repo, &bot, id).and_then(|mut b| b.open_changes(&one_change(no), one))`, receiver `b` is the Box<dyn Board + Send> the BoardOf closure returns |
| `open_work_pages` | GitHub | test-only | console/github/src/port/board.rs:111 | Defined at console/github/src/port/board/work.rs:310 (`pub(super) fn open_work_pages(&mut self, ask: &BoardAsk, bound: Bound) -> Got<OpenWork>` on GitHub), returns super::super::unsupported() at work.rs:312 for asks it does not answer |
| `target_work` | GitHub | test-only | console/github/tests/board.rs:314, console/ports/tests/join.rs:894, console/ports/src/fake/faulty.rs:1677 | GitHub impl (console/github/src/port/board.rs:127) returns unsupported() unconditionally, GitHub only (anonymous runner has no Board impl) |
| `work_item` | GitHub | test-only | console/ports/src/board.rs:286, console/ports/src/laws.rs:1474, console/github/tests/board.rs:308 (+12) | GitHub impl (console/github/src/port/board.rs:114) returns unsupported() unconditionally; GitHub only |
## Identity+Site

| Method | Impls | Reached | Sites | Why |
| --- | --- | --- | --- | --- |
| `account` | GitHub | test-only | console/github/tests/identity.rs:749, console/ports/src/fake/faulty.rs:1616, console/ports/tests/laws.rs:2195 (+2) | Identity::account(&mut self, who: &AccountId) -> Got<Account>, defined console/ports/src/identity.rs:51 |
| `code_host` | GitHub | production | console/cycle/src/main.rs:539, console/cycle/src/main.rs:578, console/dispatch/src/main.rs:206 (+84) | Site::code_host(&self, by: &Actor, caller: Caller) -> Got<Box<dyn CodeHost + Send>>, defined console/ports/src/site.rs:32 |
| `token` | GitHub | test-only | console/github/tests/identity.rs:751, console/ports/src/laws.rs:3012, console/ports/src/laws.rs:3029 (+16) | Identity::token(&mut self, scope: Scope) -> Got<Token>, defined console/ports/src/identity.rs:64 |
| `token_over` | GitHub | test-only | console/github/tests/identity.rs:755, console/ports/src/laws.rs:2967, console/ports/src/laws.rs:2974 (+7) | Identity::token_over(&mut self, scope, projects) -> Got<Token>, defined console/ports/src/identity.rs:69 |

One PR per port follows, answering the `production` rows in the adapter
and writing a floor with its cause for what cannot be answered yet.
Nothing is removed from a port after the Nov 30 freeze.

