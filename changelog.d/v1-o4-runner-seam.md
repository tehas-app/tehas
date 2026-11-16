### Changes

- The cycle supervisor can take a job waiter in place of `tehas work`: `tehas-cycle cycle <issue> --runner job` reports a cycle run on another host from the notes and exit status the waiter leaves, and reads no workspace or image of the factory. It is refused with 78 unless `TEHAS_OUTSOURCE=1`. In-house cycles are unchanged.
