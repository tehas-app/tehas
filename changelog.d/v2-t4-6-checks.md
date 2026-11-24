### Changes

- The dispatcher's branch check (`TEHAS_BRANCH_CHECK`) reads every run of the check on the branch's head and keeps the newest of each workflow, so a workflow run again does not hide an older failure of another workflow's job with the same name. A failing commit status with the check's name now holds new cycles too, as a failed check run does. A poll that checks the branch makes three reads of GitHub for it where it made one: the branch's head commit, its check runs and its statuses.
