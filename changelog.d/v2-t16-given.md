### Changes

- Images described by the model (`TEHAS_WORK_DESCRIBE`), the text files an issue links (`TEHAS_WORK_FILES`), unpacked zips (`TEHAS_WORK_ZIPS`) and the attachments of comments (`TEHAS_WORK_COMMENTS`) left core. A cycle and a planning session are given the issue's images alone, with `TEHAS_WORK_IMAGES=on`, and the comments of a retry as before. The four return in 2.1 as the `given` plugin, through a hook before a cycle starts. Describe and text files at a planning turn do not come back.
- An instance that still sets one of the four loads: the setting is ignored, with one warning line each.
- The approval record no longer lists `comment_links`. A millwright's note lifts a park's hold only where the instance gives a cycle images.
- `tehas dev --issue` needs `TEHAS_WORK_IMAGES=on` and gives the issue's images alone.

### Adopter action

Remove `TEHAS_WORK_DESCRIBE`, `TEHAS_WORK_FILES`, `TEHAS_WORK_ZIPS` and `TEHAS_WORK_COMMENTS` from the instance. An instance that set `TEHAS_WORK_FILES=on` without `TEHAS_WORK_IMAGES=on` now gives a cycle no directory unless a retry has comments.
