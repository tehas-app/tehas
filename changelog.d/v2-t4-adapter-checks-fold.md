### Changes

- A pull request's checks hold the newest run of each workflow, not only of each check name. A workflow triggered again on the same commit by another event no longer hides the older run's failure: the checks stay failed until the workflow that failed is itself re-run and passes. Two workflows with a job of the same name are both counted.
