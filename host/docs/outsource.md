---
title: Outsource
description: "Cycles run on contractors: the model proxy, the job channel, the contractor program and the commands that manage contractors."
---

# Outsource

A contractor is a machine that is not this host and does not hold the
factory's model key. Its cycle reaches the factory's model through one door:
`tehas-proxy`, a small HTTP task of its own that answers a job's model token and
forwards the ask to the endpoint the instance has configured. The real key
never leaves the factory, and a contractor is never told it.

Outsource is off until you switch it on. Off, `tehas-proxy` says the word that
says why and ends 0: no listener, no new port, nothing in `live/` and no new
line in `tehas status`.

In this release outsource is a feature still being built. It cannot be
switched on: `TEHAS_OUTSOURCE=on` stops every command with exit 78 and says
that `outsource` is still being built, and so does a line that gives a value
to any `TEHAS_OUTSOURCE_*` setting below. `tehas api --json` lists it in
`flags` as `false`. This page says what the proxy does once the feature
ships.

This page covers the model proxy, the job channel on the same listener, the
contractor program and the `tehas outsource` commands. What is still owed is
[at the end](#what-is-not-here-yet).

## What it is for

A cycle on a contractor holds its settings, a token for the product
repository, and the job's model token for the model proxy. It holds no endpoint, no provider,
no model and no key of its own. Pi reads one `models.json`, so the file the
contractor's container is given is written for that one job: every provider's
`baseUrl`, and a model's own where it has one, is the factory's proxy, and its
`apiKey` is the reference `${PI_ENSEMBLE_API_KEY_<PROVIDER>}`, the provider's
name upper-cased with every other character `_`. The job's model token is
that variable's value, so it is in no file. The factory's endpoints, backup
endpoints and key names are not in the file. The proxy holds the factory's
real key, reads it from its own environment, writes it into one request and
forgets it.

A contractor's sandbox image needs pi-rukas v0.14.86 or later: that release
reads `PI_ENSEMBLE_BASE_URL_<PROVIDER>` and `PI_ENSEMBLE_API_KEY_<PROVIDER>` as
a run's own endpoint and key and keeps the key out of every log and file. An
image that follows `release:0` today has it.

### A job's two tokens

A job has two tokens, made together when a contractor takes the job with the
channel's `job` call and sent to the contractor in that answer:

| Token | In the answer | Held by | What it may call |
| --- | --- | --- | --- |
| model token | `model_token` | the cycle's container, as the `apiKey` of its `models.json` | the model paths below, and the egress tunnel |
| job token | `token` | the contractor program, never the container | the job channel (`check-in`, `token`, `relay`, `result`, `stopped`), and the job's downloads (`index`, `bundle`, `attachments`) |

A token of one scope is refused on the other's paths with 403: the channel
answers `{"refused": "scope"}`, and a model path answers as the table below
says. A token the proxy never issued is 401. The image download comes before
a job and is made with the contractor's key, not with either token.

Both tokens have one lifetime. They die together when the job finishes, is
handed back, is lost, or its contractor is revoked. They are secrets of the
proxy's own: they live in its memory and nowhere else on the factory, never
in a command line, a log or the store. The store keeps the sha256 of each in
the job's row (`jobs.job_token_sha256` and `jobs.model_token_sha256`, schema
18), and the proxy gives neither token unless the store kept both hashes. A
proxy that restarts checks a token it does not hold against those hashes and
takes it back in its own scope only, so both tokens of a job that is running,
stopping, or lost to a silence of its current attempt still work, and a job
does not take the lost path for a restart. A token of a job that ended, of an
attempt a newer one replaced, or of a revoked contractor names no job.

A store brought forward from schema 17 keeps a job's one token as its job
token and gives it no model token. Outsource cannot be switched on in a
release, so no host has such a job.

## How it is switched on

`tehas-proxy` reads the settings `tehas_load` exports, and the state store under
`TEHAS_HOME`. Its control socket is `~/tehas/state/outsource/proxy.sock`. The
socket can also issue a job's model token by hand (`tehas-proxy --issue`),
written to `~/tehas/state/outsource/jobs/<job>/model-token` with mode 0600. It
never makes a job token. A model token it issues for a job the store does not
hold is kept in memory only.

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
`http://`, or its key unset, the proxy stops with 78 and says which. It stops
with 78 too, before it binds anything, when `TEHAS_HOME` is so long a path that
its control socket, `state/outsource/proxy.sock`, would be over the 107 bytes a
unix socket allows.

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
| The job token of a job, not its model token | 403 | `refused no job: a token of the job channel on a model path` |
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
a stream, are read of each answer but not kept yet: nothing reads them. The
proxy never guesses a
token count, and an answer it could not finish writing gives no figures.

