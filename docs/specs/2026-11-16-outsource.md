# Spec: outsource, cycles run on contractors

**Status**: Draft 2026-11-16, for the owner's review. Nothing is built.

## Purpose

CPU, RAM and disk are dear in the cloud, and many people have idle hosts:
gaming PCs, office boxes. **Outsource** lets a factory send whole cycles to
such hosts. A host that takes cycles is a **contractor**.

Outsource is **optional**. By default every cycle runs in-house, as today.

It is simple to deploy. A contractor is one program and one unit. There is no
cluster and no orchestrator.

## Optional: the guarantees

These come first because everything else depends on them.

1. **Off unless asked.** With no `TEHAS_OUTSOURCE` setting, every cycle runs
   in-house. There is no default contractor list and no fallback to
   outsourcing.
2. **Nothing extra runs.** The contractor program and the model proxy are
   installed only when a person chooses the feature. Off, there is no new
   unit, no open port and no new process in the dispatcher's path.
3. **Nothing changes on screen.** Off, there is no Contractors panel, no
   `live/contractors.json`, no `contractors` field in `tehas status --json`, and
   commands print what they print today.
4. **No new state.** The contractor and job tables are created only when the
   feature is on. Off, nothing reads or writes them.
5. **Fail closed both ways.** Off, `tehas outsource` says so and exits
   without touching anything. A contractor that calls a factory that has it off
   is refused.
6. **It never blocks the line.** On, with no contractor free, lost or revoked,
   cycles run in-house. Turning it off applies to the next job.
7. **Per repository.** On, a repository must also allow it. A new
   repository does not.

Test: the existing suite passes unchanged with the feature off, and the
output of status and the dashboard files equals today's samples.

## Terms

| Term | Meaning |
| --- | --- |
| contractor | A host that runs cycles for a factory |
| job | One cycle given to one contractor for a limited time |
| check-in | The contractor's regular "still here" call |
| timeout | How long the factory waits without a check-in before it gives up on a job |
| attempt | A number that rises each time an issue's job is given out |
| rest period | A time during which a contractor that lost a job gets no work |

A contractor belongs to one factory. The factory is the only dispatcher and
the only holder of state.

## Goals

1. **One API for both.** Everything industry or any other program does to
   contractors, a person can do with the same command and flags.
2. **Stamp who built it.** Tickets, pull requests and history say which
   contractor built a change.
3. **Durable.** A contractor that goes offline in a cycle costs the issue a
   retry and nothing else.
4. **Die gracefully.** A contractor that loses its factory stops its cycle in
   an orderly way and leaves the host clean.
5. **Secure by design for a stolen box.** An idle contractor holds nothing
   that is worth stealing.
6. **Inherit everything.** A contractor has no settings of its own beyond its
   identity and the factory's address.

## Non-goals

- Splitting one cycle over several hosts.
- A contractor that serves several factories.
- Scheduling beyond "an idle contractor that fits".
- Protection against a contractor whose owner is malicious. See the threat
  model.
- Encrypting the sandbox image. See the threat model.

## How it works

The factory never connects to a contractor. Contractors call the factory, over
HTTPS, on the factory's own address.

1. The contractor reports in: its name, free CPU, RAM and disk, the image
   digest, the pi-rukas version and what `pi-rukas-headless --capabilities`
   says.
2. The contractor asks for a job. The factory answers with a job or with
   nothing.
3. A job carries the issue, the driver, the timeout, the attempt number, the
   settings for the container and a job token.
4. The contractor runs the same container as `libexec/work` runs, with the
   same pinned mounts and variables.
5. The contractor checks in while it runs and streams the cycle's progress.
6. The contractor posts the result JSON. The code reaches GitHub by the
   branch the cycle pushes, as today. The factory reads no workspace.

The dispatcher decides where a cycle starts. It starts in-house unless the
feature is on, the repository allows it, and a contractor fits: it is idle, the
image digest matches, it has the room the cycle needs, and it is not
resting. Among those, the one with the most room.

### Job states

