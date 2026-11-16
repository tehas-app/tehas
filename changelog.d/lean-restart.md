### Changes

- A retry of a lean cycle starts it: `work-lean` is no longer given
  `--restart`, which it does not take, so the image's answer no longer refuses
  the start with exit 78 ("the flag `--restart`"). Every cycle is a fresh
  clone, so a lean run has no state to discard; classic restarts are
  unchanged.
