# ADR: the review severity policy (MEDIUM and above)

**Status**: Accepted, 2026-10-14, and open until a person applies one line of
`AGENTS.md`. Rule 7 gives that file to a person, so this record and the pages in
`host/docs/` are the cycle's part and the sentence is the person's. Until it
lands the policy judge keeps answering `unstated`, and the bar reaches a cycle as
pi-rukas's default rather than as this repository's answer.

## Context

The six-pass review of a cycle gives each finding a severity: CRITICAL, HIGH,
MEDIUM or LOW. What the lens decides is the severity of a finding. Which
severity is serious enough to stop a change from being offered for merge is a
different question, and pi-rukas treats it as the project's to answer. Before
each review it asks a policy judge to read the bar out of the product
repository's doctrine (`AGENTS.md` or `CLAUDE.md`, read at the commit the cycle
started from), and resolves it in this order:

1. `PI_ENSEMBLE_REVIEW_THRESHOLD`, when the instance sets it,
2. the severity named by a sentence the judge quotes from doctrine,
3. MEDIUM, the shipped default.

This repository states no bar. Its only merge rule is "Nothing merges on its
own" (rule 9). In cycle `97-261013-125428`, 2026-10-13, the judge answered
`unstated` twice and the driver logged:

```
work-driver: policy — unstated (denied)
work-driver: lens-review — blocking severity MEDIUM
```

Nothing was denied; that line is the wording of a shared policy trace, and the
repository simply never said. MEDIUM was applied as the bar, and in a cycle
whose simplicity lens reported 6 MEDIUM and 3 LOW findings it drove two fix
rounds of 1979 s and 1725 s: 3704 s, about a quarter of that cycle.

So the bar this factory merges behind is set by an upstream default. It moves
when pi-rukas moves that default, with no decision recorded here, and an
operator reading `policy — unstated (denied)` looks for a refusal that never
happened.

## Decision

This repository blocks at MEDIUM severity and above. A finding at MEDIUM, HIGH
or CRITICAL is fixed before the pull request is opened. Only LOW findings may
be deferred, as notes on the pull request. A CRITICAL finding blocks whatever
the bar says: no repository can make one advisory.

The bar belongs in `AGENTS.md`, which is what pi-rukas reads. That line is a
person's to add, and it is the step this cycle cannot take; see *Applying the
sentence* below. This record and the pages in `host/docs/` mirror the decision
for people; a mirror is not doctrine and changes no verdict on its own.

## Applying the sentence (a person's step)

Add this to `AGENTS.md`, as a rule of its own in the hard rules list, next to the
rule that nothing merges on its own:

```
This repository blocks at MEDIUM severity and above.
```

Three things decide whether that line works or is a silent no-op.

- **The relation, not the list.** pi-rukas takes the bar from the sentence the
  judge quotes, and reads the relation in it: "at X and above", "X or higher",
  "X and up". `This repository blocks at MEDIUM severity and above.` gives
  MEDIUM. `A finding at MEDIUM, HIGH or CRITICAL is fixed before the pull request
  is opened. Only LOW findings may be deferred.` gives nothing, and the cycle
  takes the default again. Both read as this bar to a person.
- **One sentence carries it.** The other two sentences of the Decision are safe
  to paste under the same heading: the bar comes from the sentence the judge
  quotes, and the one above carries it. They change no answer.
- **Doctrine in its own voice.** The quote is checked against the file word for
  word, and text that only appears inside a code fence or as a quotation is read
  as an example, not as a rule. The line goes in as ordinary text under a
  heading.

pi-rukas reads `AGENTS.md` first and `CLAUDE.md` after it. This repository has no
`CLAUDE.md`, so the line goes in `AGENTS.md`.

## Rationale

- **It records what already happens.** MEDIUM and above is the bar in force
  today, so the verdict becomes real without changing any cycle's behaviour.
  The change is that the answer comes from this repository rather than from a
  default that belongs to pi-rukas.
- **A bar has to be quoted to count.** A project may tighten the bar freely;
  loosening it is honoured only with a sentence that verifies against the file.
  Recording the bar here means an operator can see, in one line of `AGENTS.md`,
  what stops a merge.
