### Changes

- A container's `--cpus` is capped at the CPUs the Docker daemon counts. A daemon started before the host was given more CPUs refused every cycle's install with exit 125; tehas now sizes the containers for Docker's count and logs a line naming both numbers.
- A start that a step of `tehas work` stopped before the runner ran is parked with a comment that names the driver and, for a failed product install, what failed, such as `docker refused --cpus 47.00, it counts 16 CPUs`, instead of `status unknown, last step -`.

### Adopter action

If a command logs `its Docker daemon counts <n>`, restart the Docker daemon when no cycle runs (`sudo systemctl restart docker`) so containers get the host's CPUs.
