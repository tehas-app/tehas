#!/usr/bin/env bash
# Tests for the settings table of the manual: every setting of the loader's
# table in console/core/src/settings/rules.rs takes a row in the Instance
# settings table of host/docs/tools.md, and every row of that table names a
# setting the loader, the host or pi-rukas reads. A setting with no row is one
# an operator reads about in prose and cannot look up where the other settings
# are looked up (#1698).
# shellcheck disable=SC2016  # the awk programs and the patterns are literal
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

rules=$T_ROOT/console/core/src/settings/rules.rs
tools=$T_ROOT/host/docs/tools.md
load=$T_ROOT/console/core/src/settings/load.rs

# The body of the loader's table: the lines between `pub const TABLE` and the
# `];` that closes it. The tests of the rules that sit below the table in the
# same file name the settings too, so the body is cut before it is read.
body() { sed -n '/^pub const TABLE: &\[Setting\] = &\[$/,/^\];$/p' "$rules"; }

# The settings of TABLE, one name per line. A row that opens with always,
# given, given_or or required names its setting on its own line; a row whose
# name is too long for one line names it on the line after, where the name is
# the only thing on it. Two rows name no TEHAS_ string of their own and give
# nothing: the row of PI_RUKAS_FOLLOW, which is no TEHAS_ name, and the row of
# the issue types, which names the constant of kinds.rs rather than a string.
loader_names() {
  body | sed -nE 's/^ *(always|given|given_or|required|row)\("(TEHAS_[A-Z0-9_]*)".*/\2/p'
  body | awk '
    /^ *(always|given|given_or|required|row)\($/ { getline; if (match($0, /^[ ]*"TEHAS_[A-Z0-9_]*",$/)) print substr($0, RSTART + 1, RLENGTH - 3)
  }' | tr -d '", ' | grep 'TEHAS_'
}

# The names of a setting anywhere in the body of the table, each once. The
# count of them is what the parse of loader_names must give: every row of the
# table names its setting by a string somewhere in it, and the two rows that
# name none of tehas, the one of PI_RUKAS_FOLLOW and the one of the issue types
# in kinds.rs, give nothing to either side and so cannot make the two differ.
body_names() {
  body | grep -oE '"TEHAS_[A-Z0-9_]+"' | tr -d '"' | sort -u
}

# The names a load keeps out of the caller's shell (KEPT in load.rs): settings
# of the host and of the commands beside it, read from the environment with no
# row of TABLE. A row for one is right and its absence is no failure.
kept_names() {
  sed -n '/^const KEPT: &\[&str\] = \[$/,/^\];$/p' "$load" |
    sed -n 's/^ *"\(TEHAS_[A-Z0-9_]*\)".*/\1/p'
}

# The names the manual's Instance settings table gives: the backticked words of
# a row's first cell, the only cell that names a setting. Two cells away the
# Meaning cell names other settings, as the meaning of `TEHAS_CYCLE_STOP_FREE`
# names `TEHAS_SCRATCH_DIR`, and the Default cell holds words and digits, so a
# word is taken only while it stands at the front of the row and looks like a
# name: letters, digits and an underscore, and at the end a `<ROLE>` of the two
# rows of the PI_ENSEMBLE_ family. The words of the one row of two settings,
# `TEHAS_ANALYTICS_SRC`, `_SITE`, are both such words, and the second is written
# with `TEHAS_ANALYTICS_` taken off, so that one word is named here for its
# whole name. The comma between them stands outside both pairs of backticks, so
# it is the field between them and reaches this parse as nothing; a comma inside
# one pair, as the one of `tehas:ready`, `tehas:plan` in the meaning of another
# row, is part of that word, sits past the first `|` of the row, and is taken
# for no name.
manual_names() {
  awk -F'`' '
    /^## Instance settings$/ { on = 1; next }
    on && /^## / { exit }
    on && /^\| / { for (i = 2; i <= NF && ($i ~ /^[A-Za-z_]+,?$/ || $i ~ /^[A-Za-z_]+<[^>]*>$/); i += 2) printf "%s\n", $i }
  ' "$tools" | sed 's/,$//' |
    sed -n -e '/^_SITE$/s//TEHAS_ANALYTICS_SITE/p' \
      -e '/^TEHAS_[A-Z0-9_]*$/p' -e '/^PI_ENSEMBLE_[A-Z0-9_]*\(<[^>]*>\)?$/p' -e '/^PI_ENSEMBLE_[A-Z_]*$/p' | sort -u
}

# has <name> <list>: the list holds that name on a line of its own.
# A here-string, not a pipe: grep -q stops at the first match, and under
# pipefail the printf it leaves writing would fail the test on a match.
has() { grep -qxF "$1" <<<"$2"; }

loader=$(loader_names | sort -u)
body=$(body_names)
body_count=$(printf '%s\n' "$body" | grep -c .)
kept=$(kept_names)
manual=$(manual_names)

t_case "the loader's table is read as the loader writes it"
# The count of names is the count of rows less the two that name no TEHAS_
# string, so a parse that reads no row, or reads the tests below the table,
# fails here instead of passing every case after it. Every name the body of the
# table holds is among them, so a row whose name sits on a line this parse does
# not read fails too.
t_eq "$(printf '%s\n' "$loader" | grep -c .)" "$body_count"
missing_body=
for name in $body; do
  has "$name" "$loader" || missing_body="$missing_body $name"
done
[ -z "$missing_body" ] || t_bad "a name of the table's body this parse did not read:$missing_body"
t_contains "$loader" TEHAS_REPO
t_contains "$loader" TEHAS_REVIEW_REPO
t_contains "$loader" TEHAS_UPDATE_FROM
t_contains "$loader" TEHAS_TICKET_CONCURRENCY

t_case "the manual's table is read, and the short name of the analytics row counts"
t_contains "$manual" TEHAS_REPO
t_contains "$manual" TEHAS_ANALYTICS_SRC
t_contains "$manual" TEHAS_ANALYTICS_SITE
t_contains "$manual" PI_ENSEMBLE_SUBAGENT_MODEL

t_case "every setting of the loader's table has a row in the manual"
missing=
for name in $loader; do
  has "$name" "$manual" || missing="$missing $name"
done
[ -z "$missing" ] || t_bad "no row in the Instance settings table for:$missing"

t_case "every row of the manual names a setting the loader, the host or pi-rukas reads"
# The rows that name no row of TABLE on purpose, each checked by a case of its
# own below: TEHAS_MILLWRIGHT_LOGIN and TEHAS_WORK_REPO are set by a load from
# the app's login and from TEHAS_REPO; TEHAS_CYCLE_CPUS and TEHAS_CYCLE_CEILING
# are what the poll would count by were `auto` ever usable; TEHAS_DISK_WAIT is
# one of KEPT; TEHAS_ISSUE_TYPES is the row of TABLE whose name is a constant
# of kinds.rs rather than a string in the table's own text.
# The settings a row names with no row of the loader's table behind it, each one
# said in the comment below, and each one a row of the manual's table the case
# after this insists on. The list is a string, not a file: one word per line of
# it, because a name that shares its front with the name before it on the same
# line, as TEHAS_CYCLE_CPUS does with TEHAS_WORK_REPO and TEHAS_CYCLE_CEILING
# with it, would otherwise be a piece of another word and match nothing.
exceptions="
TEHAS_MILLWRIGHT_LOGIN
TEHAS_WORK_REPO
TEHAS_CYCLE_CPUS
TEHAS_CYCLE_CEILING
TEHAS_DISK_WAIT
TEHAS_ISSUE_TYPES
TEHAS_CYCLE_LOGIN
"
extra=""
not_set=""
for name in $manual; do
  has "$name" "$loader" && continue
  has "$name" "$kept" && continue
  case $name in PI_ENSEMBLE_*) continue ;; esac
  has "$name" "$exceptions" && continue
  # An operator has to set no more than these: the two are read from the
  # environment by a poll that could not act by them and so refuses the word
  # that would need them, `auto`, and the rest are named by the comment above.
  extra="$extra $name"