```
offered -> running -> finished
              |
              +-> stopping -> released | lost
```

- `offered`: given out, not yet accepted. It expires if not taken.
- `running`: the contractor checks in.
- `finished`: the result came in from the current attempt.
- `stopping`: the contractor is ending the job. See "Dying gracefully".
- `released`: the contractor handed it back on purpose. Not a failure.
- `lost`: the timeout passed with no check-in, or the job was revoked.

Each state change is a row in the store. The factory reads the store on
start and sorts out the jobs that expired while it was down.

## Durability

1. **Check-in.** The contractor's progress stream and its own timer both
   count. A long quiet model turn does not lose the job, because the timer
   checks in on its own. The timeout is `TEHAS_OUTSOURCE_TIMEOUT`.
2. **Attempt number.** The factory accepts a result only from the issue's
   current attempt. A contractor that comes back late cannot overwrite the
   attempt that replaced it.
3. **Recovery.** A lost job takes the path of a crashed local cycle. The
   park reason is `lost: contractor offline`. The factory waits one grace
   period past the timeout so a push can land, then looks at the branch. If
   it moved, the next cycle is a refresh that continues it. If not, it
   starts fresh.
4. **Limits.** After `TEHAS_OUTSOURCE_RETRIES` lost jobs on one issue, the
   issue runs in-house or parks for a person. A contractor that loses a job
   rests for `TEHAS_OUTSOURCE_REST`.
5. **Restart safe.** The factory restarting loses nothing. Jobs are in the
   store.

## Dying gracefully

The contractor never kills a cycle outright.

1. **Detect.** It cannot reach the factory for the timeout, or the factory
   says the job is revoked.
2. **Warn.** It sends the runner its stop signal and starts the grace period,
   `TEHAS_WORK_GRACE`, the setting a local cycle uses. The runner finishes
   its step and writes its notes.
3. **Salvage if the job is still current.** On a network blip the contractor
   pushes the branch so a replacement can continue it. On a revoked job it
   pushes nothing.
4. **Report.** It sends "stopped: contractor offline" if it can. If not, it
   keeps the result file and retries. The factory takes it only if the
   attempt is still current.
5. **Clean up.** After the grace period it runs `docker stop`, removes the
   workspace and drops the tokens.
6. **Come back.** When the factory answers again, the contractor reports in.
   After a lost job it first waits out the rest period.

`tehas-contractor drain` is the same path, on purpose. The contractor stops
taking jobs, lets a running job end within the grace period, otherwise hands
it back with its branch. A release counts against nothing.

## The stamp

The factory records the contractor at the moment it gives the job out. The
record holds the contractor id, its name, the image digest and the pi-rukas
version. It is used everywhere:

- **Pull request:** a footer line, `Built by contractor: <name> (job <id>)`.
  A local cycle says `Built by: in-house`, so the line is always present.
- **Issue:** the claim and finish comments carry the same line.
- **Commits:** a `Built-by:` trailer with the contractor's name. It survives
  a squash and an edit of the pull request text.
- **Store and analytics:** a `contractor_id` on every cycle. `tehas analytics
  export` carries it.
- **Status, dashboard, MCP:** each running and finished cycle shows it.

The factory writes the stamp, not the contractor. A contractor reports only its
id. The trailer is written by the cycle on the contractor, so the factory
checks it when the cycle finishes and rewrites it if it differs.

## Settings and the model

A contractor holds its identity (a revocable key that can only ask for jobs)
and the factory's address. It has no endpoint, no model, no provider and no
key of its own. `tehas-contractor doctor` flags any it finds.

Each job carries the same `PI_ENSEMBLE_*` values the cycle would get in-house.
The contractor only passes them on. The container's mounts and variables stay
the ones `test/work_test.sh` pins, for the remote path too.

All outsource settings are instance settings, loaded like every other one:

