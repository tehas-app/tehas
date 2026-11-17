#!/usr/bin/env bash
# Tests for the proof a real `tehas update` makes that the app's cycle token
# can push: a dry-run push from the product checkout with the token tehasbot
# makes, and a factory that stays drained when it cannot. tehasbot and the
# push itself are stubs; every other git call runs for real.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck disable=SC2154  # t_out is set by test/lib.sh, which the part sources first
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/update_lib.sh
. "$(dirname "$0")/update_lib.sh" || exit 1

product=$th/repos/widget
limit_file=$th/state/limit.json
drain_doc='{"v":1,"cycles":0,"plan":false,"until":null,"reason":"draining"}'
real_git=$(command -v git)
refused="remote: Write access to repository not granted.
fatal: unable to access 'https://github.com/acme/widget.git/': The requested URL returned error: 403"

# git, real but for the proof's push: that one records the token it was given
# and is refused while $T/push-refused holds what the remote says.
t_stub git "case \"\$*\" in
  *'push --dry-run origin HEAD:refs/heads/tehas-preflight')
    printf '%s\\n' \"\${GH_TOKEN:-}\" >\"\$T/push-token\"
    if [ -e \"\$T/push-refused\" ]; then cat \"\$T/push-refused\" >&2; exit 128; fi
    echo 'To https://github.com/acme/widget.git'; exit 0 ;;
esac
exec '$real_git' \"\$@\""

# The host's tehasbot: the app's login, and a cycle token unless $T/no-token
# says what it prints instead.
bot() {
  mkdir -p "$th/bin"
  cat >"$th/bin/tehasbot" <<EOF
#!/usr/bin/env bash
printf '%s\n' "\$*" >>'$T/calls/tehasbot'
case "\$*" in
  'github-token --login') echo 'acme-factory[bot]' ;;
  'github-token --cycle')
    if [ -e '$T/no-token' ]; then cat '$T/no-token' >&2; exit 1; fi
    echo ghs_cycle ;;
  *) exit 64 ;;
esac
EOF
  chmod +x "$th/bin/tehasbot"
}

# A host with the product checkout and tehasbot, drained or running.
host() { # drained|running
  instance
  platform 0
  bot
  rm -f "$T/push-refused" "$T/push-token" "$T/no-token"
  "$real_git" init -q --initial-branch=main "$product"
  "$real_git" -C "$product" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m first
  "$real_git" -C "$product" remote add origin "$T/product-origin"
  if [ "$1" = drained ]; then
    mkdir -p "$th/state"
    printf '%s\n' "$drain_doc" >"$limit_file"
  fi
}

t_case "an update proves the cycle token pushes, then says the drained factory is ready to clear"
host drained
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "Cycle token:  OK (acme-factory[bot] can push to origin)"
t_eq "$(printf '%s\n' "$t_out" | tail -n 1)" "the factory is drained: tehas limit clear starts the factory, and this version has run no cycle yet"
t_eq "$(cat "$T/push-token")" ghs_cycle
t_eq "$(t_calls tehasbot)" "github-token --login
github-token --cycle"
t_contains "$(t_calls git)" "-C $product -c credential.helper= -c"
t_lacks "$(t_calls git)" ghs_cycle

t_case "a cycle token the remote refuses fails the update, names the account and git's words, and the factory stays drained"
host drained
printf '%s\n' "$refused" >"$T/push-refused"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "Cycle token:  CANNOT PUSH (acme-factory[bot] was refused a dry-run push to origin)"
t_contains "$t_out" "    remote: Write access to repository not granted."
t_contains "$t_out" "The requested URL returned error: 403"
t_contains "$t_out" "Install the app on acme/widget"
t_eq "$(printf '%s\n' "$t_out" | tail -n 1)" "the factory stays drained: every cycle would stop at the sandbox's push check; fix the app, run tehas update --check, then tehas limit clear"
t_lacks "$t_out" "the factory is drained:"
t_eq "$(cat "$limit_file")" "$drain_doc"
# What install.sh installed stays installed: the proof comes after it.
t_eq "$(t_calls install)" "install "

t_case "a refused cycle token on a running factory fails the update and drains nothing"
host running
printf '%s\n' "$refused" >"$T/push-refused"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "Cycle token:  CANNOT PUSH"
t_contains "$t_out" "the factory is not drained: each start stops at the sandbox's push check until the token can push; tehas drain holds it"
t_eq "$([ -e "$limit_file" ] && echo there)" ""

t_case "no cycle token from the app fails the update with tehasbot's words and pushes nothing"
host drained
echo "tehas: GitHub's answer for the app's token held no token and expiry" >"$T/no-token"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "Cycle token:  CANNOT PUSH (the app made no cycle token)"
t_contains "$t_out" "    tehas: GitHub's answer for the app's token held no token and expiry"
t_eq "$([ -e "$T/push-token" ] && echo pushed)" ""
t_contains "$t_out" "the factory stays drained:"

t_case "a host with no product checkout proves nothing and asks tehasbot nothing"
host drained
rm -rf "$product"
t_run "$update"
t_eq "$t_rc" 0
t_lacks "$t_out" "Cycle token:"
t_eq "$(t_calls tehasbot)" ""

t_case "--check and --pending push nothing themselves: the check's report is install.sh's"
host running
printf '%s\n' "$refused" >"$T/push-refused"
t_run "$update --check"
t_eq "$t_rc" 0
t_run "$update --pending"
t_eq "$t_rc" 0
t_eq "$([ -e "$T/push-token" ] && echo pushed)" ""

t_done
