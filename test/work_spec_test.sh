#!/usr/bin/env bash
# Tests for libexec/work: what the spec pipe hands a cycle of what was
# approved, the spec its approval pinned and the approved title and body, and
# the runner it needs for that. The feature is still being built, so a case
# turns it on with TEHAS_TEST_FEATURES, as a test build may.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/work_lib.sh
. "$(dirname "$0")/work_lib.sh" || exit 1
stub_github

link=$F/12/search.md
spec='# Faster search\n\nR1: under 100 ms.\n'
sum=$(printf %b "$spec" | sha256sum | cut -d' ' -f1)
title="Faster search"
body="Make it fast.

Spec: $link sha256:$sum"

# spec_case [<setting>...]: nothing given of the issue's files unless the
# settings say so, the spec on the net, the issue and its approval with the
# pin, and a runner that reads the handoff.
spec_case() {
  images_off
  t_instance "${T_MINIMAL[@]}" TEHAS_WORK_IMAGES=0 TEHAS_WORK_FILES=0 "$@"
  net "$link" "$spec"
  issue_is "$body" OWNER "$title"
  record_is alice "$body"
  record_binds_title "$title"
  record_edit --arg l "$link" --arg s "$sum" '.spec = {link: $l, sha256: $s}'
  caps_good
  jq '.features.issueSpec = true' "$T/caps.json" >"$T/caps.new" && mv "$T/caps.new" "$T/caps.json"
}
# pipe_on [<setting>...]: the same with the feature on.
pipe_on() {
  spec_case TEHAS_SPEC_PIPE=on "$@"
  T_ENV+=(TEHAS_TEST_FEATURES=spec-pipe)
}

t_case "with the feature off a pinned spec is not fetched and the cycle is given nothing more"
spec_case
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(t_calls curl)" ""
t_eq "$(find "$imgs" -maxdepth 0 2>/dev/null | wc -l)" 0
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$mounts_off" | LC_ALL=C sort)"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" | LC_ALL=C sort)"

t_case "with the feature off the switch alone does nothing, and images are given as before"
spec_case TEHAS_SPEC_PIPE=on TEHAS_WORK_IMAGES=1
t_run "$work 7"
t_eq "$t_rc" 78
t_contains "$t_out" "spec-pipe"
t_eq "$(t_calls curl)$(t_calls gh)" ""
spec_case TEHAS_WORK_IMAGES=1
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(manifest '[has("spec"), has("approvedText")]')" '[false,false]'
t_eq "$(cd "$imgs" && find . -mindepth 1 | tr '\n' ' ')" "./manifest.json "

t_case "with the feature on a runner that does not read the spec starts nothing: 78, named, nothing fetched"
pipe_on
caps_good
t_run "$work 7"
t_eq "$t_rc" 78
t_contains "$t_out" "the feature issueSpec"
t_eq "$(t_calls curl)" ""
t_eq "$(cycle_args)" ""
t_eq "$(find "$imgs" -maxdepth 0 2>/dev/null | wc -l)" 0

t_case "with the feature on the cycle is given the spec of the pinned bytes and the approved text, in one read-only directory"
pipe_on
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$mounts_off" "$imgs:$inside.images:ro" | LC_ALL=C sort)"
t_eq "$(after -e | sed 's/=.*//' | LC_ALL=C sort)" "$(printf '%s\n' "$want" PI_ENSEMBLE_EXTRA_IMAGE_DIRS | LC_ALL=C sort)"
t_eq "$(after -e | grep '^PI_ENSEMBLE_EXTRA_IMAGE_DIRS')" "PI_ENSEMBLE_EXTRA_IMAGE_DIRS=$inside.images"
t_eq "$(cd "$imgs" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./approved.json ./manifest.json ./spec.md "
t_eq "$(printf %b "$spec" | cmp - "$imgs/spec.md" && echo same)" same
t_eq "$(manifest '.spec')" "{\"file\":\"spec.md\",\"link\":\"$link\",\"name\":\"search.md\",\"bytes\":$(printf %b "$spec" | wc -c),\"sha256\":\"$sum\",\"block\":1}"
t_eq "$(manifest '[.version, .issue, .approval.by, .images, (keys_unsorted | .[-2:])]')" '[1,7,"alice",[],["spec","approvedText"]]'
t_eq "$(jq -c '[.number, .title, .body]' "$imgs/approved.json")" "$(jq -c -n --arg t "$title" --arg b "$body" '[7, $t, $b]')"
t_eq "$(manifest '.approvedText | [.file, .bytes, .sha256]')" \
  "[\"approved.json\",$(wc -c <"$imgs/approved.json"),\"$(sha256sum "$imgs/approved.json" | cut -d' ' -f1)\"]"
t_contains "$t_out" "tehas work: the cycle is given its spec $link (sha256 $sum) and the approved title and body"
t_eq "$(t_calls curl | grep -c -e "$link")" 1
t_contains "$(cat "$T/calls/curl-headers")" "$link Authorization: Bearer ghs_app"

t_case "with the feature on a spec whose bytes changed since the approval starts nothing: exit 4, said on the issue"
pipe_on
net "$link" '# Faster search\n\nR1: under 1 s.\n'
t_run "$work 7"
t_eq "$t_rc" 4
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
t_eq "$(find "$imgs" -maxdepth 0 2>/dev/null | wc -l)" 0
t_contains "$t_out" "tehas work: the bytes of its spec are not the ones approved: they have sha256"
t_contains "$(t_calls gh)" "issue comment 7 --repo acme/widget --body Tehas started no cycle: the bytes of its spec are not the ones approved"

t_case "with the feature on a spec that cannot be fetched starts nothing"
pipe_on
rm -f "$fx/net/$(printf %s "$link" | tr -c 'A-Za-z0-9' _)"
t_run "$work 7"
t_eq "$t_rc" 4
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
t_contains "$t_out" "tehas work: its spec could not be fetched (curl exited 22), not starting a cycle"

t_case "with the feature on an approval with no spec hands over the approved text alone, on a runner that need not read it"
pipe_on
record_edit 'del(.spec)'
caps_good
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(t_calls curl)" ""
t_eq "$(cd "$imgs" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./approved.json ./manifest.json "
t_eq "$(manifest '[has("spec"), .approvedText.file]')" '[false,"approved.json"]'
t_contains "$(after -v)" "$imgs:$inside.images:ro"
t_contains "$t_out" "tehas work: the cycle is given the approved title and body"

t_case "with the feature on and images given the spec joins the images in the same manifest"
pipe_on TEHAS_WORK_IMAGES=1
net "$A/1111" "$png"
issue_is "$body ![shot]($A/1111)" OWNER "$title"
record_is alice "$body ![shot]($A/1111)" "$A/1111"
record_binds_title "$title"
record_edit --arg l "$link" --arg s "$sum" '.spec = {link: $l, sha256: $s}'
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(cd "$imgs" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./approved.json ./image-1.png ./manifest.json ./spec.md "
t_eq "$(manifest '[(.images | map(.file)), .spec.sha256, .approvedText.file]')" "[[\"image-1.png\"],\"$sum\",\"approved.json\"]"
t_eq "$(jq -c 'has("spec")' "$kept")" false

t_case "with the feature on a run by hand is its own approval and is handed nothing"
pipe_on
t_store_home "$T/home/tehas"
t_run "$work 7"
t_eq "$t_rc" 2
t_eq "$(t_calls curl)" ""
t_eq "$(find "$imgs" -maxdepth 0 2>/dev/null | wc -l)" 0

t_done
