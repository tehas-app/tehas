//! The contract of the snapshot files the host writes under its `mcp/`
//! directory: analytics writes them, the MCP server reads them. The fields of
//! a session, in the order the writer selects them, and the shape numbers of
//! the headers are here so that a field is added or renamed once, in one list
//! both sides read, and not in a writer's list and a reader's that can drift.

/// What a field holds, spelled once for both sides: the column kind the writer
/// reads from SQLite and the JSON type the reader names in its schema. `Time`
/// is a Unix second the writer turns into an ISO string, so it is a string to
/// the reader.
#[derive(Clone, Copy, PartialEq)]
pub enum Shape {
    Text,
    Int,
    Real,
    Bool,
    Time,
}

impl Shape {
    /// The JSON type of a field of this shape in the reader's schema.
    pub const fn json_type(self) -> &'static str {
        match self {
            Shape::Text | Shape::Time => "string",
            Shape::Int => "integer",
            Shape::Real => "number",
            Shape::Bool => "boolean",
        }
    }
}

/// One field of a session: the name in the file, its shape, and what the
/// reader's schema says it is.
pub type Field = (&'static str, Shape, &'static str);

/// The api version of a snapshot header and of an answer.
pub const API: i64 = 1;
/// The shape number a sessions header carries, and the one its reader expects.
pub const SESSIONS_SHAPE: i64 = 1;
/// The shape number an outcomes header carries, and the one its reader expects.
pub const OUTCOMES_SHAPE: i64 = 1;

/// The fields of a session, in the order the writer selects them. `concurrent`
/// is last, and it is the one field only a full session answers.
pub const RUN_FIELDS: [Field; 40] = [
    ("id", Shape::Text, "The session id, like work-12-1792485967."),
    ("kind", Shape::Text, "work or plan."),
    ("issue", Shape::Int, "The issue number."),
    (
        "attempt",
        Shape::Int,
        "Which attempt on the issue this run was.",
    ),
    (
        "how",
        Shape::Text,
        "What started it: label, refresh, retry-tick, outage-retry, millwright, fixer, answer, refine or hand.",
    ),
    ("started", Shape::Time, "When it started, UTC."),
    (
        "ended",
        Shape::Time,
        "When it ended, UTC. Left out while it runs.",
    ),
    ("seconds", Shape::Int, "Seconds from start to end."),
    (
        "queueWait",
        Shape::Int,
        "Seconds it waited before it started.",
    ),
    (
        "outcome",
        Shape::Text,
        "pr, merged, handoff, parked, failed, refused or stopped.",
    ),
    ("exitCode", Shape::Int, "The exit status of the cycle."),
    ("status", Shape::Text, "The status word the cycle ended with."),
    ("lastStep", Shape::Text, "The last step it reached."),
    ("failedStep", Shape::Text, "The step that failed."),
    ("cap", Shape::Text, "The cap that stopped it."),
    ("parkReason", Shape::Text, "Why it was parked."),
    (
        "reviewHeld",
        Shape::Bool,
        "Whether the review held the pull request.",
    ),
    ("failureKind", Shape::Text, "The kind of failure."),
    ("failureCause", Shape::Text, "The cause of the failure."),
    (
        "providerOutage",
        Shape::Bool,
        "Whether a model provider outage caused it.",
    ),
    ("endpoint", Shape::Text, "The model endpoint it used."),
    ("pr", Shape::Int, "The pull request number."),
    ("tehasVersion", Shape::Text, "The tehas version it ran."),
    ("tehasCommit", Shape::Text, "The tehas commit it ran."),
    ("rukasVersion", Shape::Text, "The pi-rukas version it ran."),
    ("rukasCommit", Shape::Text, "The pi-rukas commit it ran."),
    (
        "image",
        Shape::Text,
        "The sandbox image it ran in, by its id sha256:<hex>. The id names one build.",
    ),
    (
        "imageBuilt",
        Shape::Time,
        "When that image was built, UTC. Left out when unknown.",
    ),
    ("resultSchema", Shape::Int, "The schema of the result file."),
    ("input", Shape::Int, "Input tokens."),
    ("output", Shape::Int, "Output tokens."),
    ("cacheRead", Shape::Int, "Tokens read from the cache."),
    ("cacheWrite", Shape::Int, "Tokens written to the cache."),
    ("turns", Shape::Int, "Model turns."),
    ("dispatches", Shape::Int, "Model dispatches."),
    ("cost", Shape::Real, "Model cost as the runner reported it."),
    ("runnerMs", Shape::Int, "Milliseconds the runner ran."),
    ("oomKills", Shape::Int, "Processes killed for memory."),
    (
        "forksRefused",
        Shape::Int,
        "Forks refused at the process limit.",
    ),
    (
        "concurrent",
        Shape::Int,
        "Cycles running at the same time when it started.",
    ),
];

/// The fields of one step of a session.
pub const STEP_FIELDS: [Field; 5] = [
    ("step", Shape::Text, "The step name."),
    ("round", Shape::Int, "Which round of the step."),
    ("started", Shape::Time, "When it started, UTC."),
    ("ended", Shape::Time, "When it ended, UTC."),
    ("seconds", Shape::Int, "Seconds it took."),
];

/// The fields of one role's model use in a session.
pub const ROLE_FIELDS: [Field; 11] = [
    (
        "role",
        Shape::Text,
        "The role, such as developer or reviewer.",
    ),
    ("model", Shape::Text, "The model."),
    ("provider", Shape::Text, "The provider."),
    ("dispatches", Shape::Int, "Dispatches to the model."),
    ("turns", Shape::Int, "Model turns."),
    ("input", Shape::Int, "Input tokens."),
    ("output", Shape::Int, "Output tokens."),
    ("cacheRead", Shape::Int, "Tokens read from the cache."),
    ("cacheWrite", Shape::Int, "Tokens written to the cache."),
    ("cost", Shape::Real, "Model cost as the runner reported it."),
    ("ms", Shape::Int, "Milliseconds of model time."),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shape_names_one_json_type() {
        assert_eq!(Shape::Text.json_type(), "string");
        assert_eq!(Shape::Time.json_type(), "string");
        assert_eq!(Shape::Int.json_type(), "integer");
        assert_eq!(Shape::Real.json_type(), "number");
        assert_eq!(Shape::Bool.json_type(), "boolean");
    }

    #[test]
    fn a_field_list_names_each_field_once() {
        for fields in [
            &RUN_FIELDS.to_vec(),
            &STEP_FIELDS.to_vec(),
            &ROLE_FIELDS.to_vec(),
        ] {
            let mut names: Vec<&str> = fields.iter().map(|f| f.0).collect();
            let all = names.len();
            names.sort_unstable();
            names.dedup();
            assert_eq!(names.len(), all, "{names:?}");
        }
        // Only a full session answers it, so no summary or attempt list may name it.
        assert_eq!(RUN_FIELDS[RUN_FIELDS.len() - 1].0, "concurrent");
    }
}
