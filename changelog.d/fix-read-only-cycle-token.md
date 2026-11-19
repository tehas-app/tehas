### Changes

- On a read-only instance (`TEHAS_READ_ONLY=on`), the GitHub token a cycle's
  container, a planning turn or a manual session is given reads only: Contents,
  Pull requests and Issues read. Before, it could write, and only the image's
  `gh` and the container's git configuration kept a cycle from writing. A token
  kept from before the instance turned read-only is made again. `tehas update`
  asks a read-only instance's cycle token for no push and says
  `Cycle token:  OK (read-only: ...)`.
