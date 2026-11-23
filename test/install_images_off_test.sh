#!/usr/bin/env bash
# Tests for install.sh, split from install_images_test.sh: no sandbox image,
# images off, and an image the health calls behind.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1
real_console
t_env

t_case "images on with no sandbox image yet says nothing about images"
fresh
instance TEHAS_WORK_IMAGES=1
docker_labels absent
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$t_out" "TEHAS_WORK_IMAGES"

t_case "images off says nothing about images, whatever the image says"
fresh
instance
docker_labels "$img_commit|0"
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$t_out" "TEHAS_WORK_IMAGES"
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 0
t_lacks "$t_out" "Images:"
t_stub docker "$net_good"
t_stub systemctl


# --- The pi-rukas the image runs -------------------------------------------
# The factory's health says when the image is behind the pi-rukas the host
# follows (checks.rukas); --check reports it as tehas update's to fix.
t_case "--check reports an image the health calls behind, and nothing for one that is current"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
rm -f "$th/state/update-pending"
mkdir -p "$th/live"
a=$(printf 'a%.0s' {1..40}) b=$(printf 'b%.0s' {1..40})
printf '{"checks":{"rukas":{"image":"%s","imageVersion":"v0.14.53","target":"%s","targetVersion":"v0.14.54","behind":true}}}\n' "$a" "$b" >"$th/live/health.json"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Sandbox:      BEHIND (the image runs pi-rukas v0.14.53, the host follows v0.14.54; tehas update builds it)"
printf '{"checks":{"rukas":{"image":"%s","target":"%s","behind":true}}}\n' "$a" "$b" >"$th/live/health.json"
t_run "$install --check"
t_contains "$t_out" "Sandbox:      BEHIND (the image runs pi-rukas $a, the host follows $b;"
printf '{"checks":{"rukas":{"image":"%s","target":"%s","behind":false}}}\n' "$b" "$b" >"$th/live/health.json"
t_run "$install --check"
t_eq "$t_rc" 0
t_lacks "$t_out" "Sandbox:"
rm -f "$th/live/health.json"
t_run "$install --check"
t_eq "$t_rc" 0
t_lacks "$t_out" "Sandbox:"

t_done
