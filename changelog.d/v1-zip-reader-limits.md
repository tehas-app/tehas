### Changes

- An archive attached to an issue is read more strictly: an entry that claims more than the room left is refused before any of it is unpacked, an entry whose place in the archive cannot be worked out is left out as damaged, an unpacked file never replaces one already in the directory, and a retry holds each file of an archive to its own bytes (#2248, #2258).
