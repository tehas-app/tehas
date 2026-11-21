# Spec: plugins v1, the security model

**Status**: Draft 2026-11-21. Part of `2026-11-21-plugins.md`, which holds the
terms, settings and manifest. Nothing is built.

A plugin is someone else's code, possibly closed, on the factory's host. It
starts with nothing. A person grants each capability by name, for one commit.
Everything here is enforced in one place, named in "Enforced in one place",
and pinned by a test.

## Capabilities

| Capability | What it gives | What a person should weigh |
| --- | --- | --- |
| `view` | `/out` of a good refresh is served behind the sign-in, and a panel in the dock | Everyone the sign-in lets in sees it. A page can still navigate away with data in its address (web part) |
| `product` | `/product`, read only, in every run: the default branch's head in a refresh, the pull request's head in a hook | The plugin reads all the code. With `view`, what it writes is shown; with `model`, the code goes to the model |
| `model:<tokens>` | `/run/tehas/model.sock`: chat completions for one model, at most `<tokens>` a run | Model time, and where the model runs: `review` says whether the endpoint is on this host or another |
| `action:<id>` | The view's button `<id>`: a signed-in person asks for a refresh | A refresh's cost, at most one at a time |
| `hook:after:work` | A run on each pull request a cycle leaves, reported as a commit status | Its summary, 140 characters, is on the pull request for every reader of the repository; its findings are behind the sign-in |

`kit` is no capability: the kit is tehas's own files, served behind the
view's sign-in.

A grant is for the commit its line names. A new commit grants nothing until
its line is written again, so a new pin cannot change what an id means and
keep the old grant.

What no grant gives, in any version: a GitHub token or the App's key, the
Docker socket, an SSH key or agent, anything of `~/.config/tehas` or `~/tehas`
beyond the mounts below, the host's network, a write to the product, GitHub,
the state store or another plugin's files, `--privileged`, a device, a host
path.

## The container

Every run is one container. Its arguments are a pure function of the
manifest, the grants, the run's kind and the instance's limits, in
`console/core/src/sandbox.rs` beside the cycle's. Nothing else adds an
argument.

```
docker run --init --name tehas-plugin-<name>-<run>
  --cidfile <run dir>/cid --pull=never
  --user 10001:10001 --read-only
  --cap-drop ALL --security-opt no-new-privileges
  --network none
  --cgroup-parent tehas.slice
  --memory <m>g --memory-swap <m>g --cpus <n> --pids-limit 512
  --log-driver json-file --log-opt max-size=10m --log-opt max-file=1
  --tmpfs /tmp:size=256m,noexec,nosuid,nodev
  --mount type=volume,src=tehas-plugin-<name>-work-<run>,dst=/work
  --mount type=volume,src=tehas-plugin-<name>-out-<run>,dst=/out
  --mount type=volume,src=tehas-plugin-<name>-state,dst=/state[,readonly in a hook]
  [--mount type=bind,src=<product dir>,dst=/product,readonly]       with product
  [--mount type=bind,src=<socket dir>,dst=/run/tehas,readonly]      with model
  --mount type=bind,src=<run dir>/context.json,dst=/run/context.json,readonly
  <image id>
```

- **By image id**, never by tag.
- **No `--rm`**: tehas reads `/out` from the stopped container, then removes
  it with `docker rm`, and removes a container left by a run that died on the
  next tick.
- **`--network none`**, always. The model is a unix socket, not a network.
- **No host path but three**, each read only and made for the run: the product
  export, the socket's directory, the context file.
- **Disk.** `/work`, `/out` and `/state` are Docker volumes, so the host's
  files are not reachable through them and the plugin's uid owns them. After
  each run tehas measures them with `docker system df -v`. A run whose volumes
  pass `disk` from `TEHAS_PLUGIN_LIMITS` failed, and its `/work` and `/out` are
  removed; a `/state` over it is emptied and the plugin is refused until a
  person runs `tehas plugin run`. `/work` and `/out` are removed after every
  run. `tehas disk` counts the volumes and images; `tehasbot clean` removes
  what no plugin in the settings owns.
- **Time.** At `timeout` tehas runs `docker kill` on the id in `cidfile`; a
  killed client is not enough.
- **The context**, `/run/context.json`: the run's kind and trigger, the
  plugin's commit, the head commit of `/product`, and in a hook the pull
  request's number, base and head. Nothing of a person.

Test: `test/fixtures/argv/plugin-*` pins the arguments for no grant, for each
capability alone, and for the reference set, and `test/sandbox_argv_test.sh`
compares them, as for the cycle's lists. A change there is a security change
(hard rule 4).

## The build

The image is built at `tehas update`, after the fetch, for each plugin whose
pin changed or whose image is gone.

- **The fetch.** `git fetch` of the pinned commit into
  `~/tehas/plugins/<name>/repo.git`, with `-c protocol.allow=never
  -c protocol.https.allow=always -c http.followRedirects=false`, no
  submodules, no LFS, as the host's user. A private repository works only if
  the factory's GitHub App is installed on it, since the host's git helper
  gives the App's token for github.com.
- **The context** is `git archive <commit>`, piped to `docker build -`. No
  `.git`, no file outside the commit.
- **The Dockerfile's rules**, read by tehas with BuildKit's own grammar
  (continuations, heredocs, `ARG`) before the build:
  - every `FROM` names an image by `@sha256:` digest, with no `ARG` in it;
  - no `# syntax=` line;
  - `COPY --from` and `RUN --mount=from=` name a stage of the file, never an
    image;
  - no `ADD` of a URL or a git address.
  A Dockerfile that breaks one is refused, naming the line.
