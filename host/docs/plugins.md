---
title: Plugins
description: "Programs from pinned git repositories that add a view and a check to a factory: how to set one up and run it, what each capability lets it do, and what no grant gives."
---

# Plugins

A plugin is a program from a git repository, pinned by commit, that adds a
view to the factory's dashboard and may report on the pull requests a cycle
leaves. It runs on the factory's host, in a container of its own, outside
every cycle. It gets nothing a person did not grant, by name, for that
commit.

## Not in a release yet

`plugins` is a feature still being built. A release refuses
`TEHAS_PLUGIN=on`, and any line of the other `TEHAS_PLUGIN*` settings, with
exit 78. `tehas plugin` exits 78 too, except `tehas plugin check`. No release
installs the runner, the gate or their units, and no plugin is fetched, built
or run on a host. [Features](/features) says how far each part is built and
what has been seen working.

What this means for you now:

- There is nothing to turn on. Leave every `TEHAS_PLUGIN*` line out of
  `instance.env`.
- A plugin's author can check a folder on any machine with
  `tehas plugin check <dir>` ([tehas plugin](/tools-control#tehas-plugin)).
- You can try the reference plugin's driver by hand, outside tehas:
  [The reference plugin](#the-reference-plugin-floorplan).
- The GitHub App and edge changes in [Before you turn it
  on](#before-you-turn-it-on) can be made ahead.

The rest of this page says how a host works with the feature on, as the code
of this release does it. The design is the plugin specification,
[docs/specs/2026-11-21-plugins-1.0.0.md](https://github.com/tehas-app/tehas/blob/main/docs/specs/2026-11-21-plugins-1.0.0.md),
at revision 1.3.4. This release builds part of its v1.

## Setting one up

A plugin takes two lines of `instance.env`, its pin and its grants, and
`TEHAS_PLUGIN=on`. For the reference plugin:

```
TEHAS_PLUGIN=on
TEHAS_PLUGINS="floorplan=https://github.com/tehas-app/tehas-plugin-floorplan@<commit>"
TEHAS_PLUGIN_GRANTS_FLOORPLAN="<commit> view product model:15000000 action:refresh hook:after:work"
TEHAS_PLUGIN_ACCESS=write
TEHAS_PLUGIN_MODEL_DAY=20000000
```

`<commit>` is the whole commit, 40 lower-case hex digits, the same in both
lines. [The settings table](/tools#instance-settings) has every rule of
each setting.

1. **Pin it.** Add `<name>=<url>@<commit>` to `TEHAS_PLUGINS`: at most four
   plugins, each an `https://` address. A private repository works only when
   the factory's GitHub App is installed on it.
2. **Read what it asks.** `tehas plugin review <name>` fetches the pin and
   prints each ask with the line of [the capability
   table](#what-each-capability-gives), the Dockerfile's base images, whether
   the model runs on this host or another, what changed since the pin and the
   grants line to write. It writes nothing but the fetched objects and the
   base images it pulls.
3. **Grant it.** Write `TEHAS_PLUGIN_GRANTS_<NAME>`, the name in upper case:
   the commit, then the grants. A grant the manifest does not ask for gives
   nothing. An ask that is not granted refuses the plugin.
4. **Say who sees the view.** `view` and `action:` need `TEHAS_DOMAIN` and
   `TEHAS_PLUGIN_ACCESS`, the least permission on `TEHAS_REPO` that lets a
   person in: `read`, `triage`, `write`, `maintain` or `admin`. On a public
   repository every GitHub account has `read`, so the gate lets no one in
   with `read` or `triage` while the repository is public.
5. **Bound it.** `TEHAS_PLUGIN_LIMITS` is the most one run may have, by
   default `memory=2 cpus=2 disk=4` (GiB, cpus, GiB). `model:<tokens>` is
   the most one run may spend, and `TEHAS_PLUGIN_MODEL_DAY` the most all
   plugins spend in a day, by default 5000000.
6. **Build it.** `tehas update` fetches the pin, checks the manifest, the
   skills and the Dockerfile, pulls the base images by digest and builds the
   image. A plugin that breaks a rule is refused and left off; the rest of the
   update goes on, and it exits 1 at the end. `tehas plugin` shows each
   plugin's state: `unbuilt`, `building`, `ready`, or `refused` with why.

A new commit grants nothing until both lines name it again. Run `tehas
plugin review <name> <commit>` on it first: it says what changed since the
pin. A grant taken away applies at the next `tehas update`. A plugin taken
out of `TEHAS_PLUGINS` loses its images, its view, its panel and its
directory at the next update; its rows in the state store stay.

### What the model costs

A first map is expensive. The reference driver's first map of an 846-file
repository, with the factory's model (qwen3.8-flash-next), took about 10M
tokens, 499 model requests and 78 minutes. That is why the example grants
`model:15000000` a run and raises `TEHAS_PLUGIN_MODEL_DAY` to 20000000: the
default of 5000000 a day stops a first map of a repository that size. A
later refresh reads only the files that changed and should cost far less;
that is not measured yet.

The plugins share the model's endpoint with the cycles. A run sends one
request at a time, so a plugin slows the cycles down but does not take their
slots.

## What a run gets

Every run, whatever is granted:

- One container of the plugin's image, by image id, as uid 10001, with a
  read-only root, every capability dropped and no new privileges.
- No network at all (`--network none`).
- Memory and cpus bounded by `TEHAS_PLUGIN_LIMITS`, at most 512
  processes, in `tehas.slice`, so a run counts in the room cycles are admitted by
  ([Limits that hold work back](/operating#limits-that-hold-work-back)).
- `/work` and `/out`, new for the run, and `/state`, which a refresh keeps
  for the next one and a hook reads only. All three are Docker volumes. Past
  `disk` the run fails; a `/state` past it is emptied and the plugin waits
  for `tehas plugin run`.
- `/run/context.json`: the kind of run, the plugin's commit, and what its
  grants make of it. Nothing about a person.
- `docker kill` at the manifest's timeout.

No grant gives, in any version: a GitHub token or the App's key, the Docker
socket, an SSH key or agent, anything of `~/.config/tehas` or of `~/tehas`
past its own mounts, the host's network, a write to the product, GitHub, the
state store or another plugin's files, `--privileged`, a device or a host
path. A plugin never runs in a cycle's container, and a cycle's container is
given nothing new for it.

What a run leaves is read as a tar stream, never as a path on the host:
regular files and directories only, at most 5000 entries and 256 MiB. A link
or anything else fails the run and keeps the last good view.

### What each capability gives

`tehas plugin review <name>` prints, for each capability a plugin asks for,
the line of this table. Read it before you write the grant.

| Capability | What it lets the plugin do |
| --- | --- |
| `view` | Its /out is served behind the sign-in, and a panel in the dock: everyone the sign-in lets in sees it |
| `view:<id>` | One of its views, the part of its /out at that view's path, is served behind the sign-in and opens from the plugins menu: everyone the sign-in lets in sees it |
| `menu` | Its granted views are listed in the plugins menu of both front pages, by its name and their labels, in place of a panel in the dock: names and labels are public, as live/ is |
| `product` | Reads all of the product's code, read only, in every run |
| `model:<tokens>` | Chat completions through a socket, up to the tokens of the grant a run |
| `model:images` | Images in its model requests, each carried in the request and never fetched: what it sends of the product's pages goes to the model |
| `action:<id>` | A button in the view: a signed-in person asks for a refresh |
| `hook:after:work` | A run on each pull request a cycle leaves; its summary is on the pull request for every reader of the repository |
| `skill:<id>` | Its text from the pinned commit goes into the chats' instructions |
| `needs:<name>` | Runs only while the named plugin is ready, and its chat gets the skills that plugin grants to a plugin's chat; it grants the other plugin nothing |
| `tool:<id>` | The chats it names may call it: each call is a run with the caller's arguments and the plugin's /state read only, and its answer goes to the model |

A grant of anything not in this table is refused when the settings load.

What to weigh:

- **`product` with `view`.** The view shows what the plugin made of the
  code. A page cannot load anything from another site, but it can still
  navigate away with data in its address when a signed-in person opens it.
  `review` warns of this pair.
- **`product` with `model`.** The code goes to the model. `review` says
  whether that model runs on this host or another.
- **`model:<tokens>`.** Model time, shared with the cycles. See [What the
  model costs](#what-the-model-costs).
- **`action:<id>`.** Anyone the sign-in lets in can start a refresh, one at
  a time.
- **`hook:after:work`.** The status's summary, 140 characters, is public to
  every reader of the repository. The findings stay behind the sign-in.

### The build

The image is built at `tehas update` from the pinned commit and nothing
else:

- The fetch is over https only, with no submodules and no LFS.
- The build context is `git archive` of the commit: no `.git`, no file
  outside it.
- Every `FROM` names an image by `@sha256:` digest, except an earlier stage
  and `scratch`. No `# syntax=` line, no `ADD` of a URL, and `COPY --from`
  names a stage of the file. A line that breaks a rule refuses the plugin and
  is named.
- A base image with an `ONBUILD` trigger is refused: its steps would run
  outside these rules.
- The build runs with BuildKit and `--network none`. A plugin that needs
  packages carries them in its repository.

### The view and the sign-in

A view is served only by the gate, `tehas-plugin-gate`, at
`plugin-<name>.<domain>`, and only to a person who signed in with GitHub
and has at least `TEHAS_PLUGIN_ACCESS` on `TEHAS_REPO`. The sign-in goes
through the factory's GitHub App at `plugins.<domain>`. The gate checks the
person's permission again every 10 minutes and on every press, by user id,
and keeps no GitHub token. A session ends after 2 hours unused or 12 in all,
and a restart of the gate signs everyone out. The gate reads its settings
when it starts, so `tehas update` restarts a running gate: a changed
`TEHAS_PLUGIN_ACCESS`, pin or grant applies, and everyone signed in signs in
again.

Every page gets a policy that lets it load only the view's own files and the
inline blocks tehas saw when the view was swapped in. A page that names
another site loads nothing from it. No address carries a secret: the one
value in an address is the sign-in's code, which lives 60 seconds and works
once. A request from a preview or another site cannot press an action.

### The model

A plugin never holds the model's key and never reaches the endpoint. With
`model:<tokens>` granted, a refresh gets a unix socket made for that run,
`/run/tehas/model.sock`, which answers chat completions for one model id.
Each request's tokens are held against the run's grant and the day's total
before it is sent, and a request that would pass either is refused unsent.
The socket checks the request's fields, passes it to the main endpoint of
the product model's provider in `models.json`, and keeps counts, not bodies.
A hook gets no model.

## Running

A refresh makes the view. A hook reports on a pull request.

- **When the default branch moves.** `tehas-plugin.timer` runs `tehasbot
  plugin tick` every 10 minutes. It asks for a refresh of each ready plugin
  whose last refresh began at another head, from the head the poll already
  fetched. It starts nothing while the factory is paused, drained, read-only,
  or while `tehas update` runs. A failed refresh is not tried again at the
  same head.
- **On demand.** `tehas plugin run <name>`, or a press of the view's button.
  One refresh of a plugin runs at a time.
- **After a cycle.** A cycle that leaves an open pull request of the bot's
  asks each ready plugin granted `hook:after:work` for a hook on it, and
  waits for none. A hook holds no slot, lock or label. By hand: `tehas plugin
  hook <name> <pr>`.
- **What it said.** `tehas plugin log <name>` prints the last refresh's log,
  and `--hook <pr>` the last hook's on that pull request. Each control
  character but newline and tab is printed as U+FFFD, so a plugin's output
  sends no escape sequence to your terminal.

The runs, the tick and why it did nothing are logged in
`~/tehas/logs/plugins.log`. [tehas plugin](/tools-control#tehas-plugin) has
the units, the exit statuses and the order hooks run in.

### What a hook posts

One commit status on the head the hook read, and nothing else: never a
comment.

| | |
| --- | --- |
| Context | `tehas/plugin/<name>` |
| State | `success` for a `pass` or `note` verdict; `error` when the hook failed, ran out of time or left a verdict that breaks its schema |
| Description | The verdict's summary, cleaned (check boxes, markers and `@` mentions disarmed, secret-looking text masked) and cut at 140 characters |
| Link | With `view` granted, `https://plugin-<name>.<domain>/_hooks/<pr>`: the findings, behind the sign-in |

A hook never fails a status, so a plugin's status made a required check
fails only on `error`. A head that moved while the hook ran gets the status
on the commit the hook read, then a new hook on the new head. When GitHub
refuses the status, `tehas plugin` says "the status could not be written"
until one is written. The tick removes a hook's findings when its head is in
the default branch, or after 30 days.

### The gate and the panel

`tehas-plugin-gate.service` runs the gate, `tehasbot plugin-gate`. It listens
on `TEHAS_PLUGIN_GATE_ADDR`, by default `TEHAS_HOST_ADDR`'s address on port
8095, where the edge router reaches it. It ends with 78, and is not started
again, when a setting or the App's client files are missing.
[The gate of the views](/tools-control#the-gate-of-the-views) has the rest.

`tehas update` docks each ready view as the panel `p-<name>`, labelled from
the manifest, and takes it away when the plugin goes or is refused. A view
takes one of the dashboard's four panels; one that would be a fifth is left
out of the dock and the update says so. `tehas panel set` refuses an id that
starts `p-`.

### The plugins menu

A plugin granted `menu` takes no panel in the dock. `tehas update` lists it
in `live/plugins.json` instead: its name, its label, and each of its views it
is granted, with the view's label, its address and whether it opens in the
dock or a new tab. Both front pages read the file and show each plugin's
label in the menu, after the person's panels, with its views as links under
it. `live/` is public, so a plugin's name and labels are, as a panel's are;
what a view shows is behind the sign-in. A plugin without `menu` keeps the
one panel of its `view`, as before.

## Before you turn it on

These are the adopter actions for plugins. Each can be made before a
release ships the feature.

**The GitHub App.**

- Give the App "Commit statuses: Read and write" (its settings, Permissions
  and events, Repository permissions), then accept the new permission on the
  installation. Without it a hook runs but its status is not written. An App
  made with `tehas setup --new-app` from this release asks for it.
- The sign-in needs the App's callback URL and a client secret. `tehas setup
  --new-app` sets both when `instance.env` has `TEHAS_DOMAIN`. For an App
  made before, add the callback URL `https://plugins.<domain>/_auth/callback`,
  generate a client secret, copy it into a file, and run:

  ```bash
  tehas setup --app-client-id <Client ID> --app-secret-file <file>
  ```

  Remove the file afterwards. Step by step:
  [The client, for the plugins' sign-in](/github-app#the-client-for-the-plugins-sign-in).

**The edge.**

- In the proxy in front of the host, add `plugin-[a-z][a-z0-9]*` and
  `plugins` to the matcher of the edge's names. With
  `TEHAS_EDGE_ONE_LEVEL=on`, pass those names on without rewriting `Host`.
- In Cloudflare, add a cache rule that bypasses the cache for both names. A
  page behind a sign-in must never sit in a shared cache.

[Plugins' views](/edge#plugins-views) has the matcher, the one-level form
and the cache rule as you write them.

## The reference plugin: Floorplan

[tehas-app/tehas-plugin-floorplan](https://github.com/tehas-app/tehas-plugin-floorplan)
draws a [Floorplan](https://github.com/haydenschmitty/floorplan) map of the
product. A refresh makes or refreshes the map with the model and shows it in
the view, with a Refresh button. A hook rebuilds the map from a pull
request's head with no model call and reports what the change added to
Floorplan's Health check: kinds of finding, file and line, never a value. Its
verdict is `note` or `pass`. It asks for `view`, `product`, `model`,
`action:refresh` and `hook:after:work`, the grants of the example above.

To try it now, follow "Try it by hand" in its README: Node 22, git, a model
endpoint that speaks chat completions with tools, and a checkout of a
product. The driver writes the map to a folder you open in a browser. To
check its manifest and Dockerfile against tehas's rules:

```bash
git clone https://github.com/tehas-app/tehas-plugin-floorplan
tehas plugin check tehas-plugin-floorplan
```

The driver has made a first map with the factory's model, run by hand on
2026-11-21, not in a plugin's container. That run is where the cost above
comes from.

## Reference

- [tehas plugin](/tools-control#tehas-plugin): the subcommands, runs, the
  tick, hooks and their exit statuses.
- [The settings table](/tools#instance-settings): every `TEHAS_PLUGIN*` rule.
- [tehasbot](/tools-tehasbot): the units' commands.
- [A GitHub App](/github-app): its permissions and the sign-in's client.
- [The edge](/edge#plugins-views): the plugins' names and the gate's address.
- [What the feature leaves visible](/contract#plugins) while it is building.
- [Features](/features): how far plugins are built and proven.
- The schemas in `api/`: `plugin-manifest.schema.json`,
  `plugin.schema.json`, `plugin-review.schema.json`,
  `plugin-context.schema.json` and `plugin-verdict.schema.json`.
