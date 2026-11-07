### Changes

- `tehas-cycle finish` rebases in a repository of its own beside the kept workspace and pushes to the instance's repository on GitHub; it runs no git command in the workspace.
- The read-only status reads a workspace's pull request file only when it is a plain file.