- **The base images** are pulled by digest before the build, with the host's
  Docker, and the digests are shown by `tehas plugin review`.
- **The build itself** is `timeout 1800 docker build --network none
  --cgroup-parent tehas.slice --memory 4g --no-cache --pull=false
  -t tehas-plugin-<name>:<commit> -`, as the preview build is
  `--network none`. A plugin that needs packages carries them in its
  repository.

So the code that runs is the commit and the named base images, and nothing a
`RUN` fetched. Two hosts that build one commit get the same source and bases,
not always the same bytes; `tehas plugin --json` shows the commit, the
manifest's sha256 and the base digests, and the local image id as a local
fact.

## Reading `/out` and `/state`

Nothing a run left is opened as a path on the host.

- **`/out`** is read as a tar stream from `docker cp <container>:/out -`
  before the container is removed. tehas's reader in Rust takes regular files
  and directories only: no symlink, hard link, device, fifo or socket, no
  absolute name or `..`, at most 5000 entries and 256 MiB. Anything else fails
  the run and leaves the last good view.
- A good refresh's files are written into a new directory
  `~/tehas/plugins/<name>/view-<run>/` as the host's user, with names checked
  again, and swapped into `view/` with one rename. The old one is removed.
- **A hook's verdict**, `/out/_tehas/verdict.json`, comes through the same
  reader, at most 64 KiB.
- **`/state`** stays a volume the host never opens. In a hook it is mounted
  read only, so a pull request's code cannot plant anything in what the next
  refresh, or any view, reads.

## The product

`product` is a git repository made for the run, never the product's mirror
or a cycle's workspace.

- tehas clones the mirror's commit with `git clone --no-local
  --no-checkout --depth 200 --single-branch`, with `core.hooksPath=/dev/null`,
  no filters and no `GIT_*` from the host, removes the remote, and checks out
  the commit. A hook fetches the pull request's head as GitHub names it into
  the mirror first, as `tehas-cycle finish` does.
- It is owned by the host's user and mounted read only. The container is
  given `GIT_CONFIG_COUNT=1`, `GIT_CONFIG_KEY_0=safe.directory`,
  `GIT_CONFIG_VALUE_0=/product`, so git in the container reads it.
- It is removed after the run.

## The model socket

A plugin never holds the model's key, never reaches the endpoint, and has no
network. With `model:<tokens>` granted, `tehas-plugin` serves a unix socket for
the run, `/run/tehas/model.sock`, and stops it when the run ends. It is a part
of the `tehas-plugin` program, not of `tehas-proxy`: it shares no token, port
or route with outsource.

What it answers:

- `POST /v1/chat/completions` and `GET /v1/models`. Anything else: 404.
- The body is read whole, at most 1 MiB, and parsed. Only these fields pass:
  `model`, `messages`, `max_tokens`, `temperature`, `top_p`, `stop`, `tools`,
  `tool_choice`, `response_format`, `seed`. Any other field: 400.
- `model` must be the product model of `models.json`; it is rewritten to the
  endpoint's id.
- A message's content is text: a string, or parts of `type: text` only. No
  image or file part, so the model is never asked to fetch an address.
- `stream` and `n` other than 1: 400. v1 answers whole.
- `max_tokens` is required, at most 16384.
- **The budget is held before the request.** The prompt's bytes divided by 3,
  plus `max_tokens`, is reserved against the run's `<tokens>` and the day's
  `TEHAS_PLUGIN_MODEL_DAY` before it is sent. The answer's `usage` then
  replaces the reserve; an answer without `usage` keeps it. A request that
  would pass either total is 429 and is not sent.
- One request at a time per run. A second waits.
- The request goes to the endpoint and key of `models.json`'s provider, over
  `http://` or `https://` (through `curl`, as `models.rs` reaches the
  endpoint), with a 300-second bound. The answer is passed back as JSON, at
  most 4 MiB.
- Counts, not bodies, are kept: requests and tokens on the run's row.

The cycles call the model directly, so a plugin and a cycle share the
endpoint's capacity. v1 bounds a plugin by its budget, one request at a time,
and refreshes that wait for the factory (core, "Runs"). A queue of its own in
front of the endpoint is v2.

What the model writes is untrusted: it read the code. It reaches only the
plugin's own `/out` and `/state`, and through a hook, text tehas checks and
posts as quoted text.

## Granting

- `tehas plugin review <name>` prints, for a person: each ask, what it lets
  the plugin do, the Dockerfile's base images, whether the model endpoint is
  on this host, what changed since the pinned commit, and the line to write.
- `tehas update` checks each plugin: its pin fetched, its manifest read, every
  ask granted for that commit, nothing reserved asked for, its Dockerfile's
  rules, its build. A plugin that fails any of them is refused and skipped
  (core, guarantee 4).
- A grant the manifest does not ask for gives nothing, and `review` says so.
- A grant taken away applies at the next `tehas update`.

## Enforced in one place

| Rule | Where | Test |
| --- | --- | --- |
| The container's arguments | `sandbox.rs`, a pure function | `test/fixtures/argv/plugin-*` |
| Settings and grants | `settings/rules.rs` | its tests |
| The manifest | `console/core/src/plugin/manifest.rs` | its tests and `api/` samples |
| The Dockerfile's rules | `console/core/src/plugin/dockerfile.rs` | a case per rule |
| The tar reader | `console/core/src/plugin/out.rs` | a case per kind of entry |
| The model socket | `console/plugin/src/model.rs` | against a stub endpoint |
