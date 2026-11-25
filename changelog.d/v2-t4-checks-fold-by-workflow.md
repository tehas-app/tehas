### Changes

- A pull request's checks hold the newest run of each GitHub Actions workflow. A newer run of the same workflow, scheduled or re-run, now supersedes an older one, so a commit whose workflow failed once and passed in every run since reads passing, where it read failing forever. Two workflows with a job of the same name are still both counted, so a failing one is never hidden by a passing one. The workflows come from one read of the commit's workflow runs; when that read fails, the checks are not read that poll. A check of any other App keeps the newest run of each check suite.
