### Changes

- With the spec pipe on (still being built, so a release refuses it), a cycle is given the spec its approval pinned, fetched again and held to that sha256, and the approved title and body as `approved.json`, both named in the manifest of the issue's directory under `spec` and `approvedText`. A cycle with a spec starts only on a sandbox image whose pi-rukas lists `issueSpec`.
