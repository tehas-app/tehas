### Changes

- `tehas status --json`: where a running cycle's step is moves into one object, `running[].progress` (`step`, `stepNumber`, `stepTotal`, `round`, `roundLimit`, `stepSince`). It is driver-specific and may change in a minor release. The schema now names `readOnly`, which the object already printed, and a third sample, `api/status.readonly.json`, shows it.
- `tehas history --json`: the list is `runs`, not `cycles`: it holds cycles and planning turns. Where the step a cycle ended in was moves into `runs[].progress` (`step`, `stepNumber`, `stepTotal`, `round`), driver-specific like the one of status. `tehas export --json` prints `runs` too, and each `history.json` of an export directory carries `progress`.
- `tehas export` writes `export.json` at the top of the export directory, `{"version": 1}`, the layout of the directory, and `tehas export --json` gains the same `version`.
- `tehas start --json`: the approval's sums are `bodySha256` and `titleSha256`, camelCase like every other `--json` object.
- `tehas refresh --pr N --json` (and its old name under `tehasbot`): the head is `headSha`, and the object gains `"api": 1`, as every other `--json` object has.
- `tehas disk --json` and `live/disk.json`: `generatedAt` is now `time`, the word `tehas status --json` uses for when an answer was written. The MCP tool `factory_disk` answers `time` too.
- The MCP tool `factory_disk` serves no path of the host: `dockerRoot` is gone, a file system's `paths` are roles (`home`, `docker`, `tmp`, `scratch`), and a place's `path` is its path under the instance's home, or its name alone outside it. `tehas disk --json` and `live/disk.json` keep the absolute paths for a person on the host.

### Adopter action

A program that reads `--json` objects of tehas reads the new names. Every renamed field is a row of `api/renames-1.0.tsv`.

- `tehas status --json`: read `running[].progress.step` and the other step fields under `progress`, not on `running[]` itself.
- `tehas history --json` and `tehas export --json`: read `runs`, not `cycles`, and the step of a line under `runs[].progress`.
- `tehas start --json`: read `approval.bodySha256` and `approval.titleSha256`.
- `tehas refresh --pr N --json`: read `headSha`, not `head_sha`.
- `tehas disk --json`, `live/disk.json` and the MCP tool `factory_disk`: read `time`, not `generatedAt`.
- The MCP tool `factory_disk`: `dockerRoot` is gone; read the roles in `filesystems[].paths` and the home-relative `full.places[].path`.
