### Changes

- The sandbox image hands `/home/vscode/.npm` to the cycle's user, so npm in a cycle no longer fails on a root-owned cache. The image build stops when that user cannot write to npm's cache (#2407).
- The sandbox image sets `ORT_INTRA_OP_NUM_THREADS=1`, so vipune no longer writes onnxruntime affinity errors to stderr on every call under a cycle's cpuset. The build's vipune check runs on one core and stops on any stderr (#2408).

### Adopter action

Rebuild the sandbox image (`install.sh` rebuilds it) for the fixes to reach cycles.
