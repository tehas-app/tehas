# Spec: outsource, cycles run on contractors

**Status**: Draft 2026-11-16, corrected the same day with the owner's
decisions on the open points. Nothing is built.

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

1. **One API for both.** Everything a program that manages factories does to
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
   settings for the container that may leave the factory (see "Settings and
   the model") and a job token.
4. The contractor runs the same container as `libexec/work` runs, with the
   same pinned mounts and variables.
5. The contractor checks in while it runs and streams the cycle's progress.
   It renews the cycle's GitHub token and relays the cycle's issue writes
   over the same channel. See "The job channel".
6. The contractor posts the result JSON. The code reaches GitHub by the
   branch the cycle pushes, as today. The factory reads no workspace.

The dispatcher decides where a cycle starts. It starts in-house unless the
feature is on, the repository allows it, and a contractor fits: it is idle, the
image digest matches, it has the room the cycle needs, and it is not
resting. Among those, the one with the most room.

A cycle that needs a polyfill's token stays in-house, whatever else allows
it. The dispatcher reads the need from the polyfills the cycle would load:
each polyfill declares the settings it needs that are secret, and a cycle
with any such polyfill is not outsourced. No polyfill declares one today.

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
- `lost`: the timeout passed with no check-in, or the job was revoked. A
  result or "stopped" report from a lost job that was not revoked is still
  taken until the issue's next attempt is given out.

Each state change is a row in the store. The factory reads the store on
start and sorts out the jobs that expired while it was down.

### The job channel

Everything a running job needs from the factory goes over one channel: the
contractor calls the factory with the job token. What the factory requires
depends on the call:

- Check-in, token renewal and relay: the job is `running`, from the issue's
  current attempt, on a contractor that is not revoked.
- Result and "stopped" report: the job is from the issue's current attempt,
  on a contractor that is not revoked. It may be `running`, `stopping` or
  `lost`, so a report sent while stopping or after a blip is still taken.
  A job that was replaced by a newer attempt is refused.

Only an explicit "revoked" answer means the job is revoked. A refusal for a
lost job, or a factory that does not answer, takes the salvage path of
"Dying gracefully": the contractor pushes the branch and reports.

| Call | What it does |
| --- | --- |
| check-in | "still here", with the cycle's progress |
| token renewal | asks for a fresh GitHub token for the cycle |
| issue relay | asks the factory to make one issue write for the cycle |
| result | posts the result JSON |

- **Token renewal.** A local cycle's GitHub token is renewed on the host
  while the cycle runs (`TEHAS_GITHUB_RENEW_EVERY`). On a contractor the
  renewer asks the factory instead, with its job token. The factory makes
  the same narrowed cycle token a local cycle gets and hands it back. The
  contractor writes it where the container reads it, so the container sees
  no difference.
- **Issue relay.** A cycle's token reads issues and cannot write them, so a
  local cycle's issue writes go to the host's issue relay. On a contractor
  the container leaves the same requests, and the contractor sends each one
  to the factory as a relay request. The factory reads it as hostile and runs
  the same checks as the local relay, then writes with its own token and
  sends the answer back.
- **The App's key stays on the factory.** The contractor never holds it, nor
  any token that can write issues. A stolen running box still holds the
  cycle token and the job token. With them the thief can push to the
  repository and write pull requests (the cycle token has contents and pull
  request write), renew the cycle token and send relay requests, until the
  job ends or `tehas outsource revoke` runs.

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

A job carries only the `PI_ENSEMBLE_*` names on an allow list, with the values
the cycle would get in-house. The allow list is part of the release, not a
setting. Everything else stays on the factory:

- A name not on the allow list, even one the cycle would get in-house.
- The model's base URL and key. The job carries the model proxy's address and
  the job token in their place.
- A token or key a polyfill needs. A cycle that cannot run without one runs
  in-house.

The contractor only passes on what the job carries. The container's mounts
and variables stay the ones `test/work_test.sh` pins, for the remote path
too.

All outsource settings are instance settings, loaded like every other one:

| Setting | Default | Meaning |
| --- | --- | --- |
| `TEHAS_OUTSOURCE` | `0` | `1` turns the feature on; `0` or unset is off, as for every switch |
| `TEHAS_OUTSOURCE_TIMEOUT` | `600` | seconds without a check-in before a job is lost |
| `TEHAS_OUTSOURCE_RETRIES` | `2` | lost jobs per issue before it runs in-house |
| `TEHAS_OUTSOURCE_REST` | `1800` | seconds a contractor that lost a job rests |
| `TEHAS_OUTSOURCE_REPOS` | none | repositories that may outsource |
| `TEHAS_OUTSOURCE_CONTRACTORS` | none | contractor names allowed to call the factory, space separated like `TEHAS_ALLOWED_LABELERS`; empty refuses every contractor |
| `TEHAS_OUTSOURCE_PROXY_ADDR` | `127.0.0.1:8090` | the address and port the model proxy listens on, never the dashboard's |
| `TEHAS_OUTSOURCE_PROXY_IDLE` | `900` | seconds the model proxy waits on a quiet stream; must be longer than `TEHAS_OUTSOURCE_TIMEOUT` |

These names and defaults are final for 1.0. Times are whole seconds, as the
other duration settings are. Loading them has no side effects, and a
malformed one stops the command with exit 78 before anything is written. So
does a `TEHAS_OUTSOURCE_PROXY_IDLE` that is not longer than
`TEHAS_OUTSOURCE_TIMEOUT`.

### The model proxy

Contractors get the model through the factory, and only through it.

- The contractor talks to a factory address with the job's token. The factory
  forwards to the endpoint it has configured, which may have come from
  whoever manages the instance settings. Where it points is not a concern of
  this feature.
- The contractor never holds the real key. The job token dies with the job and
  with revocation.
- The proxy passes streamed replies as they arrive and does not buffer a
  reply. Its idle timeout is `TEHAS_OUTSOURCE_PROXY_IDLE`, longer than the
  check-in timeout.
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
- Per job: the allow-listed settings, the cycle's GitHub token, which reads
  issues and cannot write them and is renewed over the job channel, and the
  job token for the model proxy and the channel. All die with the job.
- The workspace is a RAM disk or a per-job directory. It is wiped when a job
  ends and when the program starts. No source stays in layers or caches.

### Threat model

| Case | What the thief gets | Answer |
| --- | --- | --- |
| Stolen idle box, disk encrypted | Nothing | Disk encryption (below) |
| Stolen idle box, not encrypted | The identity key and the public image | `tehas outsource revoke` ends it. No source, no GitHub or model credential at rest |
| Stolen running box | The running job's source, the cycle token and the job token: pushes, pull request writes, token renewal and relay requests until the job ends | `tehas outsource revoke` and the short job life. Not preventable |
| Malicious owner | What a job gives | Allowlist, per-repository permission, review of the result |
| Network lost | Nothing | The timeout and graceful death |

### The image

Encrypting the sandbox image is not part of this. The image is generic: no
source, no token, no key. A box that runs the image holds its decryption
key, so encryption would protect the one thing that is not secret. The
protection is that nothing secret is at rest.

### Image, code index and memory

Epic #2362 builds these. In short:

- **Image.** A factory builds its sandbox image locally for its own uid and
  publishes it nowhere. The factory serves it to contractors, as `docker save`
  output under contractor authentication. "Image digest" in this spec is the
  factory's image id. The contractor loads the image, checks the id and runs
  the container as the factory's uid.
- **Code index.** pi-rukas finds code through the codebase-memory index, and
  headless `/work` never builds one. The index answers only at the path it
  was built for, the factory's `$TEHAS_HOME/work/<name>`. The factory serves
  the index for the job's tip. The contractor mounts the workspace at that
  path inside the container and copies the index into the cycle's cache, as
  a cycle in-house does.
- **Memory.** A cycle's agents write vipune memories for later cycles. Today
  they are lost with the cycle, in-house too. A cycle's new memories come back
  to the factory as candidates: with the result from a contractor, or by an
  export the factory runs in-house. They turn active when the cycle's pull
  request merges, and every cycle starts with the active ones. Nothing a
  container wrote is trusted before that merge, and the factory never keeps
  an embedding a container made.

### Disk encryption

Full disk encryption protects a powered-off box. The key must not be on the
box: TPM sealing or network unlock. That is the host owner's setup. The
manual says how, and `tehas-contractor doctor` checks it.

### Controls

- An allowlist of contractors in the instance directory,
  `TEHAS_OUTSOURCE_CONTRACTORS`, like the labeller list. A contractor
  outside it is refused.
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
  requires. `running` and `room` count outsourced cycles as they count
  local ones: a cycle on a contractor is a running cycle of the factory.
- **Features and flags:** `features` keeps its meaning, what the release has.
  The `flags` object, named in `features`, maps each feature name to whether
  this instance has it on. Outsource on is `"flags": {"outsource": true}`.
  While outsource is still being built, it is in `flags` as `false` and not in
  `features`.
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
   feature and the repository allow it; no fit means in-house; a cycle whose
   polyfill declares a secret setting stays in-house with a contractor free.
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
12. **Contract:** the `contractors` field matches its schema and samples;
    `running` and `room` count an outsourced cycle; `flags` says whether
    outsource is on.
13. **Job channel:** a check-in, a renewal and a relay request for a job
    that is not `running` are refused; a result from a stale attempt or a
    revoked contractor is refused; a result posted while the job is
    `stopping` is taken; a result after the job is `lost` but its attempt is
    still current is taken; a refusal for a lost job leads to salvage, only
    "revoked" to no push; a relay request the local relay would refuse is
    refused; no reply carries the App's key.
14. **Allow list:** a job carries only allow-listed `PI_ENSEMBLE_*` names; the
    model's base URL and key are replaced by the proxy's address and the job
    token; a polyfill's token never leaves.
15. **Generic:** `scripts/check-generic.sh` covers the new files.

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

Decided:

1. **Trust scope.** 1.0 is for hosts the owner trusts only. Allowing other
   people's hosts would change the threat model and is not planned for 1.0.
2. **Timeout values.** Timeout 600 seconds, rest 1800 seconds, 2 retries.
   The grace period is `TEHAS_WORK_GRACE`, as for a local cycle.
3. **The names of the settings** are the ones in the table above.
4. **Relay and token renewal** go over the job channel. The contractor never
   holds the App's key.
5. **What leaves the factory:** only allow-listed `PI_ENSEMBLE_*` names.

Still open:

1. **pi-rukas base URL.** See step 1 of the order of work (tehas#2271).
2. **One factory per contractor.** Sharing a pool across factories would need a
   broker and is not planned.
