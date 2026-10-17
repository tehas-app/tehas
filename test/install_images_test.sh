#!/usr/bin/env bash
# Tests for install.sh: what a run says when the sandbox image does not
# read the images a cycle is given.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh"

# --- The images a cycle is given -------------------------------------------
# TEHAS_WORK_IMAGES=1 promises a cycle the images of its issue. The sandbox
# image carries a label its build writes, and says through it whether its
# pi-rukas reads them. install.sh says what it found; it is a disagreement
# between the setting and the image, so the install still succeeds.
t_env
docker_labels() { # <label value, blank for an image with no label, absent for no image>
  case "$1" in
    absent)
      t_stub docker 'case "$1 $2" in "version --format") echo 29.1.3 ;; "network inspect") echo "true bridge false isolated" ;; "image inspect") exit 1 ;; esac'
      ;;
    blank)
      t_stub docker 'case "$1 $2" in "version --format") echo 29.1.3 ;; "network inspect") echo "true bridge false isolated" ;; "image inspect") exit 0 ;; esac'
      ;;
    *)
      t_stub docker "case \"\$1 \$2\" in \"version --format\") echo 29.1.3 ;; \"network inspect\") echo \"true bridge false isolated\" ;; \"image inspect\") echo \"$1\" ;; esac"
      ;;
  esac
}
img_commit=$(printf 'c%.0s' {1..40})
img_name=tehas/pi-rukas:uid$(id -u)

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


t_done
