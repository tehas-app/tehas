# Synthetic answers of the code host's branch checks question

The one question is `BRANCH_CHECKS` in
`console/github/src/port/code_host/checks.rs`. The stub in
`tests/common/mod.rs` serves these answers.

Source of every file here:
`console/github/tests/recorded/branch-checks/live/question-main.json`, the
live answer of the one question for `main` of tehas-app/tehas-rec-branch-checks
(kit run #1, step f, 2026-11-26). Its suites count from 0 in
`data.repository.ref.target.checkSuites.nodes`. Both files move the head to
the stub's, so the commit the dispatcher names is the stub's.

| File | Changed from the recording |
| --- | --- |
| `none.json` | `oid` is the stub's head `0123456789abcdef0123456789abcdef01234567`; `checkSuites` is `{"totalCount": 0, "nodes": []}`; `status.contexts` is `[]` |
| `verify-passed.json` | `oid` is the stub's head; suite 0 (workflow Lint's, no run of `verify`) is left out and `totalCount` is 1; run 139972228367 of `verify` has `conclusion` `SUCCESS`, not `FAILURE`; `status.contexts` is `[]` |
