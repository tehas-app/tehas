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
#
# Usage: test/fixtures/protolab/make-zips.sh   (exit 0 when all four are written)
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
rm -f "$here"/good.zip "$here"/evil.zip "$here"/bomb.zip "$here"/many.zip "$here"/odd.zip

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
(cd "$tmp" && zip -q -X -9 "$here/bomb.zip" big.txt)

mkdir -p "$tmp/many"
for i in $(seq -w 1 45); do printf '%s\n' "$i" >"$tmp/many/f$i.txt"; done
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
