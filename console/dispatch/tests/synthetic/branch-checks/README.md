# Synthetic answers of the code host's branch checks question

No live answer of the one question (`BRANCH_CHECKS` in
`console/github/src/port/code_host/checks.rs`) is recorded yet; these stand
in until the recordings kit's step f records one. The stub in
`tests/common/mod.rs` serves them.

- `none.json`: the live answer `console/github/tests/recorded/activity/live/branch-trunk.json`,
  changed: `name` `trunk` to `main`, `oid` to the stub's head
  `0123456789abcdef0123456789abcdef01234567`, and an empty `checkSuites` and
  `status.contexts` added under the target.
- `verify-passed.json`: `none.json`, with one suite added: suite
  123253441310 and its run 138995376530 from the live recordings
  `console/github/tests/recorded/code-host/node/checks-36ca6a74f19204e710057645c9c28974fc459362-1.json`
  and `runs-36ca6a74f19204e710057645c9c28974fc459362-1.json`, in GraphQL's
  field names and upper-case words; changed: the run's name
  `get_candidate_prs` to `verify`.
