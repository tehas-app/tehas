### Changes

- Security fix: every container tehas starts from the sandbox image now has `--pull=never`. When the local image is gone, for example after `docker system prune -a`, the start stops with `No such image` instead of pulling an image of that name from Docker Hub and giving it the cycle's token and model keys. Build the image again with `tehas update`.
- A push check that Docker could not start (exit 125, 126 or 127, or a line of Docker's own) no longer records a push fault or tells the issue the sandbox cannot push. The start stops with 78 and says why, as the poll's probe already did.
- With `TEHAS_EGRESS=on`, each Docker call that sets up the egress proxy ends within 60 seconds, and a start waits at most 15 minutes for `state/egress.lock`. A dockerd that stops answering no longer holds a start and its issue's lock for good.
- The token renewer of a `tehas work` that was killed outright ends within a second, so it no longer keeps the issue's lock and a later `tehas work` of that issue can start.
