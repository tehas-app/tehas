### Changes

- The pinned Node moves from 22.23.5 to 26.13.1, the current LTS line. Node 22 leaves support on 2027-04-30. Pi, ctx7 and parallel-cli run on it unchanged.

### Adopter action

After the update, run `./bootstrap.sh user` (the update's `later:` line says so): it replaces Node in `~/tehas/tools/node` and reinstalls the npm tools against it.
