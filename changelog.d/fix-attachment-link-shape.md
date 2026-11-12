### Changes

- Attachment links are taken only from GitHub's attachment folders. A part of
  a link's path that is `.` or `..`, or holds an escaped `.`, `/` or `\`, ends
  the link there, curl asks for the path as written, and the bot's token goes
  only with a whole attachment link. This holds for the images and files a
  cycle is given and for the Protolab's files.
