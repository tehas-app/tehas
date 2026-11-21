### Changes

- `tehas <part> [words]` runs `tehas-<part>` from `PATH`, so the other parts of the tehas family are commands of tehas: `tehas industry ...` runs industry. A command of tehas always wins, and `industry`, `kaizen`, `kata`, `monozukuri` and `yokoten` are kept for the family. `tehas help <part>` shows the part's help.
- Each release carries `tehas-front-vX.Y.Z-linux-x86_64`, a small signed `tehas` for a machine that runs no factory, such as the one that runs industry. It knows only its help and the parts' commands.
