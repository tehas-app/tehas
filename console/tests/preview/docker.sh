# The docker of the preview tests, the body test/preview_lib.sh gives
# `t_stub docker`. It keeps what it was told in $T/fx/docker: container-<n>
# holds the commit of a running preview, image-<n> the id of its image. The id
# of the container of <n> is ab and <n> with zeros between to Docker's 64 hex,
# and abcafe with zeros after it for main. fx/extra-containers has more lines
# of a listing, fx/names-<n> is what Docker calls the container of <n> when
# that is not its one name, an id in fx/rm-fails is a container Docker does not
# remove, and fx/label-repo is how the tehas.repo label of the containers is
# written.

d=$T/fx/docker; mkdir -p "$d"
after() { local want=$1; shift; while [ $# -gt 1 ]; do [ "$1" = "$want" ] && { echo "$2"; return; }; shift; done; }
[ ! -e "$T/fx/docker-down" ] || exit 1
case "$1" in
  info) echo /var/lib/docker ;;
  version)
    # The version of the server: fx/docker-version, or one that is new enough.
    case "$*" in *"--format {{.Server.Version}}"*)
      [ ! -e "$T/fx/version-fails" ] || exit 1
      cat "$T/fx/docker-version" 2>/dev/null || echo 29.1.3 ;;
    esac ;;
  network)
    # net-missing: no such network until it is created. net-internal,
    # net-driver, net-ipv6 and net-gateway are what an inspect says of it,
    # each only when it is asked.
    [ "$2" != create ] || { rm -f "$T/fx/net-missing"; exit 0; }
    [ "$2" = inspect ] || exit 0
    [ ! -e "$T/fx/net-missing" ] || exit 1
    case "$*" in
      *--format*)
        [ ! -e "$T/fx/net-inspect-fails" ] || exit 1
        out=""
        case "$*" in *"{{.Internal}}"*) out="$out$(cat "$T/fx/net-internal" 2>/dev/null || echo true) " ;; esac
        case "$*" in *"{{.Driver}}"*) out="$out$(cat "$T/fx/net-driver" 2>/dev/null || echo bridge) " ;; esac
        case "$*" in *"{{.EnableIPv6}}"*) out="$out$(cat "$T/fx/net-ipv6" 2>/dev/null || echo false) " ;; esac
        case "$*" in *"{{index .Options \"com.docker.network.bridge.gateway_mode_ipv4\"}}"*)
          out="$out$(cat "$T/fx/net-gateway" 2>/dev/null || echo isolated)" ;;
        esac
        echo "$out" ;;
    esac ;;
  build)
    # A build makes the image the tag is left pointing at. The id is made from
    # the bytes of the archive this build was given, so the image of a commit
    # has one id however often it is built, and a build of the same commit
    # leaves the tag where it was. A build of another commit moves the tag, and
    # the image it held is left with none: one line of untagged-<n> per such
    # image, as Docker keeps a dangling image until something removes it.
    cat >"$d/context.tar"
    [ ! -e "$T/fx/build-fails" ] || { echo "step 2 failed" ; exit 1; }
    tag=$(after -t "$@"); n=${tag#tehas-preview:}
    # The hash is of the whole file and read before anything is written: the
    # handler of a build is on the pipe that takes the archive in, so a command
    # of it that ran in a subshell of that pipe would lose the directory this
    # stub keeps its answers in, which is a name the main shell made. image_of
    # in mod.rs is the same bytes, so a case can name the id of a build
    # before it runs.
    id="sha256:$(sha256sum <"$d/context.tar" | cut -c1-12)"
    was=$(cat "$d/image-$n" 2>/dev/null || true)
    [ -z "$was" ] || [ "$was" = "$id" ] || echo "$was" >>"$d/untagged-$n"
    echo "$id" >"$d/image-$n"
    echo "$id" ;;
  run)
    [ ! -e "$T/fx/run-fails" ] || exit 1
    name=$(after --name "$@")
    printf "%s\n" "$@" >"$T/calls/docker.run.${name#tehas-preview-}"
    printf "%s\n" "$@" | sed -n "s/^tehas\.sha=//p" >"$d/container-${name#tehas-preview-}"
    echo cid ;;
  inspect)
    # What a container says of itself a moment after its start: fx/settle, or
    # running and never restarted. Only to the question the command asks.
    case "$*" in *"--format {{.State.Running}} {{.RestartCount}} tehas-preview-"*)
      cat "$T/fx/settle" 2>/dev/null || echo "true 0" ;;
    esac ;;
  logs) echo "sh: cannot create /srv/x: Read-only file system" ;;
  rm)
    for a in "$@"; do
      ! grep -q -x -e "$a" "$T/fx/rm-fails" 2>/dev/null || exit 1
      case "$a" in
        tehas-preview-*) rm -f "$d/container-${a#tehas-preview-}" ;;
        abcafe0*) rm -f "$d/container-main" ;;
        ab[0-9]*) rm -f "$d/container-$((10#${a#ab}))" ;;
        -*) ;;
        *) [ ! -f "$T/fx/extra-containers" ] || sed -i "/^$a /d" "$T/fx/extra-containers" ;;
      esac
    done ;;
  ps)
    case "$*" in
      *--quiet*)
        n=$(printf "%s" "$*" | sed -n "s/.*name=\^tehas-preview-\([0-9a-z]*\)\\$.*/\1/p")
        sha=$(printf "%s" "$*" | sed -n "s/.*label=tehas\.sha=\([0-9a-f]*\).*/\1/p")
        # With no commit asked for, any container of that name is listed.
        case "$*" in
          *label=tehas.sha=*) [ -f "$d/container-$n" ] && [ "$(cat "$d/container-$n")" = "$sha" ] && echo cid ;;
          *) [ -f "$d/container-$n" ] && echo cid ;;
        esac ;;
      *)
        for f in "$d"/container-*; do
          [ -f "$f" ] || continue
          n=${f##*/container-}; name=tehas-preview-$n
          [ ! -f "$T/fx/names-$n" ] || name=$(cat "$T/fx/names-$n")
          if [ "$n" = main ]; then id=abcafe$(printf "%058d" 0); else id=$(printf "ab%062d" "$n"); fi
          echo "$id $n $(cat "$T/fx/label-repo" 2>/dev/null || echo acme/widget) $name"
        done
        cat "$T/fx/extra-containers" 2>/dev/null ;;
    esac ;;
  image)
    case "$2" in
      inspect)
        ref=${@: -1}
        case "$*" in
          # The volumes an image declares: fx/volumes, or none. fx/volumes-fails
          # is a question Docker does not answer.
          *"{{json .Config.Volumes}}"*)
            [ ! -e "$T/fx/volumes-fails" ] || exit 1
            cat "$T/fx/volumes" 2>/dev/null || echo null ;;
          *tehas.repo*) grep "^$ref " "$T/fx/image-repos" 2>/dev/null | cut -d" " -f2 || true ;;
          *) cat "$d/image-${ref#tehas-preview:}" 2>/dev/null || exit 1 ;;
        esac ;;
      ls)
        for f in "$d"/image-*; do
          [ -f "$f" ] || continue
          n=${f##*/image-}; echo "$(cat "$f") tehas-preview:$n"
        done
        # A dangling image keeps the labels of its build, and Docker lists it
        # by default, as <none>:<none>.
        cat "$d"/untagged-* 2>/dev/null | sed "s/\$/ <none>:<none>/"
        cat "$T/fx/extra-images" 2>/dev/null ;;
      # An image goes by its tag or by its id: the second is how a build whose
      # container stopped takes its own image away, and how the sweep takes an
      # image of a preview that is gone. Only a digest of the shape Docker gives
      # is a name to remove; any other word is refused and recorded, so a case can
      # see what was handed to it instead of watching nothing change. A -f before
      # the name is an option and is no image: `stop` asks for that form itself.
      # By its id a dangling image goes too: its line leaves untagged-<n>.
      # The words after "image rm", so that the two words of the subcommand are
      # no image of their own: the tag and the id arms ignore them by luck alone,
      # and only the arm that records what it did not know would say so.
      rm) for a in "${@:3}"; do
           case "$a" in
             -*) ;;
             tehas-preview:*) rm -f "$d/image-${a#tehas-preview:}" ;;
             sha256:[0-9a-f][0-9a-f]*) for f in "$d"/image-*; do
               [ -f "$f" ] || continue
               [ "$a" = "$(cat "$f" 2>/dev/null)" ] || continue
               rm -f "$f"
             done
             for f in "$d"/untagged-*; do
               [ -f "$f" ] || continue
               grep -vx -e "$a" "$f" >"$f.left" || true
               mv "$f.left" "$f"
               [ -s "$f" ] || rm -f "$f"
             done ;;
             # The escape of the newline is read by the shell of the stub, as in
             # the printf of the run arm above.
             *) printf "%s\n" "$a" >>"$T/fx/docker-unexpected" ;;
           esac
         done ;;
    esac ;;
esac
exit 0

