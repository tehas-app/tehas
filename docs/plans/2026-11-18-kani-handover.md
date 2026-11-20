# Kani proofs on train/v1.1.0: handover

State on 2026-11-18. Epic #2444, parts #2445 to #2449, milestone v1.1.0.

## How we work

- Work goes straight onto `train/v1.1.0`, one commit (or merge) per issue. No
  pull request per issue.
- One pull request from `train/v1.1.0` into main, opened after v1.0.0 is
  tagged.
- Each issue is the scope of one agent or session. A proof only proves: a
  defect it finds is fixed in its own commit with a regular test and, when a
  host would notice, a changelog fragment.

## Done

All five parts are on the branch. 30 harnesses, all verified; one local run
of the whole job took 12 minutes. Each issue has a closing comment with the
details. The issues are still open; close them when the train lands.

| Issue | Where | Notes |
| --- | --- | --- |
| #2445 retry bounds | `console/core/src/retry.rs` | One call keeps every promise of its `Outcome`; eight calls on one `Budget` pay at most `RETRIES_PER_CALLS` |
| #2446 slice arithmetic | `console/core/src/slice.rs` | Found and fixed two overflows: the memory a reserve needs (reachable with valid settings, `changelog.d/2446.md`) and the CPU share and pids limit (unreachable) |
| #2447 zip and path rule | `console/attach/src/zip.rs`, `unpack.rs` | Refactored for the solver (index-based split, byte cleaning, directory read in its own function). No behaviour change |
| #2448 placement | `console/dispatch/src/place.rs` | `fits` and `choose` split out. No behaviour change |
| #2449 grant decision | `console/core/src/grant.rs` | **Security change.** The poll does not call `grant`: its steps share `standing` and `account_of` with it, and a proof shows those agree with `grant`; 465 label shell-test runs gave identical output before and after |

## The proofs job

- It runs in the release gate only: the `proofs` job of
  `.github/workflows/release-gate.yml`. Pull requests and pushes to main run
  no proofs. A failed proof stops `tag-release` from tagging.
- Kani is pinned (`KANI_VERSION: 0.68.0`) and the job runs with
  `--all-features`. A crate with a `#[cfg(kani)]` module joins `KANI_CRATES`
  (today `tehas-core attach tehas-dispatch`); `test/kani_workflow_test.sh`
  holds the list to the crates.
- It needs a runner labelled `[self-hosted, kani]`. **None is registered
  yet**: until one is, the release gate waits on this job. See
  "Registering a kani runner" in `host/docs/operating.md`. Use a machine with
  8 or more cores and 32 GB or more.

## Decisions for you

From #2449. Neither is changed on the branch.

1. **The account `unknown`.** The pins rename the actor of a refused
   application to `unknown`. If `TEHAS_ALLOWED_LABELERS` named a GitHub
   account `unknown` (the login rule accepts it), those refused applications
   would grant. A harness pins this down. Fixes:
   - The settings refuse `unknown` in the list with exit 78. This refuses a
     login the docs allow, so it is a contract question.
   - The pins write a name no list can hold, such as `unknown#`. This changes
     the logged words and the pins tests, not the contract. Recommended.
2. **A removal between two reads.** Approval uses `label_actor`, which
   ignores removals; removals are enforced by the page guard and the issue's
   current labels. If the guard drops a page, REST reads it again later, and
   if the label was removed in between while the page still listed it, one
   poll credits the earlier application. Narrow, but it is authorisation.

Both are 1.x security fixes if taken; give each its own issue.

## Run the proofs by hand

```
git switch train/v1.1.0
cargo install --locked kani-verifier --version 0.68.0
cargo kani setup
cargo kani --manifest-path console/Cargo.toml --all-features -Z stubbing -p tehas-core -p attach -p tehas-dispatch
```

One harness: add `--harness <module>::proofs::<name>`. A failure:
`... 2>&1 | grep -B3 -A3 "Status: FAILURE"`.

## What we learned

- **Features gate modules.** `retry` is behind `tehas-core`'s `mask` feature;
  without it Kani finds no harness and says so without failing. The job uses
  `--all-features` for this reason.
- **Stubbing needs `-Z stubbing`.** Stub to a field the input sets freely
  (`owed_a_retry` to `unread`), so the proof can ask afterwards what the stub
  answered.
- **`assert_eq!` on Vecs unwinds `memcmp`** and fails with "unwinding
  assertion loop 0" at a low unwind. Compare length and elements.
- **Input length drives time.** A 12-byte string scan took 7.5 minutes, 10
  bytes under 4. Prefer index loops over `split` and bytes over chars.
- **Ignore the alloc noise.** Lines about `rust_alloc_error_handler` "not
  currently supported" do not fail a proof. Read `VERIFICATION:` and any
  `Status: FAILURE`.

## Open

- Register the `kani` runner, then run the release gate once on the train's
  head.
- The two decisions above.
- #2442 itself (start reasons, blockers report, graph proofs) is not started.
  Its proofs join `KANI_CRATES` through `tehas-dispatch` or a new crate.
- A first host's local clone held an older main whose hashes differ from
  GitHub's. Its abandoned local Kani commits are on the local branch
  `backup/kani-local-2026-11-18` there, superseded by this branch; delete it.
