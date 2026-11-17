### Changes

- Store schema 18: an outsourced job has two tokens. The model token goes into the container's `models.json` and reaches only the model paths. The job token stays with the contractor program and reaches only the job channel. Each is refused on the other's paths with 403. Both are made together, end together, and survive a restart of the model proxy. Outsource is still being built and cannot be switched on.
