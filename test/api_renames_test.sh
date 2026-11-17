#!/usr/bin/env bash
# Tests for api/renames-1.0.tsv, the keys and values of the API that 1.0
# renamed or removed: a header and rows of four tab-separated columns, kind,
# old, new and note. A kind is setting, label, command, field, word, tool or
# value, and no old name is listed twice.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

file=$T_ROOT/api/renames-1.0.tsv

t_case "the file starts with its header"
t_eq "$(head -n 1 "$file")" "$(printf 'kind\told\tnew\tnote')"

t_case "every row has four columns, a kind, an old name and a note"
t_eq "$(awk -F '\t' 'NR > 1 && (NF != 4 || $2 == "" || $4 == "") { print NR }' "$file")" ""

t_case "every kind is one the siblings read"
t_eq "$(awk -F '\t' 'NR > 1 && $1 !~ /^(setting|label|command|field|word|tool|value)$/ { print NR }' "$file")" ""

t_case "no old name is listed twice"
t_eq "$(awk -F '\t' 'NR > 1 { print $2 }' "$file" | sort | uniq -d)" ""

t_case "an exit status row names a tehas command, the old status and the new one"
t_eq "$(awk -F '\t' 'NR > 1 && $2 ~ / exit=/ && !($1 == "value" && $2 ~ /^tehas [a-z-]+( --[a-z-]+)? exit=[0-9]+$/ && $3 ~ /^[0-9]+$/) { print NR }' "$file")" ""
t_contains "$(cut -f 2 "$file")" "tehas check-product exit=78"

t_case "every label of 0.30 is a label of host/labels.tsv or a label row"
# The labels of v0.30.6, the last 0.30 release, which 1.0 updates from.
for label in tehas:ready tehas:running tehas:awaiting-merge tehas:shipped tehas:pause \
  needs-human-attention tehas:question tehas:plan tehas:refine tehas:planning tehas:replaced \
  tehas:refined tehas:millwright-escalated tehas:lean tehas:blocked tehas:refreshing \
  tehas:all-hands epic tehas:finish tehas:prototype tehas:improvement cross-project; do
  row=$(awk -F '\t' -v l="$label" '$1 == "label" && $2 == l' "$file")
  kept=$(cut -f 1 "$T_ROOT/host/labels.tsv" | grep -c -x -F -- "$label")
  t_eq "$label: $kept $([ -n "$row" ] && echo row)" "$label: $([ -n "$row" ] && echo "0 row" || echo "1 ")"
done

t_case "the file ends with a newline and holds no carriage return"
t_eq "$(tail -c 1 "$file" | od -An -c | tr -d ' ')" '\n'
t_eq "$(grep -c $'\r' "$file")" 0

t_done
