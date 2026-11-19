### Changes

- The installer and `tehas update` on an aarch64 or arm64 machine now stop at once with "this machine has no tehas release", instead of failing to download a tarball that was never built. Releases are built for x86_64 only, and the manual now says so.
- `tehas state copy` and `tehas state load` examples and samples show the schemas 1.0 prints, and the manual lists the refusal of a copy older than v0.30.0.
- The manual says that a freeze moves only tehas's own time limit: the runner's deadline and the cycle unit's 8-hour stop still count while a cycle is frozen.
- llms.txt and the analytics pages no longer describe the Pi RPC path, which is gone.
