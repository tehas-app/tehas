# The gh of the preview tests, the body test/preview_lib.sh gives `t_stub gh`:
# it answers from fixture files under $T/fx. A file <name>.rc is an exit status.

# The repository itself: gh-down models the pull request calls failing, repo-fails
# the repository check failing (a token that lost access gets a 404 for everything),
# and repo-body is an answer of exit 0 that says something else.
if [ "$1 $2" = "api repos/acme/widget" ]; then
  [ ! -e "$T/fx/repo-fails" ] || { echo "gh: Not Found (HTTP 404)" >&2; exit 1; }
  [ ! -e "$T/fx/repo-body" ] || { cat "$T/fx/repo-body"; exit 0; }
  # fx/default-branch names the default branch; without it the answer has none,
  # and the run has no main preview to speak of.
  jq -cn --arg b "$(cat "$T/fx/default-branch" 2>/dev/null)" "{full_name: \"Acme/Widget\"} + (if \$b == \"\" then {} else {default_branch: \$b} end)"
  exit 0
fi
# The head of a branch is what the upstream has for it. fx/branch-fails is a call
# that fails.
case "$1 $2" in "api repos/acme/widget/branches/"*)
  [ ! -e "$T/fx/branch-fails" ] || { echo "gh: connection refused" >&2; exit 1; }
  jq -cn --arg s "$(git -C "$T/upstream" rev-parse --verify "refs/heads/${2#repos/acme/widget/branches/}")" "{commit: {sha: \$s}}"
  exit 0 ;;
esac
[ ! -e "$T/fx/gh-down" ] || { echo "gh: connection refused" >&2; exit 1; }
# fx/slow holds a call about one pull request until fx/go is there, ten seconds
# at most, so that a test can stop the command while it works. fx/held says a
# call is being held.
[ ! -e "$T/fx/slow" ] || { : >"$T/fx/held"; for _ in $(seq 200); do [ ! -e "$T/fx/go" ] || break; /bin/sleep 0.05; done; }
case "$1 $2" in
  "api repos/acme/widget/pulls?"*)
    # The run reads the open pull requests in pages. A fixture that names page
    # 1 answers the paging itself: a page it does not have is where GitHub ends.
    p=$(printf "%s" "$2" | sed -n "s/.*[&?]page=\([0-9][0-9]*\).*/\1/p")
    if [ -f "$T/fx/pulls-1.json" ]; then
      # The call for a page is written down as the run makes it, so that a case
      # counts the pages of the list the run read and not the files of fx.
      echo "api repos/acme/widget/pulls?per_page=100&page=${p:-1}" >>"$T/calls/gh-page"
      cat "$T/fx/pulls-${p:-1}.json" 2>/dev/null || echo "[]"
      exit 0
    fi
    cat "$T/fx/pulls.json" 2>/dev/null || exit 1 ;;
  # One pull request. fx/pull-<n>.rc is a call that fails, and the answer is
  # what GitHub has as its description, which is where a notice goes.
  "api repos/acme/widget/pulls/"*)
    n=${2#repos/acme/widget/pulls/}
    [ -f "$T/fx/pull-$n.json" ] || { echo "gh: Not Found (HTTP 404)" >&2; exit 1; }
    [ ! -f "$T/fx/pull-$n.rc" ] || { echo "gh: API rate limit exceeded (HTTP 403)" >&2; exit 1; }
    cat "$T/fx/pull-$n.json" ;;
  # The verdict question of the sweep and the run, answered by the one
  # helper the preview stubs share (gh-verdict.sh, written into the bin).
  "api graphql")
    sh "$T/bin/gh-verdict" "$@" ;;
  "api -X")
    # fx/write-fails is a write GitHub does not take. One line a write,
    # whatever text the description has in it. A write of a description is kept
    # as the bytes the command sent, and becomes what GitHub answers for that
    # pull request from then on: fx/desc-<n>.
    [ ! -e "$T/fx/write-fails" ] || exit 1
    printf "%s" "$*" | tr "\n" " " >>"$T/calls/gh-writes"
    echo >>"$T/calls/gh-writes"
    case "$3 $4" in
      "PATCH repos/acme/widget/pulls/"*)
        n=${4#repos/acme/widget/pulls/}
        # The body is the argument at the place the command puts it: -f and
        # then body=<text>. Text of a description that begins with body= must
        # not be taken for the flag.
        if [ "${5:-}" != -f ]; then
          echo "gh stub: a write without -f where the command puts it" >&2
          exit 1
        fi
        case "${6:-}" in
          body=*) printf %s "${6#body=}" >"$T/fx/desc-$n" ;;
          *)
            echo "gh stub: a write without body=<text> where the command puts it" >&2
            exit 1 ;;
        esac
        jq -c --rawfile b "$T/fx/desc-$n" ".body = \$b" "$T/fx/pull-$n.json" >"$T/fx/x" &&
          mv "$T/fx/x" "$T/fx/pull-$n.json"
        # GitHub answers a write with the pull request as it has it then.
        cat "$T/fx/pull-$n.json" ;;
    esac ;;
esac
