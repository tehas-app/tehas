### Changes

- The dispatcher's branch check (`TEHAS_BRANCH_CHECK`) reads the check on the branch's head through the code host port, which folds every run and commit status of that name, so a passing job of one workflow does not hide a failure of another workflow's job with the same name. A failing commit status with the check's name now holds new cycles too, as a failed check run does. A poll that checks the branch makes three reads of GitHub for it where it made one: the branch's head commit, its check runs and its statuses.
