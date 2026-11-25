### Changes

- A refine that links its new issue as a part of the original reads the new issue's parent first. A part already under the original is done with nothing sent; a part of another issue, in this repository or another, is never moved, and the poll says the link failed in `logs/dispatch.log` (#3202, #3220).