## What it never does

- Never logs a body. A prompt holds source code. One line per request names the
  method, the path, the status, the time and whose job it was, and nothing of
  either body; a path is cut at 200 bytes and holds no line end.
- Never writes the factory's model key anywhere but one request, and never
  gives it to a contractor. The endpoint sees the factory's `Host` and its own
  key: the contractor's token and its own `Host` are dropped.
- Never holds a job's tokens in the store, a log or a command line. The store
  holds only their sha256.
- Never forwards a path outside the table, and never reads a body to decide
  anything beyond the one word of the ask that says whether the answer is a
  stream.
- Never queues a request over its bound of streams; one over it is ended and
  the cycle retries it as it retries a busy model.

## Off means off

Outsource not on: `tehas-proxy` says `outsource is off: no model proxy
is listening` and ends 0. Nothing binds a port, nothing is written under the
state, `live/contractors.json` is not there, `tehas status` and its `--json`
say no word about the feature, the MCP server has no `factory_contractors`,
and the rest of the suite is as it was.

## The job channel

The same listener answers a contractor's calls about its work, at
`/outsource/v1/<call>`, each a POST with a small JSON body and a JSON answer:

| Call | Made with | What the factory does |
| --- | --- | --- |
| `join` | the one-time code of `tehas outsource invite` | registers the host's name and the hash of its key; the code is spent |
| `hello` | the contractor's key | notes that the contractor is there, and keeps the report it carries |
| `job` | the contractor's key | takes the offer in its hand: the job moves to running and its two tokens are issued |
| `check-in` | the job token | notes that the job is alive, while it runs |
| `token` | the job token | answers a fresh cycle token, narrowed as a local cycle's is |
| `relay` | the job token | checks one issue write as the local relay does and makes it with the factory's token |
| `result` | the job token | keeps the cycle's result where its report reads it and finishes the job |
| `stopped` | the job token | the job is handed back; both its tokens are spent |

A key is sent as a bearer, with the contractor's name in
`x-tehas-contractor`. The store holds the sha256 of the key and of the code,
never either one.

Every call, the join included, is refused with 403 `not allowed` when
`TEHAS_OUTSOURCE_CONTRACTORS` does not list the contractor's name. A refused
join does not spend its code.

