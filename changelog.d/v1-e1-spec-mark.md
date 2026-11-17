### Changes

- A new optional feature, `spec-pipe` (`TEHAS_SPEC_PIPE`), is registered and still being built: a release refuses it set to `on` with exit 78, and `tehas api --json` lists it in `flags` as `false`. With it on, an issue names its spec with one line of its body, `Spec: <link> sha256:<64 hex digits>`, the link a file attached on GitHub; a line tehas cannot read starts nothing and the poll says why once.
