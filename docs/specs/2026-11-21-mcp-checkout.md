# The MCP server reads the shared checkout

Issue #2709, step 1 of epic #2689. An outside agent asks why the product is
the way it is. The git history is the first source: the log, a commit and its
diff, and who last changed each line.

## What it is

- `TEHAS_MCP_CHECKOUT`, a host setting: the path of one clone of the product
  with its whole history. Every MCP session and every agent user on the host
  reads the same clone. Unset, nothing changes.
- `tehas mcp` mounts it read only at `/checkout` and passes
  `--checkout /checkout`. A path that is no directory stops it with 78.
- The server opens it with gix at start and on every call. Not a git
  repository with a commit, or a shallow clone: 78.
- Three tools, full keys only: `factory_git_log`, `factory_git_show`,
  `factory_git_blame`. Listed beside the plugin tools, outside the catalog of
  sixteen, as the plugin tools are.
- Every answer carries `head` and `untrusted`, and text is masked as logs are.
  An author is named, never an address.

## No program runs

gix reads objects; it runs nothing unless asked. The server opens the clone
with `Options::isolated()`: no system, user or environment config. Diffs and
blame read blobs as git stores them (`Mode::ToGit`), so no textconv, filter,
external diff or fsmonitor is run whatever the clone's config and attributes
say. A test plants each of them in a clone's config, shows that `git` itself
runs them, and shows that the server does not.

A commit argument is hex only, so no revision syntax reaches gix. A path is
relative, with no `.` or `..` part.

## Changes to the MCP server

A security change. `host/docs/mcp-server.md` changes with the code.

| Where | Was | Is |
| --- | --- | --- |
| What it reads | `~/tehas/live`, `~/tehas/mcp`, `~/tehas/logs` | The same, and the shared checkout when `TEHAS_MCP_CHECKOUT` is set |
| What it runs | Nothing | Unchanged: the checkout is read in the process, no `git` program |
| The container's mounts | The four fixed, then the plugin calls (#2692) | The same, then `<TEHAS_MCP_CHECKOUT>:/checkout:ro`, only when it is set. Pinned by `test/fixtures/argv/mcp-checkout` |
| Its arguments | ... `--plugin-calls <socket>` | Then `--checkout /checkout`, before `--outsource` |
| Its network | `tehas-mcp`, no masquerade | Unchanged |
| A full key | The factory's logs and sessions | And the product's code and history |
| A dashboard key | The four live-file tools | Unchanged |

## Cost

gix with `blame`, `revision` and `sha1` and no default features adds 107
crates to the lock file and doubles the release binary, from 3.4 to 7.3 MiB.

## Step 2: current on a connection (#2715)

- `tehas-mcp-fetch.socket` (`Accept=yes`, at most 4 at once) listens on
  `~/tehas/run/mcp-fetch/fetch.sock`. Each connection starts one
  `tehas-mcp-fetch@.service`, which runs `tehasbot mcp fetch`.
- The server connects to it on `initialize` and `server/discover`, on a thread
  of its own. It writes nothing: the socket's directory is mounted read only,
  as the plugin calls socket is.
- The fetch: a lock, nothing within 60 s of the last report, `git fetch --prune
  origin`, `git merge --ff-only @{upstream}`, both with `core.hooksPath=/dev/null`.
  The report, `~/tehas/mcp/checkout.json`, is read by the server from the
  directory it already has.
- The revision the server speaks keeps no session, so a client passes `seen`,
  the `head` it last saw, and the answer adds `checkoutUpdated`.

| Where | Was (step 1) | Is |
| --- | --- | --- |
| The container's mounts | ... `<checkout>:/checkout:ro` | And `~/tehas/run/mcp-fetch:/run/tehas/fetch:ro`, only with the checkout |
| Its arguments | ... `--checkout /checkout` | And `--checkout-fetch /run/tehas/fetch/fetch.sock` |
| What it reaches on the host | Nothing | A connection to one socket, which carries no byte |
| What the host runs for it | Nothing | `git fetch` and a fast-forward of the clone, at most once a minute |

## Not in these steps

Tickets, the code index and learnings are steps 3 to 5.
