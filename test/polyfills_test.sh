#!/usr/bin/env bash
# Tests for scripts/polyfills.sh: the polyfills a tree carries, which of them a
# --capabilities answer has made unnecessary, and their removal. Each case
# works on a scratch tree; the last checks this checkout.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

script=$T_ROOT/scripts/polyfills.sh
tree=$T/tree
# The marker is put together here, so this file marks no line of its own.
mark='# polyfill'

# A tree with a polyfill of someFeature and one of otherFeature, each with its
# test and a caller that uses it on a line of its own.
fresh() {
  rm -rf "$tree"
  mkdir -p "$tree/lib" "$tree/test" "$tree/libexec" "$tree/docs" "$tree/instances/a"
  for f in someFeature otherFeature; do
    printf '# shellcheck shell=bash\n# Upstream issue: up-owner/pi-rukas#%s\n# Upstream PR: up-owner/pi-rukas#%s\ntehas_polyfill_%s() { :; }\n' \
      "${#f}" "$((${#f} + 100))" "$f" >"$tree/lib/polyfill-$f.sh"
    printf '#!/usr/bin/env bash\n' >"$tree/test/polyfill_${f}_test.sh"
  done
  cat >"$tree/libexec/run" <<EOF
#!/usr/bin/env bash
case "\$mode" in
  native) echo native ;;
  polyfill) tehas_polyfill_someFeature ;; $mark someFeature
  off) echo off ;;
esac
tehas_polyfill_otherFeature $mark otherFeature
EOF
  # A record and an instance may name a marker; they are not the tree's code.
  echo "a line $mark someFeature" >"$tree/docs/notes.md"
  echo "a line $mark someFeature" >"$tree/instances/a/instance.env"
  echo '{"features":{"someFeature":true,"otherFeature":false}}' >"$T/caps.json"
}

t_case "list names each polyfill, its upstream issue and its upstream pull request"
fresh
t_run "'$script' --root '$tree' list"
t_eq "$t_rc" 0
t_eq "$t_out" "otherFeature up-owner/pi-rukas#12 up-owner/pi-rukas#112
someFeature up-owner/pi-rukas#11 up-owner/pi-rukas#111"

t_case "expired is the polyfills whose feature the answer sets to true"
t_run "'$script' --root '$tree' expired '$T/caps.json'"
t_eq "$t_rc" 0
t_eq "$t_out" someFeature
echo '{"features":{"someFeature":"true"}}' >"$T/caps.json"
t_run "'$script' --root '$tree' expired '$T/caps.json'"
t_eq "$t_out" ""
for bad in '[]' 'not json'; do
  echo "$bad" >"$T/caps.json"
  t_run "'$script' --root '$tree' expired '$T/caps.json'"
  t_eq "$t_rc" 1
  t_contains "$t_out" "is not a JSON object"
done

t_case "prune removes an expired polyfill, its test and its lines, and leaves the rest"
fresh
t_run "'$script' --root '$tree' prune '$T/caps.json'"
t_eq "$t_rc" 0
t_eq "$t_out" "libexec/run
lib/polyfill-someFeature.sh
test/polyfill_someFeature_test.sh"
t_eq "$(cd "$tree" && find lib test -type f | sort | tr '\n' ' ')" "lib/polyfill-otherFeature.sh test/polyfill_otherFeature_test.sh "
t_lacks "$(cat "$tree/libexec/run")" "someFeature"
t_contains "$(cat "$tree/libexec/run")" "tehas_polyfill_otherFeature"
t_contains "$(cat "$tree/libexec/run")" "off) echo off ;;"
bash -n "$tree/libexec/run" || t_bad "the caller no longer parses"
t_contains "$(cat "$tree/docs/notes.md")" someFeature
t_contains "$(cat "$tree/instances/a/instance.env")" someFeature
t_run "'$script' --root '$tree' check"
t_eq "$t_rc" 0

t_case "prune with nothing expired changes nothing"
fresh
echo '{"features":{}}' >"$T/caps.json"
before=$(find "$tree" -type f -exec cat {} + | sha256sum)
t_run "'$script' --root '$tree' prune '$T/caps.json'"
t_eq "$t_rc" 0
t_eq "$t_out" ""
t_eq "$(find "$tree" -type f -exec cat {} + | sha256sum)" "$before"

t_case "a marked line inside a larger statement is said when its removal breaks the file"
fresh
printf '#!/usr/bin/env bash\nif true; then %s someFeature\n  :\nfi\n' "$mark" >"$tree/libexec/broken"
echo '{"features":{"someFeature":true}}' >"$T/caps.json"
t_run "'$script' --root '$tree' prune '$T/caps.json'"
t_eq "$t_rc" 1
t_contains "$t_out" "libexec/broken no longer parses"

t_case "check names a polyfill without its upstream issue, pull request or test, and a marker without a polyfill"
fresh
sed -i '/^# Upstream issue/d' "$tree/lib/polyfill-someFeature.sh"
sed -i '/^# Upstream PR/d' "$tree/lib/polyfill-otherFeature.sh"
rm "$tree/test/polyfill_otherFeature_test.sh"
echo "x $mark missingFeature" >>"$tree/libexec/run"
t_run "'$script' --root '$tree' check"
t_eq "$t_rc" 1
t_contains "$t_out" "lib/polyfill-someFeature.sh names no upstream issue"
t_lacks "$t_out" "lib/polyfill-someFeature.sh names no upstream pull request"
t_contains "$t_out" "lib/polyfill-otherFeature.sh names no upstream pull request"
t_contains "$t_out" "lib/polyfill-otherFeature.sh has no test/polyfill_otherFeature_test.sh"
t_contains "$t_out" "libexec/run marks a line for missingFeature, which has no lib/polyfill-missingFeature.sh"
t_lacks "$t_out" "docs/"

t_case "a bad argument is 64"
for args in "" "bogus" "list extra" "expired" "prune $T/nope" "--root $T/nope list"; do
  t_run "'$script' $args"
  t_eq "$t_rc" 64
done

t_case "this checkout's polyfills pass check"
t_run "'$script' check"
t_eq "$t_rc" 0
t_eq "$t_out" ""

t_done
