# Build hosts: a factory may outsource cycles to brawny hosts

Recorded 2026-11-16 from a request of the owner and a read of `main` at
`988c2543`. A draft: nothing here is built.

## What is wrong

One factory is one host. `console/core/src/sandbox.rs` builds the argv of
every cycle container as a local `docker run`, and the room for them is the
host's own `tehas.slice` (`TEHAS_TICKET_CONCURRENCY`, `TEHAS_SLICE_CONTAINERS`).
A factory that wants more parallel cycles, or bigger ones, has to grow its VM.
A person with a few strong machines that already run self-hosted GitHub CI
runners (for example `gin` and `tonic`) cannot lend them to the factory.

## Goal

A factory may name build hosts in its instance directory. The dispatcher
starts a cycle's container on one of them when it has room. With none named,
nothing changes: every cycle runs on the control host as today.

The control host keeps the dispatcher, the state store, the GitHub token
handling and the dashboard. A build host only runs containers.

## Settings

In `optional.env`, read by `lib/config.sh` like the rest:

```
# Hosts that run cycle containers, each with the cycles it may hold at once.
# Unset: every cycle runs here.
#TEHAS_BUILD_HOSTS="gin:4 tonic:8"
# SSH user, and the key the dispatcher (never a container) connects with.
#TEHAS_BUILD_USER=tehas-build
#TEHAS_BUILD_KEY=~/.config/tehas/build_ed25519
# Whether the control host runs cycles too, when build hosts are named [1].
#TEHAS_BUILD_LOCAL=1
```

A malformed value (a host with no slot count, a count that is not a positive
integer, a name that is not a host name, a key that does not exist) stops
every command with 78 before anything is called or written. Nothing falls
back to local when a build host is wrong.

## Design

### Where the workspace lives

Cycles push their branch to GitHub; the workspace is not needed back. So the
whole cycle moves, not only the container: the clone, the install container
(`install_run`), the push check (`push_check`) and the cycle container
(`cycle_run`) all run on the build host, against a workspace under the build
host's own `TEHAS_BUILD_ROOT` (default `~/tehas-build`). Nothing is synced
from the control host.

What comes back is the exit status, the cycle's log lines, and the heartbeat
the supervisor already reads. The log is streamed over the SSH channel into
the control host's `logs/` as now.

### How a command reaches the host

The dispatcher runs the argv over `ssh -i <key> <user>@<host> -- docker ...`,
one connection per container, with `ServerAliveInterval` so a dead host is
seen. It does not use `DOCKER_HOST=ssh://`: the bind mounts of `cycle_run` are
paths on the machine where the daemon runs, so the argv must be built with the
build host's paths, and an explicit ssh call keeps that visible and testable.
`sandbox.rs` takes the root and the user as arguments, as it takes `tree` and
`host` today.

### Choosing a host

The factory sees only the slots it was given. It counts its own running cycles
per host from the store and picks the host with the most free slots. It does
not know what other factories use of the same machine (see "Fleet" below). A host at its count is skipped. When every host
is full and `TEHAS_BUILD_LOCAL=1`, the local slice is used. Otherwise the
issue waits, as it waits today for a full slice.

### A host that goes away

A connection that drops mid-cycle ends the cycle as `host-lost`, not as a
failed cycle. The issue is not parked for it: it is requeued, up to
`TEHAS_BUILD_REQUEUES` times, and the host is held out of choice for a few
minutes. Containers left on a lost host are named with the cycle, so
cleanup on reconnect removes them by name.

### What a build host needs

- Docker, and the sandbox image at the pinned tag. A first use pulls it; the
  dispatcher refuses a host whose image label (`tehas.rukas.images`) does not
  match the control host's, with the reason in the log.
- The egress proxy container (`tehas-egress`), started per host by the same
  code that starts it locally, so `TEHAS_EGRESS=1` holds there too.
- A user that may run `docker` and nothing else the factory does not need.
- The model endpoints reachable from the host. Cycles call the model from the
  container, not through the control host.

`bootstrap.sh` gets a `build-host` target that prepares these on a machine,
and `tehas build-hosts check` runs the same checks from the control host
without starting a cycle.

### Sharing a machine with CI runners

A machine that is also a GitHub runner is shared on the CPU and memory, not on
the workspace. The slot count in `TEHAS_BUILD_HOSTS` is the whole of what the
factory takes; the person sets it low enough to leave the runner its share.
Cycle containers keep their own limits (`--cpus`, `--memory`, pids) as today,
so a cycle cannot take the host. The build root is separate from the runner's
work directory. The factory does not touch the runner service.

## Security

This is a change in what a cycle's container touches, so the pull request
says so (hard rule 4).

- The container is given the same mounts and environment as now. The pin in
  `test/work_test.sh` stays unchanged and a new case pins the argv built for
  a build host to the same shape, with only the paths changed.
- The SSH key is read by the dispatcher on the control host only. It is never
  mounted, passed as a variable or written to a log. A container never sees
  it, and a build host holds no credential of the factory beyond what a
  cycle's container is given now.
- A build host is trusted with the cycle's token and the model keys for the
  length of a cycle. A host a person does not control must not be named.
- Authorisation is unchanged: it stays who applied the label, decided on the
  control host before any host is chosen.

## Tests first

- `lib/config.sh`: each malformed form of the new settings exits 78, loading
  has no side effect, and unset leaves behaviour as today.
- `sandbox.rs`: the build-host argv equals the local argv with the build
  root and user substituted; an ssh wrapper quotes every word.
- The chooser: free slots, full hosts, `TEHAS_BUILD_LOCAL`, a held-out host,
  all in Rust unit tests with an in-memory store.
- `host-lost`: a stubbed ssh that exits 255 mid-run requeues once and then
  stops at the limit.
- No test needs a real second machine. `t_stub` replaces ssh and docker.

## Docs

`host/docs/host.md` (settings and the build-host setup), `host/docs/operating.md`
(a lost host), `host/docs/features.md` (not proven until a cycle has run on a
real build host and the host's name is in its record) and a changelog
fragment with an adopter action only for factories that name hosts.

## Fleet (tehas-industry)

This spec is the factory side: the executor and the setting. A registry of
hosts, slot sharing between factories on the same machine, provisioning and
the share left to a machine's CI runners belong to tehas-industry, which writes
`TEHAS_BUILD_HOSTS` into each instance. A single factory with no industry works
from the setting alone.

## Out of scope

- A host registry and cross-factory slot accounting (tehas-industry).
- Running the dispatcher or the store anywhere but the control host.
- Syncing a workspace from the control host to a build host.
- Dev sessions, planning turns and the refresh. They stay local first. If the
  design holds for cycles, a follow-up moves them.
- Scheduling by a host's real load. Slot counts are set by a person.
- Starting or stopping runner services.

## Done when

- A factory with `TEHAS_BUILD_HOSTS="gin:4 tonic:8"` runs a cycle on `gin`, and
  its record in the analytics store names the host.
- A factory with the setting unset behaves as before, and its tests are
  unchanged.
- A cycle on a host that is switched off midway is requeued once, and the
  host is skipped for the hold time.
- `scripts/verify.sh` passes, and the proof on a real host is written down in
  `features.md` with what was seen.

## Open points for the owner

1. The model is reached from the build host directly. Do `gin` and `tonic`
   reach `b200-1` and the backup endpoint, or must the control host proxy?
2. Is `TEHAS_BUILD_LOCAL=1` the right default when hosts are named?
3. Are dev sessions wanted on build hosts in the first release, or later?
