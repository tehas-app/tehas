### Changes

- `tehas setup` rebuilds the sandbox image when the instance's `sandbox.Dockerfile` changed. It used to leave an image on the current pi-rukas commit as it was, so the change reached cycles only after `tehas update`.
