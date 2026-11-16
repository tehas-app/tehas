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

t_case "the file ends with a newline and holds no carriage return"
t_eq "$(tail -c 1 "$file" | od -An -c | tr -d ' ')" '\n'
t_eq "$(grep -c $'\r' "$file")" 0

t_done
