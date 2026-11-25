# T10 rename checklist

Date: 2026-11-24. Status: a checklist for T10 of
`docs/plans/2026-11-21-coding-standard-v2.md`, read at 98a369aca on
`refactor/coding-standard-v2`. Every file:line below is from that commit.

T10 renamed the repository to `tehas-app/tehas-factory` (#2980). GitHub
redirects the old name, `tehas-app/tehas`, until that name is reused for a
get-started repository. This list says what still depends on the old name,
what fails, and what closes it.

Each item has one status:

- **done**: nothing is left to do.
- **before the alpha**: the alpha waits for it.
- **before the reuse**: `tehas-app/tehas` may not be reused until it is
  done.

The checklist gates the alpha. The code moves it names (item 1's move to
the repository id) come after the alpha.

## 1. Checks that compare a link with the configured name

Status: **before the alpha** (the settings step). The code move is after
the alpha.

What fails: these read a link or a name GitHub answers and compare it with
the repository the instance names (`TEHAS_REPO`). GitHub answers with the
current name, `tehas-app/tehas-factory`. An instance that still names
`tehas-app/tehas` gets `Unread(Inconsistent)` or a refusal: a write that was
sent is read back as not this project's, and a read fails closed. None of
them misreads another project as this one.

In the adapter, `console/github/src/port/` (#3195's "Open for the lead"
names these):

- `writes.rs:401`, `issue_of` (defined at :380), `ends_with_folded(url,
  "/{repo}/issues/{n}")`. Used by `close` (:118, call :124), the edit
  (:219, :313), `add_part` (:303, call :327) and :359.
- `blueprint_writes.rs:290`, `read_issue` (defined at :265): the issue's
  link. Used at `blueprint_writes.rs:38`, :87, :204 and `blueprints.rs:292`.
- `blueprint_writes.rs:315`, `read_comment` (defined at :304): the
  comment's link. Used at `blueprint_writes.rs:131` and `writes.rs:156`.
- `blueprints.rs:690`, `read_comments`: each comment's link.
- `blueprints/edits.rs:488` and :491, `issue_of` (defined at :477): a
  comment's link, pull request or issue.
- `code_host_writes.rs:306`: the pull request's link after a write.
- `code_host_writes.rs:328`, `read_comment` (defined at :316): the
  comment's link on a pull request.
- `code_host_writes/open.rs:235`: the opened pull request's `html_url`, and
  :240, its head repository's `full_name`.
- `code_host/change.rs:495`, `read_pull`: the pull request's link.
- `code_host/change.rs:584`: each comment's link on a pull request.
- `code_host/change.rs:886`, `on_pull`: a review's pull request link.

`code_host.rs:292` (`read_facts`) compares too, but takes GitHub's name
where it differs, so it does not fail.

Outside the adapter, the same comparison by name, with the same effect
(the factory's own pull request reads as another repository's):

- `console/core/src/accounts.rs:73`, `own_pull`: `headRepository.nameWithOwner`.
- `console/core/src/plugin/hook.rs:154`, `open_bot_pull`: `head.repo.full_name`.
- `console/dispatch/src/decide/retry.rs:53`: the pull request's head repository.
- `console/src/preview/sweep.rs:95` and `console/src/preview/verdict.rs:53`:
  `full_name`.
- `console/cycle/src/relay.rs:223`: refuses a `--repo` that is not the
  configured name. Not a misread: a cycle that names the new name is
  refused until the setting names it too.

What closes it:

- Before the alpha: every instance's `TEHAS_REPO` names
  `tehas-app/tehas-factory`, so the configured name is current and every
  check above passes. Today `instances/tehas-tehas/instance.env:4` still
  says `TEHAS_REPO=tehas-app/tehas` (see item 5).
- After the alpha: move the checks onto the repository id, as #3195 did for
  the reads (`GitHub::is_here_id`, `console/github/src/forge.rs:214`). The
  move to `databaseId` is ruled for after the alpha.

## 2. GraphQL by the old name resolves today

Status: **done** (checked 2026-11-24).

Checked live and read-only:
`repository(owner:"tehas-app",name:"tehas")` answers `databaseId`
1411216280, `nameWithOwner` `tehas-app/tehas-factory`. So, while the
redirect holds:

- #3195's learned id is the right one: a handle set to the old name learns
  1411216280 and reads this project as here
  (`console/github/tests/port.rs:191`,
  `a_renamed_project_is_still_here_by_its_repository_id`).
- tag-release (#3200) reaches the repository through the redirect from a
  clone whose origin names the old name.

This stops holding at the reuse. Item 3 covers that.

## 3. The reuse of `tehas-app/tehas`

Status: **before the reuse**. The `tehas update` refusal is still to
build. No code is in this change.

T10 says `tehas-app/tehas` will be reused for a get-started repository.
That ends the redirect. From then on, an instance or clone that still
names `tehas-app/tehas` reaches a different, real repository.

What fails:

- Reads by id (#3195) see the get-started repository's id, not 1411216280,
  and read it as elsewhere: they fail closed. A handle that learns the
  get-started repository's id first reads this project as elsewhere.
- Every write by name goes to the get-started repository: labels,
  comments, pull requests, statuses, and tag-release's pull request and
  push.

What closes it:

- A gate: no instance, remote or setting names `tehas-app/tehas` before
  the name is reused. Items 4 and 5 list where to look.
- To build: `tehas update` refuses with 78 when `TEHAS_REPO` still names
  `tehas-app/tehas`, and does not only warn. Today nothing checks
  `TEHAS_REPO` for the old name. `console/core/src/release.rs:38`
  (`FORMER_REPO`) and `moved` (:49) are read only for
  `TEHAS_RELEASE_REPO` (`console/ops/src/update/release.rs:959`) and the
  checkout's origin (`console/ops/src/update/checkout.rs:91`), and both
  only say a line. For the reviewer: the same change could turn those two
  lines into refusals, since after the reuse they read from the
  get-started repository (a release from there fails the built-in
  signature check, `console/core/src/release.rs:11`, but the update should
  stop before it asks).

## 4. Remotes of operator clones

Status: **before the reuse**.

tag-release reads the project from `git remote get-url <remote>`
(`console/xtask/src/tag.rs:1026` to :1028, #3200; wired at
`console/xtask/src/main.rs:255`). A clone whose origin names
`tehas-app/tehas` opens its pull request and pushes there. Today the
redirect carries both. After the reuse both go to the get-started
repository.

A host that updates from its checkout (`TEHAS_UPDATE_FROM=git`) reads its
origin the same way; `tehas update` says so today
(`console/ops/src/update/checkout.rs:91`).

What closes it: each operator clone and each host checkout sets its
origin to `tehas-app/tehas-factory` before the reuse.

## 5. The rest of T10's own list

| What | Where | Status |
| --- | --- | --- |
| The release repository | `host/versions.env:78`, `TEHAS_RELEASE_REPO=tehas-app/tehas-factory` | done (#2980) |
| The installer | `host/release/install:4` (the address) and :31 (`repo=`) | done (#2980) |
| `tehas update` on the old name | `console/core/src/release.rs:38`, `console/ops/src/update/release.rs:959`, `console/ops/src/update/checkout.rs:91`: it says a line | done as T10 asks; the refusal is item 3, before the reuse |
| The schema ids in `api/` | 43 of 43 `$id`s name `tehas-app/tehas-factory` (for example `api/disk.schema.json:3`) | done (#2980) |
| The manual | `host/docs/contract.md:912` to :930 (Changed in 2.0), `host/docs/tools-update.md:177` to :181 and :350 to :355 | done |
| The setting as a contract change (T9) | `host/docs/contract.md:916` to :930, with its adopter action | done |
| AGENTS.md | names no repository as owner/name; nothing to change | done |
| The tests | `console/tests/repo_name.rs` (nothing outside its `KEPT` list, :15, names the old name), `console/tests/update_release.rs:681`, `console/tests/update_checkout.rs:770`, `console/github/tests/port.rs:191` and :214 | done |
| The worked example instance | `instances/tehas-tehas/instance.env:2` and :4, `TEHAS_REPO=tehas-app/tehas`. `repo_name.rs` keeps it: a person changes it | before the alpha (item 1) |
| Old installs' fixtures | `test/fixtures/install-0.30/versions.env:72`, `test/fixtures/install-0.32/versions.env:72` | kept: what those releases left on a host |
| The repository's rename on GitHub | the owner's; the live query in item 2 answers `tehas-app/tehas-factory` | done |

## Where the old name is today

`grep -rnP 'tehas-app/tehas(?![-\w])'` over the tracked tree at 98a369aca,
`CHANGELOG.md` included (it has none): 33 lines.

| Area | Lines | Kept because |
| --- | --- | --- |
| `console/core/src/release.rs` | 4 | the one rule that knows the old name |
| `console/tests/update_release.rs` | 3 | the test of that rule |
| `host/docs/tools-update.md` | 4 | the line `tehas update` says |
| `host/docs/contract.md` | 3 | Changed in 2.0 |
| `changelog.d/v2-t10-rename.md` | 2 | the note of the rename |
| `instances/tehas-tehas/instance.env` | 2 | a real instance; to change (item 5) |
| `test/fixtures/install-0.3*/versions.env` | 2 | old releases' files |
| `docs/plans/` | 7 | records (2 are T10's own text) |
| `docs/audits/` | 5 | records |
| `docs/specs/` | 1 | records |

`console/tests/repo_name.rs` spells the name in two halves, so it is not
counted. Each place matches a line of its `KEPT` list, so the test passes.
