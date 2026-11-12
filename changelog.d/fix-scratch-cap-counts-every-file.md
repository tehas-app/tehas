### Changes

- The scratch cap counts every file a cycle writes: a scratch under `TEHAS_SCRATCH_DIR` with more than 500,000 files and directories counts as over `TEHAS_SCRATCH_MAX`, and as the largest under `TEHAS_CYCLE_STOP_FREE`, instead of as the part that was read.
