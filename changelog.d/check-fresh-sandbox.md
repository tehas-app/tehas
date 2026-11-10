### Changes

- `install.sh --check` no longer reports `Sandbox: BEHIND` for an image built after the last status run. It read the health file, which still named the image before, so an update that rebuilt the image on a new pi-rukas release stopped at its own check until the next poll. When the image's `tehas.rukas` label is not the image the health read, the line is `Sandbox: NOT READ` and is not a finding.
