### Changes

- A refresh whose conflict check judged nothing no longer runs: when the read of
  the merge runs out of time, when git cannot be started, when the request's
  record holds no pull request or a head that is no commit, when git writes no
  tree object, or when it answers a status that is no answer, `tehas work` exits
  78, says which of them in the cycle log and writes no note, so no container
  starts. A merge that is clean runs as before, and so does one made by a git
  that has no such read of its own, with what git said in the log (#2227).
- A product's install, or the sandbox's push check, that cannot open its
  preflight log says that, with the path of the log and the fault of the system,
  and stops the start with 78. Neither ends as exit 127, with the lines of the
  step before it quoted as its own output, and the install's fault is no longer
  told as a sandbox that refused to push (#2227).
