### Changes

- Every service in `host/systemd` sets `NoNewPrivileges=yes`, and `tehas-mcp`,
  `tehas-top` and `tehas-energy` have `MemoryMax=256M`. The cycle's unit
  passes the issue to its shell as an argument.
- The docs server runs as the edge router does: a read-only root, every
  capability dropped but `NET_BIND_SERVICE`, and `no-new-privileges`.
- The dashboard draws a link from the queue file only when it is a web
  address.
- With `TEHAS_EGRESS=1`, the push check and the install of the product's
  dependencies before a cycle are on the cycle's network too.
- The product's `.pi/settings.json` in a workspace is read only as a plain
  file of at most 64 KiB; the manual names the files the host reads there.
- A bounded call of the poll holds no more of a program's answer than its
  ceiling.

### Adopter action

With `TEHAS_EGRESS=1`, a product whose install fetches from a host outside the
proxy's list now fails its install before the cycle starts: add that host to
`TEHAS_EGRESS_HOSTS`.
