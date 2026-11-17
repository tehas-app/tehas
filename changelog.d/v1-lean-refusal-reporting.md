### Changes

- The park of a lean start that `tehas work` refused now names what was not supported and who is at fault. When the sandbox image's pi-rukas lacks it, the comment says to rebuild the image to keep the issue on lean, or to take `tehas:lean` off to run it on classic, said as a change of driver. When tehas passed `work-lean` a flag it never takes, the comment says this is a fault in tehas and asks for an update, not for the label to come off; a Retry of that park is not held for the label.
- `dispatch.log` has a start's line before the lines of the cycle it starts. Before, a start refused at once could log `done #N` before `start #N`.
- A start refused before its runner ran logs one line that names the refusal, in place of the warnings about a missing result and progress.
- A classic start refused by `tehas work`'s check now ends as `refused`, as a refused lean start does, and not as `unknown`.
- `/health` and `tehas status` say when the last start was refused, with a few words of why, rather than nothing: `The last start was refused before anything ran: #7, the sandbox image lacks what it was given.` The status does not fail on it.
