### Changes

- 1.0 updates a host on 0.30.x, 0.31.x or 0.32.x. A 0.30.x host can name only a release candidate or the release with `tehas update --to`, not an alpha or a beta; update to v0.32.0 first to try a beta. The store's tests now migrate the stores v0.31.0 and v0.32.0 made, and a shell test updates a home a v0.32.0 install left.
- The update manual names a known limit: a program that manages the factory and writes 0.30 names such as `TEHAS_TICKET_CONCURRENCY` into its `instance.env` stops the update with 78 until the manager writes 1.0's names.
