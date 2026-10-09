# ADR: not adopting pi-durable (deferred)

**Status**: Deferred — 2026-10-07. Revisit when a trigger below fires.

## Context

tehas drives a headless agent harness (`pi --mode rpc --approve`) inside a restricted container for each work cycle. The cycle's state lives **outside** that container, on the host: GitHub labels are the control plane, and `.pi/work-state/*.json` plus small atomic files under `~/tehas` are the durable state that the dispatcher, the status poller, and the dashboard read.

`@earendil-works/pi-durable` (the `packages/durable` package in the pi monorepo) is a **durable agent harness**: conversations whose entries and documents are committed to storage (SQLite, JSONL, or memory) before anything is shown, tasks that checkpoint at every step so a restarted process resumes an unfinished run (`harness.resume()`), replay-safe subagents, and per-conversation environments. It is marked **experimental** — the API changes without notice between releases.

## Decision

Do not adopt pi-durable. The decision is a **deferral with a revisit trigger**, not a dismissal.

## Rationale

1. **Layer mismatch.** pi-durable *is* the agent harness. tehas already drives a complete harness (`pi --mode rpc`) in a sandbox image pinned by digest. Adopting pi-durable would **replace the inner agent layer**, not add a dependency — the sandbox image, the contract test, and the whole "restricted container runs the agent" model would have to move to it.
2. **Storage model tension.** pi-durable stores state **inside** the process (one storage per process, no cross-process locking). tehas' state lives on the host **by design**, so the dispatcher, `tehas-status`, and the tailnet dashboard can read it without talking to a live agent. Durable state inside the sandbox is gone when the container is torn down; mounting it out adds exactly the coupling this repo's audit flagged as drift.
3. **Experimental + pin-by-digest.** The package's own header says the API changes without notice. This repo's standard is pin-by-digest, upgrade-on-purpose. An experimental dependency and that standard do not compose today.
4. **Bash house style.** tehas is shell scripts plus two small Deno programs. pi-durable is a Node/TypeScript harness dependency — a different shape of project.

## What it would fix (why this is a deferral, not a no)

The 2026-10-07 audit found that a cycle which dies mid-run (container crash, OOM, host reboot) leaves a stale `status: running` that the mtime heuristic does not clear and that requires a manual `--restart`. pi-durable's checkpoint-and-resume is exactly the missing layer for that gap. If the `tehas-work` crash-recovery gap stops being tolerable, pi-durable is the candidate.

## Revisit triggers

- pi-rukas itself ships on pi-durable (the inner harness becomes durable without tehas changing shape), or
- cycles routinely outgrow the 6-hour `TEHAS_WORK_TIMEOUT` and the stale-`running` failure mode becomes common enough to warrant a durable store, or
- the pluggable issue-backend work (roadmap item 2) needs a backend-agnostic durable state layer that host-side files no longer provide cleanly.
