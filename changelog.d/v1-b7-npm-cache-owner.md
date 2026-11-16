### Changes

- The sandbox image no longer leaves a root-owned npm cache in `/home/vscode/.npm`, where npm in a cycle failed with `EACCES`. The image build stops if root owns anything there.
