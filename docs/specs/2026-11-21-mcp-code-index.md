# The code index on the MCP server

Step 4 of epic #2689, for milestone 2.1. Steps 1 to 3 (the shared checkout,
its fetch on a connection, tickets) are in v1.4.0.

The owner's decision: the index is the one `tehas index` builds, the
codebase-memory-mcp index that cycles search. It is not built a second time,
and it is rebuilt when the clone moves.

## What was measured

On this repository, with codebase-memory-mcp 0.12.1 (the pin):

- A build takes 6 s and writes a 145 MB database.
- A query against a copy of the cache directory that nobody may write answers
  in about 1.3 s, with a read-only `HOME` and `TMPDIR`.
- A code snippet is read from the source files at the path the index was
  built from, not from the database. With the tree moved away, the answer is
  `(source not available)`.
- The `cli` mode leaves a daemon running (`--cbm-daemon-internal`), and no
  setting turns that off.

## The design

1. **One index, published when a build ends.** After a run of `tehas index`
   that wrote a database, and only while `TEHAS_MCP_CHECKOUT` is set, the run
   copies the index to `state/index-mcp`. It copies into a new directory,
   then renames it into place, so a query never reads half a build. A query
   that is open keeps the files it opened. The copy keeps the run's record:
   the commit indexed, the image that built it, and the time. Cycles keep
   their own copy as they do now, and a query never holds up a cycle or a
   build.
2. **A query runs in its own container, not in the server.** The query
   container is started from the sandbox image whose id the copy records,
   which is the image that wrote the index. Its arguments are pinned like the
   indexer's: `--network none`, `--read-only`, `--cap-drop ALL`,
   no-new-privileges, `--init` and `--rm`, a tmpfs `/tmp`, and `tehas.slice`
   with a small memory limit. It is given two mounts: the published index
   read-only at the cache directory, and the shared clone read-only at
   `TEHAS_WORK_DIR`, the path the index was built from, so snippets resolve.
   It gets no token, no setting and no network. `--rm` ends the daemon with
   the container.
3. **A broker on the host, reached through a read-only mount.**
   `tehas-mcp-code.socket` (`Accept=yes`, `MaxConnections=4`, mode 0600) holds
   its socket in `~/tehas/run/mcp-code`, and the MCP container is given that
   directory read-only, the same way as the fetch socket and the plugin calls.
   `tehasbot mcp code` reads one request of at most 2 KiB. It runs the query
   container under a 20 s bound and answers at most 64 KiB.
4. **The server's tools.** Full keys only, and only while the server has the
   code socket and a checkout:

   | Tool | Runs | Arguments |
   | --- | --- | --- |
   | `factory_code_search` | `search_graph` | `query` or `name`, optional `label` and `limit` |
   | `factory_code_text` | `search_code` | `pattern`, optional `limit` |
   | `factory_code_snippet` | `get_code_snippet` | `name`, the qualified name a search gave |
   | `factory_code_outline` | `get_file_outline` | `path` |
   | `factory_code_trace` | `trace_path` | `name`, `direction` (`inbound` or `outbound`), optional `depth` |

   The server builds the indexer's arguments itself, so a client never names
   a project or a raw argument. The broker checks the same list again. Tools
   that write (`index_repository`, `delete_project`, `manage_adr`,
   `ingest_traces`) are never reachable. Open Cypher (`query_graph`) is left
   out too, because a query of any size could hold a slot.
5. **Answers.** Every answer carries `indexed` (the commit of the index) and
   `head` (the clone's commit). When the two differ it also carries
   `stale: true`, because the index's line numbers can be off by the changes
   since. Answers are masked and carry `untrusted`, as the git tools' do.
6. **Rebuilt when the clone moves.** When `tehasbot mcp fetch` moves the
   clone to a commit the index does not record, it starts
   `tehas-index.service` without waiting. The run takes its usual lock and
   waits for cycles as it does now. The dispatcher still rebuilds the index
   before cycles, as now.

## Changes to the MCP container

| Where | Was (v1.4.0) | Is |
| --- | --- | --- |
| Its mounts | The four fixed, the plugin calls, the checkout, the fetch socket | And `~/tehas/run/mcp-code:/run/tehas/code:ro`, only with the checkout. Pinned by `test/fixtures/argv/mcp-code` |
| Its arguments | ... `--checkout-fetch <socket>` | And `--code /run/tehas/code/code.sock` |
| What it reaches | The fetch socket and the plugin socket | And the code socket, one request of at most 2 KiB per connection |
| What the host runs for it | A fetch at most once a minute | And a query container per call, at most four at a time, 20 s each |

The new mount is a security change, and its pull request says so.

## Cost

- Disk: one more copy of the index, 145 MB for this repository.
- Time: a container start plus about 1.3 s for each call.
- Code: no new crate in the server, and no codebase-memory-mcp in its image.

## Why not the alternatives

- **Running codebase-memory-mcp inside the MCP container.** It would put a
  large C program, fed text from the internet, in the process that holds
  every key, and it would leave its daemon running there. The image would
  also need a second copy of the indexer, which can drift from the one that
  wrote the index.
- **Reading the database directly.** Its schema belongs to the indexer and
  changes between releases. Snippets would still need the source files.
- **Reading `state/index` in place.** A rebuild rewrites it, so a query could
  read half a build.

## Built after v2.0.0

The coding standard v2 refactor gives the `$TEHAS_HOME` layout an owner
(T5). `state/index-mcp` and `run/mcp-code` will be added there, not as new
string paths. This step is built on main once the refactor branch has merged.
