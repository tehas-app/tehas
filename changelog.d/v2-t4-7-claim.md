### Changes

- The dispatcher's claim of an issue is three writes in place of one: the `tehas:running` mark, then the label it takes the place of off, then the `TEHAS_ASSIGNEE` assignee. That is one more GitHub request per claim. A poll stopped between them leaves the issue marked running, so it is never started twice. An assignee login that cannot be resolved is said once in `logs/dispatch.log`, and that poll's claims go without one.