done
[ -z "$extra" ] || t_bad "no setting behind the rows:$extra"

# TEHAS_ANALYTICS_SITE is in the loader's table, so the row that names it by the
# short `_SITE` is checked the other way round: with the name taken out of the
# manual's list the case above finds it missing from the manual.
for name in $loader; do
  has "$name" "$manual" || not_set="$not_set $name"
done
[ -z "$not_set" ] || t_bad "the loader takes $not_set and the manual's table gives it no row"

t_case "the rows that name no setting of TABLE are the ones said above"
for name in $exceptions; do
  has "$name" "$manual" || t_bad "$name is no longer a row of the manual's table"
done
# The app names the bot and the millwright, so neither is a setting an instance
# sets: a row for the bot's login, refused with 78 since the factory became one
# app, would tell an operator to set a thing that stops every command.
has TEHAS_BOT_LOGIN "$manual" && t_bad "TEHAS_BOT_LOGIN is a row again: the app names the bot"
has TEHAS_GITHUB_APP_INSTALLATION "$manual" &&
  t_bad "TEHAS_GITHUB_APP_INSTALLATION is a row again: the installation is looked up"

t_case "the five settings #1698 found have rows"
for name in TEHAS_CYCLE_LOGIN TEHAS_SCRATCH_DIR TEHAS_TOP TEHAS_UPDATE_FROM TEHAS_RELEASE_REPO; do
  has "$name" "$manual" || t_bad "no row for $name"