- **Doctrine is read at the base commit.** A cycle cannot lower its own bar
  mid-run by editing `AGENTS.md`: the edit ships and is flagged to a reviewer,
  but the cycle's doctrine is read from the commit it started from. That is why
  rule 7 assigns this sentence to a person: the cycle's own edit would be inert
  for its own gate anyway.

## Alternatives considered

- **Block at HIGH and above only.** Cheaper: most of the 3704 s above went into
  fixing MEDIUM findings. Refused for now because it loosens the bar, needs a
  verified citation to be honoured at all, and nothing measured here says those
  fixes were wasted. Revisit when a cycle is measured in which MEDIUM findings
  were fixed and the change they produced was shown to be worth nothing.
- **A setting of tehas' own** (`TEHAS_REVIEW_THRESHOLD`, translated in
  `lib/config.sh`). Refused: it would touch `lib/config.sh`, `lib/sandbox.sh`
  and the pinned list of container variables in `test/work_test.sh`, which is
  rule 4 territory, for no gain. `lib/sandbox.sh` already passes any
  `PI_ENSEMBLE_*` line from `instance.env` to a cycle's container, apart from
  the names tehas owns (tehas reads two of them and sets the rest itself; the
  list is in `host/docs/tools.md`), so the operator override needs no tehas
  code.
- **Say nothing and keep the default.** What produced this issue.

## Consequences

- The issue is not met by this record alone. Its acceptance criterion is the bar
  in `AGENTS.md`, and only a person may write it (rule 7). The ticket stays open
  until that line lands, and the check is one cycle's log: the line
  `work-driver: policy — unstated (denied)` becomes a verdict that names the bar
  and the sentence it quoted.
- The pages that say what a review verdict does now name the bar:
  `host/docs/operating.md`, the row for it in `host/docs/features.md`, one line
  under the flow in `host/docs/overview.md`, and the
  `PI_ENSEMBLE_REVIEW_THRESHOLD` row in `host/docs/tools.md`. `host/docs/llms.txt`
  lists the six-pass review as a stage of a cycle and sends the reader to
  Operating for what a finding does; a stage list is not a description of a
  verdict and does not restate the bar.
- `instances/example/instance.env` documents `PI_ENSEMBLE_REVIEW_THRESHOLD`,
  commented out. An instance that sets it wins over the product's doctrine, and
  that is the operator's call, not a cycle's. tehas does not validate the value:
  `lib/config.sh` checks no setting of that name, and pi-rukas honours it only
  when it reads as one of the four severities after trimming and upper-casing.
  Anything else falls back to doctrine and the default with no message from the
  settings check, so a typo in an instance shows up only in the cycle's log for
  the bar. This is unlike a `TEHAS_*` setting, which stops a command with 78.
- `host/docs/tools.md` now documents what the `PI_ENSEMBLE_*` channel is, because
  this bar travels over it: which names tehas reads and does not pass on, which
  it sets for every container, and which it sets from its own settings after it
  reads the instance file, so an instance line for one of those is replaced and
  not honoured. It also records that the same channel carries pi-rukas levers
  that loosen a cycle's own guards (`PI_ENSEMBLE_PROTECTED_PATHS`,
  `PI_ENSEMBLE_POLICY_JUDGE`). tehas has always passed those through and sets
  none of them. Naming the channel here records no change to it: the mounts and
  the environment of a cycle's container are unchanged, and `test/work_test.sh`
  still pins them.
- What happens at the review round cap is pi-rukas's rule, not this bar: three
  rounds, a CRITICAL finding always parks, and a cap reached with only MEDIUM
  and HIGH findings left, an approving adversarial review and those findings
  posted on the pull request carries on to the gate instead. This record does
  not change that.
- The other half of the issue, the `policy — unstated (denied)` wording in
  pi-rukas, is not fixable here. It belongs in pi-rukas's own repository; the
  improvement loop filed it under category `docs`, which routes here, and that
  routing is itself worth correcting there.
