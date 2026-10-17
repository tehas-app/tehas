#!/usr/bin/env bash
# Make the zips test/protolab_test.sh attaches to a ticket. Run by hand; the
# zips it writes are committed, so the tests need no zip tool.
#
#   good.zip   a prototype: index.html with the script and data it refers to,
#              a prototype.md, and a pdf the lab does not host
#   evil.zip   ../escape.txt, /abs.txt, a link to /etc/passwd, and ok.txt
#   bomb.zip   big.txt, 11 MB of zeros
#   many.zip   45 small text files
#   odd.zip    names unzip would read as a pattern or an option, a name with
#              a backslash, and one seven levels deep
#   edge.zip   a file with no Unix type bits, two names that clean to one, a
#              file where a folder is needed, a damaged file, a locked file
#   entries.zip  205 small text files
#   sizes.zip  four files of 4 MB of zeros each
#
# Usage: test/fixtures/protolab/make-zips.sh   (exit 0 when every zip is written)
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
# One time on every file, in one zone, so the same script writes the same zips.
export TZ=UTC
same_time() { find "$@" -exec touch -d '2026-10-17 00:00:00' {} +; }
rm -f "$here"/good.zip "$here"/evil.zip "$here"/bomb.zip "$here"/many.zip "$here"/odd.zip \
  "$here"/edge.zip "$here"/entries.zip "$here"/sizes.zip

mkdir -p "$tmp/good/data"
cat >"$tmp/good/index.html" <<'HTML'
<!doctype html>
<title>Good</title>
<script src="app.js"></script>
<p id="out">loading</p>
HTML
printf '%s\n' 'fetch("data/sample.json").then(r => r.json()).then(d => { out.textContent = d.name; });' >"$tmp/good/app.js"
printf '%s\n' '{"name": "sample"}' >"$tmp/good/data/sample.json"
printf '%s\n' '# Prototype' '' 'What this zip holds, for a language model.' >"$tmp/good/prototype.md"
printf '%s\n' '%PDF-1.4' >"$tmp/good/notes.pdf"
same_time "$tmp/good"
(cd "$tmp/good" && zip -q -X "$here/good.zip" index.html app.js data/sample.json prototype.md notes.pdf)

# zip itself strips ../ and a leading /, so these entries are written directly.
python3 -I - "$here/evil.zip" <<'PY'
import sys, zipfile
def entry(name, mode):
    info = zipfile.ZipInfo(name, (2026, 10, 17, 0, 0, 0))
    info.create_system = 3
    info.external_attr = mode << 16
    return info
with zipfile.ZipFile(sys.argv[1], "w") as z:
    z.writestr(entry("../escape.txt", 0o100644), "escaped\n")
    z.writestr(entry("/abs.txt", 0o100644), "absolute\n")
    z.writestr(entry("link", 0o120777), "/etc/passwd")
    z.writestr(entry("ok.txt", 0o100644), "ok\n")
PY

head -c $((11 * 1024 * 1024)) /dev/zero >"$tmp/big.txt"
same_time "$tmp/big.txt"
(cd "$tmp" && zip -q -X -9 "$here/bomb.zip" big.txt)

mkdir -p "$tmp/many"
for i in $(seq -w 1 45); do printf '%s\n' "$i" >"$tmp/many/f$i.txt"; done
same_time "$tmp/many"
(cd "$tmp/many" && zip -q -X "$here/many.zip" ./*.txt)

python3 -I - "$here/odd.zip" <<'PY'
import sys, zipfile
with zipfile.ZipFile(sys.argv[1], "w") as z:
    for name in ["-x.txt", "a[1].txt", "a1.txt", "st*r.txt", "q?.txt", "qq.txt",
                 "back\\slash.txt", "1/2/3/4/5/6/deep.txt", "1/2/3/4/5/ok.txt"]:
        info = zipfile.ZipInfo(name, (2026, 10, 17, 0, 0, 0))
        info.create_system = 3
        info.external_attr = 0o100644 << 16
        z.writestr(info, "content of " + name + "\n")
PY

python3 -I - "$here/edge.zip" <<'PY'
import sys, zipfile
def entry(name, mode):
    info = zipfile.ZipInfo(name, (2026, 10, 17, 0, 0, 0))
    info.create_system = 3
    info.external_attr = mode << 16
    return info
path = sys.argv[1]
with zipfile.ZipFile(path, "w") as z:
    z.writestr(entry("plain.txt", 0o644), "plain\n")
    z.writestr(entry("a b.txt", 0o100644), "first\n")
    z.writestr(entry("a-b.txt", 0o100644), "second\n")
    z.writestr(entry("d.txt", 0o100644), "d\n")
    z.writestr(entry("d.txt/inner.txt", 0o100644), "inner\n")
    z.writestr(entry("bad.txt", 0o100644), "BADCONTENT\n")
    z.writestr(entry("locked.txt", 0o100644), "LOCKEDDATA\n")
data = bytearray(open(path, "rb").read())
# Damage bad.txt, so its checksum fails.
at = data.find(b"BADCONTENT")
data[at] = ord("X")
# Mark locked.txt as encrypted, in its local header and its central record.
for sig, flag, name in ((b"PK\x03\x04", 6, 30), (b"PK\x01\x02", 8, 46)):
    at = data.find(sig)
    while at >= 0:
        if data[at + name:at + name + 10] == b"locked.txt":
            data[at + flag] |= 1
        at = data.find(sig, at + 4)
open(path, "wb").write(bytes(data))
PY

mkdir -p "$tmp/entries"
for i in $(seq -w 1 205); do printf '%s\n' "$i" >"$tmp/entries/e$i.txt"; done
same_time "$tmp/entries"
(cd "$tmp/entries" && zip -q -X "$here/entries.zip" ./*.txt)

mkdir -p "$tmp/sizes"
for f in a b c d; do head -c $((4 * 1024 * 1024)) /dev/zero >"$tmp/sizes/$f.txt"; done
same_time "$tmp/sizes"
(cd "$tmp/sizes" && zip -q -X -9 "$here/sizes.zip" a.txt b.txt c.txt d.txt)
