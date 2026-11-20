### Changes

- A release now pins the exact pi-rukas commit it was released with, so installing or updating to that release always gets the same pi-rukas. Its `host/versions.env` ends with `PI_RUKAS_FOLLOW=commit:<sha>`, the commit `release:0` named when the release was packed. The release workflow's log names the commit and its version. If upstream cannot be asked, the release is not published.
- An install from a checkout still follows the newest pi-rukas release on the line, `PI_RUKAS_FOLLOW=release:0`, as before.
- pi is pinned at an exact version in both, 1.5.2 today, with its lockfile in `host/npm/pi/`.
- An instance that sets `PI_RUKAS_FOLLOW` in `instance.env` still wins for its sandbox image, as before, for example `release:0` to keep following the newest pi-rukas on a host installed from releases.
