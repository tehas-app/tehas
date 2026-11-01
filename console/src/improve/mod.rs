//! One run of the improvement loop: read the cycles that ended since the last
//! run, ask a model what could be better, and file one issue per finding that
//! is not already in the issue stack.
//!
//!   console improve due <prompt file>
//!   console improve run <prompt file>
//!
//! `due` holds the lock for the sweep and says whether a run has anything to
//! do. `run` is the run. libexec/improve calls them in that order with the
//! slice check between them, so a run with nothing to read starts no container
//! and asks no slice, and one without a usable slice stops before it calls
//! GitHub. The console reads no slice of its own.
//!
//! The findings are about the factory and about pi-rukas, so pi-rukas does not
//! run here: the analyst is plain Pi with one prompt, lib/improve-prompt.md.
//! What it reads holds issue text and model output, which may be written to
//! steer it. So it is given the records read-only and one directory to write
//! to, and nothing else: no token, no repository, no tool that runs a command
//! or opens a connection. This program makes every GitHub call, checks the one
//! file the analyst leaves against a fixed shape, and writes the issue itself.
//! test/improve_run_test.sh pins what the container is given. It runs in
//! tehas.slice, as every sandbox container does, with limits of its own: one
//! CPU, 2g of memory and no swap.
//!
//!   records    $TEHAS_HOME/tmp/improve/window/<issue>-<stamp>, written when a
//!              cycle ends (lib/improve.sh). A run takes the five oldest and
//!              leaves the rest for the next. An analyst stopped at its bound
//!              halves that number for the next run, down to one, in
//!              state/improve.cap; one that finishes in time doubles it again.
//!              What a stopped analyst's file holds is filed, and the records
//!              it does not name go back to the window
//!   filed      at most 3 issues a run, labelled tehas:improvement; none in a
//!              repository where TEHAS_IMPROVE_MAX_OPEN such issues are open,
//!              10 unless the instance says otherwise. A finding of the
//!              category pi-rukas goes to TEHAS_IMPROVE_RUKAS_REPO where that
//!              is set. One of the category boundary is an issue in each, each
//!              naming its own half, linked both ways and labelled
//!              cross-project too; with no repository for pi-rukas, one issue
//!              with both halves. Both repositories are read for duplicates,
//!              and each has that limit to itself. With
//!              TEHAS_IMPROVE_RUKAS_SKIP=1 a finding of the category pi-rukas
//!              is dropped and counted as skipped
//!   issues     each filed issue names the product's issues its cycles worked,
//!              and says what working on it means: a finding about the factory
//!              is built where it is filed, one about pi-rukas is fixed in
//!              pi-rukas and never worked around in another repository. The
//!              line names the other repository when the finding speaks of
//!              issues of one, so a person reading it sees which repository
//!              holds the fix. A filed issue whose text names a repository
//!              other than the one it is filed in also carries cross-project,
//!              which is what the dispatcher holds a cycle back for
//!   rechecked  when one of those issues is worked on again, the analyst says
//!              whether the problem came back, and that is a comment on the
//!              filed issue with the pi-rukas commit the new cycle ran; at most
//!              10 a run. Of a closed issue, the comment says whether the
//!              commit that closed it is in that build, and "not in this
//!              build" rather than "yes" when it is not, or that the build is
//!              unknown. Nothing is closed or reopened
//!   logs       the whole log of each cycle a finding names, gzipped, as assets
//!              of the release tehas-improve-logs; over 5 MB, in numbered pieces
//!   removed    a run's records when it finishes; anything in tmp/improve, and
//!              the per-cycle files and the kept analyst output in logs/, after
//!              seven days
//!
//! Settings, all from the environment libexec/improve loaded:
//! TEHAS_IMPROVE_REPO, the repository issues are filed in.
//! TEHAS_IMPROVE_RUKAS_REPO, optional, the repository for what only a change in
//! pi-rukas can fix. TEHAS_IMPROVE_RUKAS_SKIP, 1 to file no such finding at all.
//! TEHAS_IMPROVE_MAX_OPEN, how many of the loop's issues may be open in one
//! repository. TEHAS_REPO, the product a cycle worked on.
//! PI_ENSEMBLE_SUBAGENT_MODEL and PI_ENSEMBLE_SUBAGENT_PROVIDER, the model that
//! reads.
//!
//! Exit of `due`: 0 there are records to read; 10 nothing to do, or a limit
//! holds runs, or another run is going; 1 and 78 as `run`.
//! Exit of `run`: 0 the run finished, also with an analyst stopped at its bound
//! that left a good file, or there was nothing to do, or a limit holds runs; 1
//! the analyst or GitHub failed and the records were kept; what a failed
//! analyst printed is in logs/improve-analyst-<stamp>.log; 64 usage; 78 a
//! setting is missing.

mod body;
mod check;
