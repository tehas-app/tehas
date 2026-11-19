### Changes

- Building the sandbox image survives a flaky apt mirror: each apt step retries its fetches, and when it still fails it runs once more after 30 seconds. CI, the release gate and the release workflow retry a tool download whose transfer fails, and still check it against its pinned sum.
