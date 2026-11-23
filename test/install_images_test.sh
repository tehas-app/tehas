#!/usr/bin/env bash
# Tests for install.sh: what a run says when the sandbox image does not
# read the images a cycle is given.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1
real_console

# --- The images a cycle is given -------------------------------------------
# The sandbox image's label says whether its pi-rukas reads the images
# TEHAS_WORK_IMAGES=1 promises. A disagreement is said; the install succeeds.
t_env
t_case "images on and a sandbox image that does not read them: said, and the install still succeeds"
fresh
instance TEHAS_WORK_IMAGES=1
docker_labels "$img_commit|0"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "note: TEHAS_WORK_IMAGES is 1 and the sandbox image $img_name says its pi-rukas does not read the images a cycle is given."
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 0
t_contains "$t_out" "Images:       NOT READ"
t_contains "$t_out" "does not read the images a cycle is given"

t_case "images on and a sandbox image that reads them: nothing is said"
fresh
instance TEHAS_WORK_IMAGES=1
docker_labels "$img_commit|1"
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$t_out" "TEHAS_WORK_IMAGES"
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 0
t_lacks "$t_out" "Images:"

t_case "images on and a sandbox image that says nothing names the build that records the answer"
fresh
instance TEHAS_WORK_IMAGES=1
docker_labels blank
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "note: TEHAS_WORK_IMAGES is 1 and the sandbox image $img_name does not say whether its pi-rukas reads the images a cycle is given. Rebuild it with $th/sandbox/build.sh, which records the answer."
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 0
t_contains "$t_out" "Images:       NOT SAID"

t_done
