### Changes

- On a host that runs a prerelease newer than the newest release, such as `v1.0.0-rc.2` while the newest release is `v0.32.0`, `tehas update --check` no longer names the older release as the newest or says `tehas update --to` moves the host back. It says the host is on a prerelease ahead of the newest release, and names the newest prerelease of the same version, such as `v1.0.0-rc.3`, when there is a newer one for `tehas update --to` to move to.