| Setting | Meaning |
| --- | --- |
| `TEHAS_OUTSOURCE` | on or off; unset is off |
| `TEHAS_OUTSOURCE_TIMEOUT` | how long without a check-in before a job is lost |
| `TEHAS_OUTSOURCE_RETRIES` | lost jobs per issue before it runs in-house |
| `TEHAS_OUTSOURCE_REST` | rest period of a contractor that lost a job |
| `TEHAS_OUTSOURCE_REPOS` | repositories that may outsource; default none |

Names and defaults are for review. Loading them has no side effects, and a
malformed one stops the command with exit 78 before anything is written.

### The model proxy

Contractors get the model through the factory, and only through it.

- The contractor talks to a factory address with the job's token. The factory
  forwards to the endpoint it has configured, which may have come from
  whoever manages the instance settings. Where it points is not a concern of
  this feature.
- The contractor never holds the real key. The job token dies with the job and
  with revocation.
- The proxy passes streamed replies as they arrive and does not buffer a
  reply. Its idle timeout is a setting and is longer than the check-in
  timeout.
- It forwards only the model paths a cycle needs.
- It counts tokens per contractor and per job. It never logs bodies, because
  they hold source code.
- A busy upstream's answer goes back as it is, so the agents' retry of a busy
  model (#215) works unchanged.
- It runs apart from the dispatcher, so a slow stream cannot hold up
  dispatch.

A contractor also reaches GitHub and the product's package registries
directly. The network allowlist the docs describe is: the factory, GitHub and
those registries.

## Security

### What a contractor holds

- At rest: its identity key and the factory's address. Nothing else.
- Per job: the settings, a GitHub token for the repository, and a model
  proxy token. All die with the job.
- The workspace is a RAM disk or a per-job directory. It is wiped when a job
  ends and when the program starts. No source stays in layers or caches.

### Threat model

| Case | What the thief gets | Answer |
| --- | --- | --- |
| Stolen idle box, disk encrypted | Nothing | Disk encryption (below) |
| Stolen idle box, not encrypted | The identity key and the public image | `tehas outsource revoke` ends it. No source, no GitHub or model credential at rest |
| Stolen running box | The running job's source and live tokens, if the thief has root | Revocation and the short job life. Not preventable |
| Malicious owner | What a job gives | Allowlist, per-repository permission, review of the result |
| Network lost | Nothing | The timeout and graceful death |

### The image

Encrypting the sandbox image is not part of this. The image is generic: no
source, no token, no key. A box that runs the image holds its decryption
key, so encryption would protect the one thing that is not secret. The
protection is that nothing secret is at rest.

### Disk encryption

Full disk encryption protects a powered-off box. The key must not be on the
box: TPM sealing or network unlock. That is the host owner's setup. The
manual says how, and `tehas-contractor doctor` checks it.

### Controls

- An allowlist of contractors in the instance directory, like the labeller
  list. A contractor outside it is refused.
- `tehas outsource revoke <contractor>` ends a contractor at once: its check-ins,
  its model proxy access and its results are refused.
- A result from a contractor is untrusted. It goes through pull request review
  and CI like any other cycle. Nothing merges on its own.
- A repository must allow outsourcing. The default is no.

## Visibility

- **Factory dashboard:** a Contractors panel and `live/contractors.json`, only
  when the feature is on. Per contractor: name, status, the issue it is on, age
  of its last check-in, image digest, pi-rukas version, jobs done and jobs
  lost. Statuses: `idle`, `working`, `stopping`, `silent`, `resting`,
  `revoked`, `draining`. No addresses, no keys, no host names beyond the
  contractor's own name.
- **MCP:** `factory_contractors`, read only, behind the dashboard keys.
- **Status contract:** `tehas status --json` gains an optional `contractors`
  object, with `api/` schema and samples, as the status contract spec
  requires. The factory's feature list says whether outsource is on.
- **Analytics:** `contractor_id` on cycles.

## Programs that manage factories

A program that manages several factories uses the same surface as a person:
the instance settings, `tehas outsource invite`, `list`, `revoke`, `drain` and
`status` with `--json`, and `tehas status --json`.

- A change to the model endpoint or any outsource setting is a change to the
  factory's settings. The next job carries it. Nothing is pushed to
  contractors, and a running job keeps what it started with.
- The tehas side has no setting, endpoint or key type that exists only for
  such a program. A standalone install uses the same commands.

## Commands

All take `--json` and exit 78 when the feature is off or a setting is
malformed.

```
tehas outsource invite [--name <name>]   print the command a host runs to join
tehas outsource list
tehas outsource status
tehas outsource revoke <contractor>
tehas outsource drain <contractor>
tehas-contractor join <invite>
tehas-contractor run
tehas-contractor drain
tehas-contractor doctor
```

### The word "outsource" in the CLI

The feature name shows wherever a person looks:

- Every command of the feature lives under `tehas outsource`.
- `tehas status` prints an `outsource:` line, only when the feature is on:
  how many contractors are idle, working and resting, and how many cycles
  are outsourced.
- Each cycle in `tehas status` and in the logs says `outsourced to
  <contractor>` or `in-house`.
- `tehas-contractor --help` and its log lines start with `tehas outsource
  contractor`, so a person who finds it on a host knows what it is for.
- The unit is `tehas-outsource-contractor.service`, so `systemctl` shows the
  feature name too.

## Where the code goes

New logic is Rust (AGENTS.md). Nothing here needs more bash than a thin unit
and the install step, and the bash ceiling does not rise.

- `console/contractor/`: the contractor program, a new workspace crate.
- The job, contractor and attempt state in the store crate, with tables created
  only when the feature is on.
- The dispatcher chooses where a cycle starts.
- The model proxy, a separate task in the console.
- `host/docs/outsource.md`, and `features.md` marks it as not yet proven.
- `install.sh` installs the contractor program and proxy by feature.

## Tests

The gate needs no network and no sandbox image. Stub the outside world with
`t_stub`.

1. **Off:** the whole suite is unchanged; no new file, field, table or panel.
2. **Settings:** a malformed `TEHAS_OUTSOURCE_*` exits 78 before any write;
   loading has no side effects.
3. **Placement:** in-house by default; a contractor is picked only when the
   feature and the repository allow it; no fit means in-house.
4. **Job states:** each transition, and the factory restart in each state.
5. **Loss:** a silent contractor; a late result is rejected by attempt number;
   the retry cap; the rest period; a moved branch means a refresh.
6. **Dying:** a stop signal in each cycle step; the network dropping between
   salvage and report; a revoked job pushes nothing; drain versus crash.
7. **Stamp:** the footer, trailer, store row and analytics column; a
   contractor's wrong trailer is rewritten.
8. **Settings inherited:** a contractor with no settings gets the factory's
   values; a change reaches the next job and not the running one; `doctor`
   flags a model setting on the box.
9. **Container:** `test/work_test.sh` pins the same mounts and variables for
   the remote path.
10. **Proxy:** streaming passes through; an expired or revoked token is
    refused; a path outside the model paths is refused; no body is logged.
11. **Secrets:** a scan of the contractor's directories finds no GitHub or
    model credential at rest; the workspace is wiped at start and end.
12. **Contract:** the `contractors` field matches its schema and samples.
13. **Generic:** `scripts/check-generic.sh` covers the new files.

## Order of work

1. Check that pi-rukas takes a base URL and token per run. If not, the fix
   goes into pi-rukas first.
2. Store tables and the job state machine, with the feature gate.
3. The model proxy.
4. The contractor program and `tehas outsource` commands.
5. Placement in the dispatcher.
6. The stamp.
7. Visibility: status contract, live file, panel, MCP, analytics.
8. Manual, `features.md` and the changelog fragment.

## Open points

1. **Trust scope.** Version 1 is for hosts the owner trusts. Whether other
   people's hosts are ever allowed is the owner's call, and it changes the
   threat model.
2. **Timeout values.** The default timeout, grace and rest period.
3. **pi-rukas base URL.** Not yet checked. See step 1.
4. **The names of the settings** above.
5. **One factory per contractor.** Sharing a pool across factories would need a
   broker and is not planned.
