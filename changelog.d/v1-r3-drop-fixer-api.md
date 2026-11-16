### Changes

- `tehas api` no longer lists the old names of the millwright: the field `fixer` and the feature word `fixer-grant` are gone from `tehas api --json` and from the feature line of `tehas api`.

### Adopter action

A program that reads `tehas api --json` reads `millwright` instead of `fixer`, and checks for the feature `millwright-grant` instead of `fixer-grant`.
