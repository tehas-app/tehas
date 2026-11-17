### Changes

- Outsource, still being built and refused in a release, gains its commands
  and its contractor. `tehas outsource invite|list|status|revoke|drain`
  manage the contractors of a factory. The model proxy's listener also
  answers the job channel: a contractor joins with a one-time code, takes the
  job in its hand, checks in, renews the cycle's token, relays its issue
  writes, which the factory checks as the local relay does and makes with its
  own token, and posts the result. `tehas-contractor join|run|drain|doctor`
  runs a job with the same `tehas work` and container as the factory, and
  stops its cycle in an orderly way when it loses the factory: the stop
  signal, the grace, a push of the branch while the job is current, a report,
  and a wipe of the job's files. A cycle on a contractor gets exactly the
  mounts and variables of a cycle at home. With outsource off nothing changes
  (#2274).
- A new setting, `TEHAS_OUTSOURCE_CONTRACTORS`, lists the contractors the job
  channel answers. Empty, the default, refuses every contractor (#2274).
