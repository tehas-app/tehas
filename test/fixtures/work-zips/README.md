#!/usr/bin/env bash
# The zip fixtures for the tests of an archive a cycle is given. They are
# written by hand and committed, so the shell tests need no zip tool, as the
# Protolab's do. tmp/issue-41/mkzip.sh in a scratch directory writes them and
# is not part of the tests; re-run it there and copy the three over if one has
# to change.
#
#   good.zip   a.md, img/b.png and c.svg: a text file and an image the cycle is
#              given, and an svg it is not
#   big.zip    five text files of 1048575 bytes each, so the 1 MB of one file
#              holds four of them and the fifth is left out by it
#   evil.zip   ../escape.md, a link to /etc/passwd, and ok.md: the first two
#              write nothing and are left out by their name, the third is taken
