### Changes

- One function decides how a work run ended, and four ends in the analytics change. A run `tehas stop` stopped is `stopped` and a lost run is `failed`; both were `parked`. A run that parked is never `merged`, `stopped` or `pr`: a parked run that claimed a merge the report did not find, and a parked run whose issue closed or that was interrupted, end as `handoff`, `failed` or `parked`. Runs recorded before the update keep their outcome.

### Adopter action

A program that reads the analytics copies and compares counts of `runs.outcome` across the update sees the four moves from then on: runs that counted as `parked` now count as `stopped` or `failed`, and parked runs that counted as `merged` or `stopped` now count as `handoff`, `failed` or `parked`. The words are the same.
