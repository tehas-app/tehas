---
title: Outsource
description: "Cycles run on contractors: the model proxy, the job channel, the contractor program and the commands that manage contractors."
---

# Outsource

A contractor is a machine that is not this host and does not hold the
factory's model key. Its cycle reaches the factory's model through one door:
`tehas-proxy`, a small HTTP task of its own that answers a job's token and
forwards the ask to the endpoint the instance has configured. The real key
never leaves the factory, and a contractor is never told it.

Outsource is off until you switch it on. Off, `tehas-proxy` says the word that
says why and ends 0: no listener, no new port, nothing in `live/` and no new
line in `tehas status`.

In this release outsource is a feature still being built. It cannot be
switched on: `TEHAS_OUTSOURCE=on` stops every command with exit 78 and says
that `outsource` is still being built, and `tehas api --json` lists it in
`flags` as `false`. This page says what the proxy does once the feature
ships.

This page covers the model proxy, the job channel on the same listener, the
contractor program and the `tehas outsource` commands. What is still owed is
[at the end](#what-is-not-here-yet).

## What it is for

A cycle on a contractor holds its settings, a token for the product
repository, and a token for the model proxy. It holds no endpoint, no provider,
no model and no key of its own. Pi reads one `models.json`, so the file the
contractor's container is given is written for that one job: every provider's
`baseUrl`, and a model's own where it has one, is the factory's proxy, and its
`apiKey` is the reference `${PI_ENSEMBLE_API_KEY_<PROVIDER>}`, the provider's
name upper-cased with every other character `_`. The job's token is that
variable's value, so it is in no file. The factory's endpoints, backup
endpoints and key names are not in the file. The proxy holds the factory's
real key, reads it from its own environment, writes it into one request and
forgets it.

A contractor's sandbox image needs pi-rukas v0.14.86 or later: that release
reads `PI_ENSEMBLE_BASE_URL_<PROVIDER>` and `PI_ENSEMBLE_API_KEY_<PROVIDER>` as
a run's own endpoint and key and keeps the key out of every log and file. An
image that follows `release:0` today has it.

The job token is a secret of the proxy's own. It lives in the proxy's memory
and nowhere else on the factory. It is issued when a contractor takes its job
with the channel's `job` call, and goes to the contractor in that answer. It
dies with the job, with revocation, and with a restart of the proxy. It is
never in a command line, never in a log and never in the store.

## How it is switched on

`tehas-proxy` reads the settings `tehas_load` exports, and the state store under
`TEHAS_HOME`. Its control socket is `~/tehas/state/outsource/proxy.sock`. The
socket can also issue a job's token by hand (`tehas-proxy --issue`), written
to `~/tehas/state/outsource/jobs/<job>/job-token` with mode 0600.

| Setting | Default | What it is |
| --- | --- | --- |
| `TEHAS_OUTSOURCE` | `off` | `on` runs the proxy, once the feature ships. While it is being built, `on` stops every command with 78, and so does any value but `on`, `off`, `1` or `0` |
| `TEHAS_OUTSOURCE_PROXY_ADDR` | `127.0.0.1:8090` | The address and port to listen on. Never the dashboard's `TEHAS_HOST_ADDR` port |
| `TEHAS_OUTSOURCE_CONTRACTORS` | empty | The contractors the channel answers, by name, one space apart. Empty refuses every contractor |
| `TEHAS_OUTSOURCE_TIMEOUT` | `600` | Seconds a job may go without a contractor's check-in before it is lost |
| `TEHAS_OUTSOURCE_PROXY_IDLE` | `900` | Seconds a model stream may go silent before the proxy drops it. Must be longer than the timeout above, or every command stops with 78 |

The proxy asks the endpoint of the provider the instance's agents run on, the
same one a cycle's container is given: `PI_ENSEMBLE_SUBAGENT_PROVIDER` names it
in `models.json`, and the variable that file names as its `apiKey` holds the
key in the instance's env. With the feature on and that endpoint missing, not
`http://`, or its key unset, the proxy stops with 78 and says which.

Version 1 listens in plain http. The trust boundary is the address you bind: a
contractor must reach the port, and what it can do is what is listed below.
Bind the address of a network you control, and keep it off a public interface
until the join handshake and TLS land.

## What it answers

Only the model paths a cycle's agents ask an endpoint for, whatever base path
the endpoint's own address carries:

| Path ends with | Why a cycle needs it |
| --- | --- |
| `/chat/completions` | The completion the cycle's agents run on |
| `/models` | The models the endpoint offers, for a role's pick |
| `/models/list` | The same list under another name |
| `/count-tokens` | The token count of a prompt, for a budget |
| `/completions` | A completion that is not a chat |
| `/health` | Whether the endpoint answers at all, for `doctor` |

Everything else is refused with 404 before the endpoint is called, so a probe
cannot tell a live token from a dead one by what it costs the factory. A
request line of a method the proxy does not serve, or of no shape at all, is
400, also with no endpoint call.

## What it refuses

Each refusal is one line in the unit's journal, no upstream call, and an answer
of the proxy's own that says only what its status says: no token, no endpoint,
no byte of a body.

| What | Answer | The line says |
| --- | --- | --- |
| No bearer word, or one the proxy never issued | 401 | `refused no job: a token this factory issued for no job` |
| A job whose record says `finished`, `released` or `lost`, or that the records do not know | 403 | `refused job <n> of contractor <name>: a job whose record says <state>` |
| A job whose record names another attempt or another contractor than the token was issued for | 403 | `refused job <n> of contractor <name>: a job whose record names another attempt or contractor` |
| A contractor that is revoked, or has no record | 403 | `refused job <n> of contractor <name>: a contractor that is revoked or unknown` |
| A path outside the table | 404 | `refused no job: the path <path>` |
| An endpoint that cannot be reached or answers nothing | 502 | the words of the failure, which name the host and not the key it was to send |

The job and the contractor are read from the factory's records on every
request, never from a field the contractor sent: the state store's
`contractor-list`, where a revoked contractor may not work and a draining one
still finishes its job, and `job-show` of the job the token was issued for. A job that ended or a
contractor revoked mid-stream is refused by the next request rather than
remembered wrongly from the last one. A factory whose records cannot be read
answers only refusals, and forwards nothing: that is the point of a proxy that
holds the factory's model key.

## What it passes through

- A streamed reply is copied as the endpoint sends it: the contractor reads the
  first chunk before the endpoint has sent the second. Nothing is buffered to
  its end.
- A busy endpoint's answer goes back exactly as it came, its status and its
  body, so the agents' retry of a busy model works unchanged. The proxy adds no
  status of its own to an answer it can carry; it answers with its own head
  only where there is nothing to carry.
- A request is one connection, answered and closed: `Connection: close`, no
  chunked transfer, no keep-alive, no pipeline, which is what Pi's own client
  does of a provider.

## What it counts

The requests each token carried are counted per contractor and per job, in the
proxy's memory. `tehas-proxy --counts` prints them and clears them. A count
names no token, no path and no byte of a body.

The endpoint's own token figures, `usage` of an answer or of the last chunk of
a stream, are read of each answer but not kept yet: they reach the state
store, `tehas status` and the dashboard with #2277. The proxy never guesses a
token count, and an answer it could not finish writing gives no figures.

## What it never does

- Never logs a body. A prompt holds source code. One line per request names the
  method, the path, the status, the time and whose job it was, and nothing of
  either body; a path is cut at 200 bytes and holds no line end.
- Never writes the factory's model key anywhere but one request, and never
  gives it to a contractor. The endpoint sees the factory's `Host` and its own
  key: the contractor's token and its own `Host` are dropped.
- Never holds the job token in the store, a log or a command line.
- Never forwards a path outside the table, and never reads a body to decide
  anything beyond the one word of the ask that says whether the answer is a
  stream.
- Never queues a request over its bound of streams; one over it is ended and
  the cycle retries it as it retries a busy model.

## Off means off

Outsource not on: `tehas-proxy` says `outsource is off: no model proxy
is listening` and ends 0. Nothing binds a port, nothing is written under the
state, `tehas status` says no word about the feature, and the rest of the suite
is as it was.

## The job channel

The same listener answers a contractor's calls about its work, at
`/outsource/v1/<call>`, each a POST with a small JSON body and a JSON answer:

| Call | Made with | What the factory does |
| --- | --- | --- |
| `join` | the one-time code of `tehas outsource invite` | registers the host's name and the hash of its key; the code is spent |
| `hello` | the contractor's key | notes that the contractor is there |
| `job` | the contractor's key | takes the offer in its hand: the job moves to running and its token is issued |
| `check-in` | the job's token | notes that the job is alive, while it runs |
| `token` | the job's token | answers a fresh cycle token, narrowed as a local cycle's is |
| `relay` | the job's token | checks one issue write as the local relay does and makes it with the factory's token |
| `result` | the job's token | keeps the cycle's result where its report reads it and finishes the job |
| `stopped` | the job's token | the job is handed back; its token is spent |

A key is sent as a bearer, with the contractor's name in
`x-tehas-contractor`. The store holds the sha256 of the key and of the code,
never either one.

Every call, the join included, is refused with 403 `not allowed` when
`TEHAS_OUTSOURCE_CONTRACTORS` does not list the contractor's name. A refused
join does not spend its code.

What each call needs:

- `check-in`, `token` and `relay`: the job is running, it is the issue's
  current attempt, and its contractor is not revoked.
- `result` and `stopped`: the same, or the job is stopping, or it was lost to a
  silence and its attempt is still current. A result of an attempt that a newer
  one replaced is refused with 409 `stale`.

Only a revoked contractor is answered `revoked`. A contractor that gets it
pushes nothing. Nor does one told `stale`: a newer attempt of the issue has
replaced its job, and a push would move the branch the replacement works
on. Any other refusal names the state, and a contractor that gets one, or no
answer for the timeout, pushes its branch so a replacement can continue it.

A state store that gives no answer (busy, locked, failing) is not a refusal.
The call is answered 503 with no `refused` word, and the contractor takes it
as no answer and asks again. Any 503 is read that way.

A job carries its issue, its attempt, the timeout, its token, the
`models.json` with every provider behind the proxy, and a short list of
settings that shape a cycle: `TEHAS_REPO`, the three every load requires
(`TEHAS_CONTROL_ISSUE`, `TEHAS_ALLOWED_LABELERS`, `TEHAS_GITHUB_APP_ID`, the
App's public id and never its key), `TEHAS_BRANCH`, `TEHAS_DRIVER`,
`TEHAS_READ_ONLY`, the work, verify and build timeouts, `TEHAS_WORKSTREAMS`,
`TEHAS_RUKAS_WORKAROUNDS`, `TEHAS_OUTSOURCE`, and the models and providers of
the agents' roles. The list is part of the release, not a setting. No address,
path, key or token of the factory is on it.

## A cycle on a contractor

A contractor runs the same `tehas work` as the factory does, with the same
container: the same mounts and the same variables, pinned by the same test
(`test/work_test.sh`). Two things on the host side of the container go to the
factory instead, because a contractor holds no token that writes issues and no
GitHub App key:

- The cycle's token comes from the factory over the job channel, and is
  renewed the same way. The contractor writes it where the container reads it.
  It is also the token the host side of `tehas work` acts with: a contractor
  never holds the App's own token.
- Each issue write the cycle's agents make is read and checked as the local
  relay checks it, then sent to the factory, which checks it again with the
  same rules and the job's own scope and makes the write with its own token.

This happens only with outsource on and the contractor program naming
itself to the cycle. Otherwise a cycle is as it was.

## The stamp

With outsource on, the factory says who built each cycle. It writes the
stamp itself; a contractor never does.

- **The start comment** on the issue ends with a line: `Built by contractor:
  <name> (job <id>)` for a cycle placed on a contractor, `Built by: in-house`
  for one run on this host. The contractor is the one holding the issue's job
  given out since the cycle's approval.
- **The report's comments** at the end of the cycle end with the same line.
- **The pull request's body** gets the line as its last line when the cycle
  ends. An older stamp line in the body is replaced, so the line is there once.
- **Commits.** Each commit of a contractor's pull request over its base
  branch carries a `Built-by: <name>` trailer. When the cycle ends the factory
  reads the branch into a repository of its own; a commit whose trailer is
  missing or names someone else is written again with the right one, and so
  are the commits after it. Each keeps its tree, author and committer. The
  branch is pushed with a lease on the commit it was read at, so work pushed
  since is never overwritten. A pull request that is merged or closed, one
  from a fork, and the end of a cycle after a stop are not rewritten.
- **The store** keeps who built it in the job's row: `store job-stamp --issue
  <n> --since <approval>` answers the job, its contractor, attempt and state,
  or null for a cycle run in-house.
- **The analytics** keep the contractor's name in `runs.contractor_id`
  ([the analytics](/analytics)). An in-house run leaves it empty.

When the store cannot say which job a contractor's cycle was, the comments
and the pull request carry no stamp and the log says so: a guess would be
wrong.

With outsource off nothing of this happens: no line, no trailer check, no
store call.

## The commands

```
tehas outsource invite [--name NAME] [--json]   print the command a host runs to join
tehas outsource list [--json]                   each contractor, its status and its job
tehas outsource status [--json]                 the contractors counted by status
tehas outsource revoke <contractor> [--json]    end a contractor at once
tehas outsource drain <contractor> [--json]     ask a contractor to take no new job
```

`invite` prints `tehas-contractor join <address> <name> <code>`. The address
is `TEHAS_OUTSOURCE_PROXY_ADDR`, the code lives `TEHAS_OUTSOURCE_JOIN_TTL`
seconds, and the store keeps only its sha256. Before the command it notes an
address no other host can reach (a loopback or wildcard address: put the
factory's own address in its place) and a name `TEHAS_OUTSOURCE_CONTRACTORS`
does not list. `list` gives each contractor one
of the statuses `idle`, `working`, `stopping`, `silent` (not heard of for
`TEHAS_OUTSOURCE_TIMEOUT` seconds), `resting`, `draining` or `revoked`, the
issue it is on, when it was last heard of, and its jobs done and lost.
`status` prints the `outsource:` line: how many contractors are idle, working
and resting, and how many cycles are outsourced. `revoke` loses the
contractor's jobs, and its check-ins, model requests and results are refused
from then on.

Off, or while the feature is being built, every subcommand exits 78 and
touches nothing. Exit: 0 done; 1 the store refused, or no such contractor; 64
a bad argument.

## The contractor program

`tehas-contractor` runs on the contractor's host. Its help and every line it
logs start with `tehas outsource contractor`.

```
tehas-contractor join <factory> <name> <code>   join with the code tehas outsource invite printed
tehas-contractor run                            take jobs and run them until drained or revoked
tehas-contractor drain                          take no new job; hand a running one back after the grace
tehas-contractor doctor                         say what this host holds that it should not
```

At rest it holds its name, its key and the factory's address, in
`~/.config/tehas/contractor.json` (mode 0600), and nothing else. A job
leaves the job's token, its settings, the cycle token, a `models.json` whose
every provider is the factory's proxy, and a clone of the product. All of it
is wiped when the job ends and when the program starts. The `models.json`
names the factory at the address this host joined with, whatever address the
proxy binds.

The wipe removes `repos`, `work` and `state/cycles` under `TEHAS_HOME`, which a
factory keeps too. So `run` refuses with 78, before it removes anything, a
home that holds a factory (a state store, a settings checkout, an instance
directory), and a `~/.pi/agent/models.json` it did not write. A contractor runs
on a host of its own.

A running job checks in on its own timer, a quarter of the job's timeout, so a
long quiet model turn does not lose it. It ends one of five ways:

| End | When | Cycle stopped | Branch pushed | Reported |
| --- | --- | --- | --- | --- |
| finished | the cycle ended | no | by the cycle | the result |
| offline | no check-in answered for the timeout | yes | yes | stopped, or the result if one was written |
| lost | the factory refused a check-in for the job's state | yes | yes | the same |
| stale | a newer attempt of the issue replaced the job | yes | no | nothing |
| revoked | the factory said `revoked` | yes | no | nothing |
| drained | `tehas-contractor drain`, and the grace passed | yes | yes | stopped: a release that costs nothing |

The cycle is never killed outright. It gets its stop signal and
`TEHAS_WORK_GRACE` seconds to finish its step and write its notes; only then
is it killed and its container stopped. A report the factory does not answer
is retried for the job's timeout.

The salvage push runs no git in the workspace, whose hooks and git config the
container could write. It fetches the issue's branch (`feature/issue-<n>`, or
that with a `-` and more) out of the workspace into a repository of its own,
and pushes it from there, with hooks off, to the product's repository and
never to an address the workspace names.

`doctor` exits 3 when it finds something: a model endpoint or key in the
environment, a `models.json`, a cycle token or a checkout left with no job
running, or no encrypted disk. Full disk encryption is the host owner's to set
up, with its key off the box (TPM sealing or network unlock).

The contractor has no setting of its own. `TEHAS_OUTSOURCE` must be on, as for
every command of the feature, and while the feature is being built every verb
exits 78.

## What is not here yet

Still owed, each in its own issue:

- #2275, placement: the dispatcher offering a job to a contractor and the
  supervisor waiting on it. Until then no job is offered, so a contractor that
  asks gets none.
- The image digest and pi-rukas version of a contractor's cycle as the job
  is given out. The job's row has no column for them, and adding one is a
  store schema change, which this release does not make.
- #2277, the counts in `tehas status`, the dashboard and the MCP server.
- #2278, the install of the proxy and the contractor program, their units,
  and the join installer.
- #2354, TLS and signed requests on the channel. Version 1 is plain http with
  bearer keys: bind the proxy's address on a network you control.
- Job tokens that outlive a restart of the proxy. They are held in its memory
  only, so a restart refuses every running job's check-in with 401 `no job`,
  and each such job takes the lost path and costs its issue a retry. Keeping
  a hash of each token in the store needs a store schema change, which this
  release does not make.
