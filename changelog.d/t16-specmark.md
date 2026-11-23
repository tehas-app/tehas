### Changes

- The spec mark is dropped. It was still being built and a release refused it. A `Spec: <link> sha256:<hex>` line in an issue is now plain text: the poll does not read it, a start fetches and pins no spec, a cycle is given no `spec` or `approved.json`, and no pull request is checked for a `<!-- tehas:spec v1 -->` block or parked as `spec-block`. `"spec-pipe"` is gone from the `flags` of `tehas api --json`. Lean's `pi-spec v1` block is a different thing and stays.
- `TEHAS_SPEC_PIPE` is no longer read. A line of it still loads, with one warning line that names it, and the setting is dropped.

### Adopter action

Take any `TEHAS_SPEC_PIPE` line out of `instance.env`.
