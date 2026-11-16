---
title: Outsource, the model proxy
description: "The factory's model proxy: what a contractor's cycle may ask it for, how it is switched on, and what it never does."
---

# Outsource, the model proxy

A contractor is a machine that is not this host and does not hold the
factory's model key. Its cycle reaches the factory's model through one door:
`tehas-proxy`, a small HTTP task of its own that answers a job's token and
forwards the ask to the endpoint the instance has configured. The real key
never leaves the factory, and a contractor is never told it.

Outsource is off until you switch it on. Off, `tehas-proxy` says the word that
says why and ends 0: no listener, no new port, nothing in `live/` and no new
line in `tehas status`.

The rest of [outsource](#what-is-not-here-yet) is not built yet: the
contractor's program and the job channel. The state store holds the
contractors and the jobs. This page is the factory's half, the proxy, and what
it will not do.

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

The job token is a secret of the proxy's own. It lives in the proxy's memory,
is written once to a file of mode 0600 in the job's directory under the state,
and dies with the job and with revocation. It is never in a command line, never
in a log and never in the store.

## How it is switched on

`tehas-proxy` reads the settings `tehas_load` exports, and the state store under
`TEHAS_HOME`. Its control socket is `~/tehas/state/outsource/proxy.sock`; a
job's token is written to `~/tehas/state/outsource/jobs/<job>/job-token`.

| Setting | Default | What it is |
| --- | --- | --- |
| `TEHAS_OUTSOURCE` | `0` | `1` runs the proxy. Anything else stops every command with 78 |
| `TEHAS_OUTSOURCE_PROXY_ADDR` | `127.0.0.1:8090` | The address and port to listen on. Never the dashboard's `TEHAS_HOST_ADDR` port |
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

The endpoint's own figures, `usage` of an answer or of the last chunk of a
stream, and the requests each token carried are counted per contractor and per
job. `tehas-proxy --counts` prints the request counts and clears them. They
reach the state store, `tehas status` and the dashboard with #2277. A
proxy that could not finish writing an answer counts nothing: an answer no one
read is no answer to price. The proxy never guesses a token count, and a row
names no token, no path and no byte of a body.

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

`TEHAS_OUTSOURCE` not `1`: `tehas-proxy` says `outsource is off: no model proxy
is listening` and ends 0. Nothing binds a port, nothing is written under the
state, `tehas status` says no word about the feature, and the rest of the suite
is as it was.

## What is not here yet

The proxy is the first part of [outsource](/features). Still owed, each in its
own issue:

- #2274, the contractor's program, the join handshake and TLS, and giving the
  contractor's container the `models.json` whose `baseUrl` is the proxy. Until
  that lands, no container is given the file and no contractor calls the proxy.
- #2275 and #2276, placement in the dispatcher and the stamp on a result.
- #2277, the counts in `tehas status`, the dashboard and the MCP server.
- #2278, the install and the join installer.

Until #2274 issues tokens for real jobs, no contractor holds one, so the proxy
answers every request with a refusal and forwards nothing to the endpoint.
