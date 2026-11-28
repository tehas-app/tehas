### Changes

- With `TEHAS_BRANCH_CHECK` set, an idle poll reads the branch's check in one GitHub call again: one GraphQL question for the branch's head, its check runs, their workflow runs and the commit's statuses (was four calls). Where GitHub cannot answer it whole, or refuses part of it, the poll reads them one by one as before, and says the same lines.
