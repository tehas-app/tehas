### Changes

- The tidy reads what closed its issues through the tickets port, by the rule `tehasbot status` uses. When that read fails, the line still starts `WARN: tidy: the read of what closed these issues failed:`, and the reason after it is the port's, in tehas's words, not gh's. A pull request of another repository no longer earns `tehas:shipped` from the tidy, as it no longer does from `tehasbot status`.
