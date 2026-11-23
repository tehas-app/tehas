### Changes

- A `state/account-ids` file that cannot be read with certainty now stops `tehas update` and `install.sh` with 78, names the line and what is wrong, and is left as it is; until it is repaired no listed login counts. A file whose lines only end in spaces or carriage returns is checked as it reads and written back plain.
