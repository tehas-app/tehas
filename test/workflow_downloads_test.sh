#!/usr/bin/env bash
# Tests for the downloads of .github/workflows: every tool a job downloads with
# curl retries a failed transfer and is still checked against its pinned sum
# before it runs. The runners see TLS reads fail mid-transfer, so one bad read
# should not fail a gate. The calls of the GitHub API are not downloads and keep
# no retry: a POST tried twice could make a second release.
# shellcheck disable=SC2016  # the assertions name the literal text of the workflows
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

retry='curl -fsSL --retry 5 --retry-all-errors --retry-delay 5 -o '

for w in verify.yml release-gate.yml release.yml; do
  workflow=$T_ROOT/.github/workflows/$w

  t_case "$w: each download retries a failed transfer and is checked against its sum"
  # Each step on its own, from its name to the next one, so a sum in one step
  # cannot stand for a download in another.
  n=0
  while IFS= read -r name; do
    step=$(awk -v n="      - name: $name" '$0 == n {s = 1; next}
      s && /^      - (name|uses):/ {exit}
      s' "$workflow")
    downloads=$(grep -c -- ' -o "' <<<"$step")
    [ "$downloads" -gt 0 ] || continue
    n=$((n + downloads))
    t_eq "$name: $(grep -cF -- "$retry" <<<"$step") of $downloads" "$name: $downloads of $downloads"
    curl_at=$(grep -n -- ' -o "' <<<"$step" | tail -n 1 | cut -d : -f 1)
    sum_at=$(grep -n -- '| sha256sum -c -' <<<"$step" | tail -n 1 | cut -d : -f 1)
    [ -n "$sum_at" ] && [ "$sum_at" -gt "$curl_at" ] || t_bad "$name: no sum is checked after its download"
  done <<<"$(grep -E '^      - name:' "$workflow" | sed 's/^      - name: //')"
  [ "$n" -gt 0 ] || t_bad "$w downloads nothing, so the case checked nothing"

  t_case "$w: every curl is a retried download or a call of the GitHub API"
  others=$(grep -E '(^|[^a-z])curl ' "$workflow" | grep -v '^ *#' | grep -vF -- "$retry" |
    grep -vF -e 'Authorization: Bearer $GH_TOKEN' -e '"${auth[@]}"')
  t_eq "$others" ""
  t_lacks "$(cat "$workflow")" 'wget '
done

t_done