done

# The example instance's optional.env is the reference for the optional
# settings: the Commands page sends an operator to it, so a setting of TABLE
# that an operator may set and that holds no line there is one they cannot look
# up where the other optional settings are looked up (#1768).
optional=$T_ROOT/instances/example/optional.env
instance=$T_ROOT/instances/example/instance.env

# The names optional.env gives a line of, commented out or not: the head of a
# line up to the `=`. The lines of comments that name a setting in their prose
# do not reach this parse, because such a line holds no `=` after the name.
optional_names() {
  sed -nE 's/^#([A-Z][A-Z0-9_]*)=.*/\1/p' "$optional"
}

# The names instance.env sets: the required ones belong there, and one of them
# listed in optional.env as well would tell an operator to copy a line that
# `tehas setup` already writes.
instance_names() {
  sed -nE 's/^([A-Z][A-Z0-9_]*)=.*/\1/p' "$instance"
}

t_case "every setting of TABLE that is not required has a line in optional.env"
# The `when` of each row comes from `console settings rules --json`, the file a
# release carries, so the case reads the loader's own answer rather than a copy
# of it. The required settings are the ones instance.env holds, retired names
# (RETIRED in load.rs) are lines a host must take out and not ones to copy, and
# the PI_ENSEMBLE_* family belongs to pi-rukas, which reads them itself.
if ! t_console; then
  t_skip "no console: $t_console_why"
else
  when=$("$T_CONSOLE" settings rules --json |
    jq -r '.settings[] | select(.when != "required") | .name')
  retired=$(sed -n '/^const RETIRED: \[&str; [0-9]*\] = \[$/,/^\];$/p' "$load" |
    sed -n 's/^ *"\([A-Z][A-Z0-9_]*\)".*/\1/p')
  instance_set=$(instance_names)
  optional_lines=$(optional_names)
  missing=
  for name in $when; do
    has "$name" "$retired" && continue
    has "$name" "$instance_set" && continue
    case $name in PI_ENSEMBLE_*) continue ;; esac
    has "$name" "$optional_lines" || missing="$missing $name"
  done
  [ -z "$missing" ] || t_bad "no line in instances/example/optional.env for:$missing"
fi

t_done