A `hello` carries the contractor's report: the id of its sandbox image
(`docker image inspect` of `PI_ENSEMBLE_IMAGE`), the pi-rukas version the image
is labelled with, its free memory in MiB and the free disk of its tehas home in
KiB. The store keeps the last one with the second it came, in place of the one
before. A field the contractor left out, or sent in a shape the store does not
take, is kept as none. Placement fits a cycle to a contractor only when its
report names this host's image id, the memory a cycle may use and
`TEHAS_CYCLE_MIN_FREE` of disk, and is no older than `TEHAS_OUTSOURCE_TIMEOUT`.
The answer to a `hello` names this host's image id as `image`, so a
contractor that has another one fetches it (see [The downloads](#the-downloads)).
It keeps a broken download in `state/outsource/images` and resumes it on the
next `hello`. A kept file the factory answers with 416, one fetched whole but
never loaded, is removed and fetched again from the start.

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

A job carries its issue, its attempt, the timeout, its two tokens, the
`models.json` with every provider behind the proxy, and a short list of
settings that shape a cycle: `TEHAS_REPO`, the three every load requires
(`TEHAS_CONTROL_ISSUE`, `TEHAS_ALLOWED_LABELERS`, `TEHAS_GITHUB_APP_ID`, the
App's public id and never its key), `TEHAS_BRANCH`, `TEHAS_DRIVER`,
`TEHAS_READ_ONLY`, the work, verify and build timeouts, `TEHAS_WORKSTREAMS`,
`TEHAS_RUKAS_WORKAROUNDS`, `TEHAS_OUTSOURCE`, and the models and providers of
the agents' roles. The list is part of the release, not a setting. No address,
path, key or token of the factory is on it. A job also names the factory's
account, the `uid` and `gid` its containers run as, which the contractor
hands the cycle's folders to.

## The egress tunnel

The same listener carries a job's connections to GitHub, the package
registries and the hosts the instance lists, at
`/outsource/v1/egress?to=<host>:<port>`. It is an HTTP/1.1 Upgrade, not a
CONNECT, so it works on the proxy's own port and behind a TLS terminator
that passes an Upgrade, as Caddy's reverse proxy does. A contractor asks
with a GET that carries `Connection: Upgrade`, `Upgrade: tehas-egress` and
the job's model token as a bearer. The answer is `101 Switching Protocols`,
and from then on the bytes are the tunnel's.

What it takes:

- The job's model token. The job token is refused with 403 `scope`, a token
  the proxy never issued with 401. The job must be in a state that a
  `result` is taken in: running, stopping, or lost to a silence while its
  attempt is current, so the salvage push of a stopped job still goes out.
  A job that ended and a revoked contractor are refused with one log line,
  and nothing is connected.
- A destination on the factory's list: the base list of `TEHAS_EGRESS=1`
  (GitHub and the npm and cargo registries, on 443) and
  `TEHAS_EGRESS_HOSTS`, by the same rule the in-house egress proxy uses.
  A model endpoint of `models.json` is refused even when it is listed: model
  traffic stays on the counted model route. No host alias is read.
- After the name resolves, every address must be public and not the
  factory's own, so the dashboard, the MCP server and the docs are out of
  reach. A loopback, private, shared, link-local or unique local address is
  refused.

A refusal is a JSON answer with a `refused` word:

| Status | Word | Why |
| --- | --- | --- |
| 400 | `an egress tunnel is a GET with Upgrade: tehas-egress` | no Upgrade asked |
| 400 | `no host:port to tunnel to` | `to` is missing or no `host:port` |
| 401 | `no job` | a token the proxy never issued |
| 403 | `scope` | the job token, not the model token |
| 403 | `revoked`, or the job's state | the job ended or its contractor was revoked |
| 403 | `not on the egress list` | the destination is not listed |
| 403 | `a model endpoint: use the model route` | the destination is a model endpoint |
| 403 | `an address that is not public` | the name resolved to a private address or to the factory |
| 502 | `the host could not be resolved`, `the host could not be reached` | the destination did not answer |
| 503 | `too many tunnels` | 128 tunnels are open over every job |

A tunnel ends when both sides have ended, or when neither has sent anything
for `TEHAS_OUTSOURCE_PROXY_IDLE` seconds. There is no cap on a single write
and nothing is buffered, so a large push or install passes. An open tunnel
does not hold one of the listener's 32 places, so downloads never crowd out
a check-in. The log has one line when a tunnel opens or is refused, naming
the job, the contractor and the destination, and one when it closes, with
its bytes each way. It never has a token or a byte of the tunnel.

## The downloads

What a contractor would otherwise build or fetch itself, it fetches from the
factory, on the same listener. Each is a GET under `/outsource/v1/`, answered
with the file:

| Path | Made with | What it is |
| --- | --- | --- |
| `image?id=<id>` | the contractor's key, as `hello` | the sandbox image `PI_ENSEMBLE_IMAGE` names, as `docker save` output, only when `<id>` is its id |
| `index` | the job token | the code index of `state/index`, when `state/index.json` names the image the factory runs now |
| `bundle` | the job token | a git bundle of `origin/<branch>` of the product checkout (`TEHAS_WORK_REPO`), that one ref and its history |
| `attachments` | the job token | the issue's images and files with their `manifest.json`, fetched and checked on the factory as an in-house start does |

The image comes before a job, so it is the contractor's, by its key and
name; a revoked or unlisted contractor gets nothing. The other three take the
job token of a running job, as `check-in` does, and serve that job alone. The
model token on them is 403 `scope`.

Each file is made on the first request that needs it and kept for the next:
an image per image id in `state/images`, created on the first image download,
and an index per tip and image, a bundle per tip and a job's files per job in
`state/outsource/downloads`. Concurrent requests make it once. A job's files
are made under a lock of that job's own, so a slow fetch for one job holds up
no other. When a new image is saved, the one before it stays for a contractor
still loading it and older ones are removed. A job's files go when the job
ends, is handed back, or its contractor is revoked. Those of a job that is no
longer running for another reason, such as one lost to a silence, go when
the next job's files are made. Files are compressed with zstd when `zstd` is on
the proxy's `PATH`, and served as a plain tar otherwise; the answer says which.

An answer names what it is in fields of its own: `x-tehas-sha256` of the file
and `x-tehas-pack` (`zstd`, `tar` or `bundle`) on every one, `x-tehas-image`
and `x-tehas-uid` (the uid the image was built for) on the image,
`x-tehas-tip` and `x-tehas-image` on the index, `x-tehas-tip` and
`x-tehas-branch` on the bundle. A `Range: bytes=<from>-` resumes a broken
download with 206. At most two image downloads run at once; a third waits.

| Status | Word | When |
| --- | --- | --- |
| 400 | `a download is a GET` | another method |
| 400 | `an image download names an image id` | no `id`, or no image id |
| 404 | `not the factory's image` | the id is not the image the factory runs |
| 404 | `no index` | no index is built |
| 404 | `an index of another image` | the index was built by another image |
| 404 | `no files given` | neither `TEHAS_WORK_IMAGES` nor `TEHAS_WORK_FILES` is `1` |
| 404 | `no downloads here` | the listener has no downloads |
| 409 | the fetch's own words | the issue's files could not be fetched or checked: an in-house start would stop on it |
| 416 | `a range of no bytes` | a range past the end of the file |

The issue's files are made with the dispatcher's latest record of the issue
and the instance's own settings (`TEHAS_WORK_IMAGES`, `TEHAS_WORK_FILES`,
`TEHAS_WORK_COMMENTS`), by the same fetch and link rule an in-house start
uses. An issue with no recorded approval gets none, and the job does not
start.

The log has one line per download, naming the call, the status, the job or
contractor and the bytes sent. It never has a token or a key.

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

- **The start comment** on the issue, of a cycle or of a refresh, ends with
  a line: `Built by contractor: <name> (job <id>)` for a cycle placed on a
  contractor, `Built by: in-house` for one run on this host. The contractor
  is the one holding the issue's job, offered or running, given out since the
  cycle's approval. A retry after a lost job is not that job's.
- **The report's comments** at the end of the cycle end with the same line.
- **The pull request's body** gets the line as its last line when the cycle
  ends. An older stamp line in the body is replaced, so the line is there once.
- **Commits.** Each commit a contractor's cycle adds to its pull request's
  branch carries a `Built-by: <name>` trailer. Before the cycle is handed to
  the contractor, the factory notes the heads of the repository's branches in
  `state/cycles/<issue>.heads`. When the cycle ends it reads the branch into a
  repository of its own and checks the commits over the head the branch had
  then, or over the base branch for a new branch. Commits an earlier cycle or
  a person pushed are left as they are. A commit whose trailer is missing or
  names someone else is written again with the right one, and so are the
  commits after it. Each keeps its tree, author and committer. The branch is
  pushed with a lease on the commit it was read at, so work pushed since is
  never overwritten. A pull request that is merged or closed, one from a
  fork, the end of a cycle after a stop, and a cycle whose heads could not be
  noted are not rewritten, and a read-only instance only says what it would
  push.
- **The store** keeps who built it in the job's row: `store job-stamp --issue
  <n> --since <approval>` answers the job, its contractor, attempt and state,
  or null for a cycle run in-house. With `--open` it answers only a job
  offered or running, which is what a start asks.
- **The analytics** keep the contractor's name in `runs.contractor_id`
  ([the analytics](/analytics)). An in-house run leaves it empty.

When the store cannot say which job a contractor's cycle was, or the
cycle's approval has no time to ask it with, the comments and the pull
request carry no stamp and the log says so: a guess would be wrong, and a
contractor's cycle is never stamped in-house.

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

## Where a person and a program see the contractors

With outsource on, each run of `tehasbot status` (every poll, and the end of a
cycle) writes `~/tehas/live/contractors.json` from the state store. With it
off, the same run takes the file away, so a factory with the feature off has
none. The file holds, at `time`, the counts by status and how many cycles are
outsourced (`counts`), and per contractor its `name`, `status` (the words of
`tehas outsource list`), `issue` (the issue of the job in its hand, left out
when it holds none), `lastSeenAt`, `jobsDone` and `jobsLost`, and from its
last report `image`, `rukas`, `memMb`, `diskKb` and `reportedAt`, each left out
when it did not say. No address, no
key and no host name but the contractor's own. The docs server does not serve
it under `/live/` yet: the dashboard's Contractors panel, which reads it, is
still to come.

Three readers show it, and each only while the file is there:

- `tehas status` prints an `outsource:` line after the cycles, the same line
  as `tehas outsource status`, and each running cycle ends with
  `outsourced to <contractor>` or `in-house`:

  ```
  cycle     #15 running, step develop, started 2 min ago, outsourced to box1
  cycle     #12 running, step review, started 40 min ago, in-house
  outsource: 1 idle, 1 working, 1 resting; 1 cycle(s) outsourced
  ```

- `tehas status --json` has `contractors`: the counts, and the contractors as
  `list`. See [tehas status --json](/api#tehas-status-json).
  `running` and `room` count an outsourced cycle as they count one on this
  host: this host's supervisor holds the cycle's lock while the contractor
  runs it.
- The MCP server's `factory_contractors` answers the file, read only. A
  dashboard key is served it too. With outsource off the tool is not listed
  and a call of it is an unknown tool. See [the MCP server](/mcp-server#the-tools).

## The contractor program

`tehas-contractor` runs on the contractor's host. Its help and every line it
logs start with `tehas outsource contractor`.

```
tehas-contractor join <factory> <name> <code>    join with the code tehas outsource invite printed
tehas-contractor run                             take jobs and run them until drained or revoked
tehas-contractor drain                           take no new job; hand a running one back after the grace
tehas-contractor doctor                          say what this host holds that it should not
tehas-contractor own --as <uid:gid> <folder>...  hand the cycle's folders to the factory's account
```

At rest it holds its name, its key and the factory's address, in
`~/.config/tehas/contractor.json` (mode 0600), and nothing else. A job
leaves the job's two tokens, its settings, the cycle token, a `models.json` whose
every provider is the factory's proxy, the product, the factory's code index
and the issue's files. All of it is wiped when the job ends and when the
program starts. The `models.json`
names the factory at the address this host joined with, whatever address the
proxy binds.

Before its container starts, the cycle asks the program (`own`) to hand the
folders the container writes to the account the job names: the workspace,
the code index cache, the relay folder, and the cycle's memory, sessions and
agent runs. They must be owned by the account the container runs as, not
only writable by it: the code index refuses a cache folder it does not own,
whatever its mode. So the cycle names its container's `--user` to the
program, and the folders are handed over only when that is the job's
account. Today the container runs as the account the program runs as, so a
contractor that runs as another account than the factory's keeps the
folders with its own account and the cycle runs as before; running the
container as the factory's account is a later step. Only folders under
`$TEHAS_HOME/work` and `~/.pi/agent/ensemble-runs` are handed over, a link
is changed itself and never followed, and a folder outside the cycle is
refused, and the cycle does not start, before anything is changed. A file
the program cannot hand over also stops the cycle, and what it handed over
before that stays handed over. A job that names no account leaves the
folders as they are. The container is given no mount, variable or token
more for it.

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

Before a cycle starts, the program fetches what the job needs from the
factory's downloads, and nothing of it from GitHub:

- The product, from the bundle: `repos/<name>` gets `origin/<branch>` at the
  factory's tip, and the repository's address as `origin`, so the cycle's
  pushes go where they do in-house. No bundle, no job: it is handed back.
- The code index, into `state/index`, where `tehas work` copies it into the
  cycle's cache. No index, or one of another image, is no error: the log says
  `this cycle searches without a code index` and the cycle starts.
- The issue's files, into `work/<name>-<issue>.images`. None given is no
  error; a refusal of the factory's fetch is, and the job is handed back.

When the factory's `hello` answer names an image this host does not have under
`PI_ENSEMBLE_IMAGE`, the program fetches it, resuming a broken download, loads
it with `docker load` and checks its id before it asks for a job.

The program's own git, the salvage push, goes through the egress tunnel: for each job it runs a CONNECT proxy on
127.0.0.1 that opens every tunnel on the factory's listener with the job's
model token, and gives it to git as `http.proxy`. No proxy variable of the
user's and no `NO_PROXY` takes git around it. A host the factory refuses is
answered to git with the factory's status and word.

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
  is given out, on the stamp of its work.
- The dashboard's Contractors panel, and serving `contractors.json` under
  `/live/` for it.
- The endpoint's token figures per contractor and per job, kept in the store.
  Nothing reads them yet.
- #2278, the install of the proxy and the contractor program, their units,
  and the join installer.
- #2379, the container's own way out: the internal network and the
  contractor's local end for the cycle's container, so its git and
  registries go through the egress tunnel too. Until then only the
  contractor program's own git does.
- #2365, the container's side of the code index: the workspace mounted at
  the factory's own path, which the index answers at. Until then the index a
  contractor is given is copied into the cycle but finds nothing.
- `tehas work` giving the issue's files to a contractor's container: the
  program puts them where an in-house cycle has them, and a contractor's
  `tehas work` does not mount them yet.
- The image of a job kept while a job names it. A job does not name its
  image in the store yet, so the factory keeps the image before the current
  one and no older.
- #2354, TLS and signed requests on the channel. The image download takes the
  contractor's key as `hello` does until requests are signed. Version 1 is plain http with
  bearer keys: bind the proxy's address on a network you control.
