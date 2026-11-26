### Changes

- `tehas state unpack` and `tehas state pack` no longer leave their working directory behind when a TERM, HUP or INT comes the moment it is made.
- One change is on purpose: a TERM, HUP or INT that comes after a pack or unpack has finished with its directory now ends the command with 128 and the signal's number, as the default does (HUP gives 129). Before, such a signal was dropped.
