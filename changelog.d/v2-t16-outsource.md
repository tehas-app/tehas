### Changes

- Outsource is dropped in 2.0, with the model proxy and the contractor program. It was still being built: no release could turn it on. Gone are `tehas outsource`, `tehasbot outsource-proxy`, the programs `tehas-proxy` and `tehas-contractor`, `tehas-outsource-proxy.service`, `outsource` in the `flags` of `tehas api --json`, `contractors` in `tehas status --json` with the sample `api/status.outsourced.json`, the MCP tool `factory_contractors`, `live/contractors.json`, and the vipune stand-in of the sandbox image.
- `TEHAS_OUTSOURCE` and `TEHAS_OUTSOURCE_TIMEOUT`, `_GRACE`, `_RETRIES`, `_REST`, `_REPOS`, `_CONTRACTORS`, `_JOIN_TTL`, `_PROXY_ADDR` and `_PROXY_IDLE` are no longer read. An instance that still sets one loads as before, with one warning line that names it, and the setting is dropped.
- The state store moves to schema 27 at the update. The step drops the five tables of outsource, `contractors`, `join_codes`, `jobs`, `job_history` and `issue_attempts`, which no release wrote a row of, and keeps every other row. `store init` and `store migrate` no longer take `--outsource`, and the store's contractor, invite and job verbs are gone.
- `tehas update` and `install.sh` stop, disable and remove `tehas-outsource-proxy.service`, and remove `libexec/tehas-proxy`, `libexec/tehas-contractor`, `live/contractors.json` and `sandbox/vipune-shim`, where an earlier version left them.

### Adopter action

Run `tehas update`. Then take any `TEHAS_OUTSOURCE` and `TEHAS_OUTSOURCE_*` lines out of `instance.env`.
