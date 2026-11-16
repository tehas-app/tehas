//! What a job's model asked for and answered, counted per contractor and per
//! job, so the factory can report it (#2277) and no contractor reports it for
//! itself.
//!
//! The figures are the ones the endpoint itself gives: an OpenAI-shaped
//! completion answers `usage` with `prompt_tokens`, `completion_tokens` and
//! `total_tokens`, and a streamed answer carries them in its last chunk when
//! the caller asked for them. A body that answers no `usage`, or answers one
//! of another shape, counts nothing: the proxy never guesses a token count,
//! and never keeps a body to look for one later.
//!
//! The rows are written to the store, which is the only place the factory
//! keeps a number of its own. The store's rows are what `tehas status` and
//! the dashboard read; nothing here holds a total in memory between calls.

/// The three figures of one request, as the endpoint answered them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub prompt: u64,
    pub completion: u64,
    pub total: u64,
}

impl Usage {
    pub fn is_empty(&self) -> bool {
        self.prompt == 0 && self.completion == 0 && self.total == 0
    }
}

/// The `usage` of one answer's JSON, or of the last chunk of a stream that
/// carried one. None for an answer that holds no figures of its own, and for
/// anything that is not JSON: the proxy reads the bytes of the last chunk for
/// this one object and keeps nothing of the rest of it.
pub fn of_answer(body: &[u8]) -> Option<Usage> {
    let text = std::str::from_utf8(body).ok()?;
    // A completion is one object; a gateway that answers an event stream to a
    // request that did not ask for one is read the way a stream is.
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| usage_of(v.get("usage")?))
        .or_else(|| of_stream(body))
}

/// The last chunk of a stream that carries a `usage`: the chunks of a server
/// sent-event stream, each `data: <json>`, in the order they arrived. The
/// first figure a chunk gives wins, which is what the endpoint sent last: the
/// stream is read once and the search stops at the first answer that has the
/// figures.
pub fn of_stream(body: &[u8]) -> Option<Usage> {
    let text = std::str::from_utf8(body).ok()?;
    // The chunks last sent are the ones that carry the price of the whole, so
    // the search starts at the end.
    text.lines().rev().find_map(|line| {
        let data = line.trim().strip_prefix("data:")?.trim();
        if data.is_empty() || data == "[DONE]" {
            return None;
        }
        serde_json::from_str::<serde_json::Value>(data)
            .ok()
            // The figures ride beside the delta of a streamed chunk, and are
            // the whole answer's, not one chunk's.
            .and_then(|v| {
                usage_of(v.get("usage")?).or_else(|| {
                    v.get("choices")
                        .and_then(serde_json::Value::as_array)
                        .and_then(|a| a.last())
                        .and_then(|c| usage_of(c.get("usage")?))
                })
            })
    })
}

fn usage_of(value: &serde_json::Value) -> Option<Usage> {
    let figure = |name: &str| value.get(name).and_then(serde_json::Value::as_u64);
    let usage = Usage {
        prompt: figure("prompt_tokens")?,
        completion: figure("completion_tokens")?,
        total: figure("total_tokens")?,
    };
    (!usage.is_empty()).then_some(usage)
}

/// What one row of the store holds: who worked, which job, and the figures of
/// one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub contractor: String,
    pub job: i64,
    pub usage: Usage,
}

/// The store verb that keeps one row, and the words it is given in. One verb,
/// so the proxy holds no SQL of its own, and one verb whose rows are keyed by
/// contractor and job, so #2277 can read a contractor's totals back.
pub const VERB: &str = "outsource-usage";

/// The arguments of that verb for one row. Never a token, never a path of the
/// request, never a byte of a body.
pub fn args(row: &Row) -> Vec<String> {
    vec![
        VERB.to_string(),
        "--contractor".to_string(),
        row.contractor.clone(),
        "--job".to_string(),
        row.job.to_string(),
        "--prompt".to_string(),
        row.usage.prompt.to_string(),
        "--completion".to_string(),
        row.usage.completion.to_string(),
        "--total".to_string(),
        row.usage.total.to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANSWER: &str = r#"{"id":"x","choices":[],"usage":{"prompt_tokens":120,"completion_tokens":340,"total_tokens":460}}"#;

    #[test]
    fn the_figures_of_an_answer_are_the_endpoint_s_own() {
        assert_eq!(
            of_answer(ANSWER.as_bytes()),
            Some(Usage {
                prompt: 120,
                completion: 340,
                total: 460
            })
        );
    }

    #[test]
    fn an_answer_with_no_figures_counts_nothing() {
        for body in [
            "",
            "not json",
            "{}",
            r#"{"choices":[]}"#,
            r#"{"usage":null}"#,
            r#"{"usage":{}}"#,
            r#"{"usage":{"prompt_tokens":0,"completion_tokens":0,"total_tokens":0}}"#,
            r#"{"usage":{"prompt_tokens":"120"}}"#,
            r#"{"usage":{"prompt_tokens":120}}"#,
            r#"{"usage":{"prompt_tokens":-1,"completion_tokens":1,"total_tokens":2}}"#,
        ] {
            assert_eq!(of_answer(body.as_bytes()), None, "{body}");
        }
    }

    #[test]
    fn the_figures_of_a_stream_come_from_the_chunk_that_carries_them() {
        let stream = [
            "data: {\"choices\":[{\"delta\":{\"content\":\"def\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\" main\"}}]}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":9,\"total_tokens\":16}}\n\n",
            "data: [DONE]\n\n",
        ]
        .concat();
        assert_eq!(
            of_stream(stream.as_bytes()),
            Some(Usage {
                prompt: 7,
                completion: 9,
                total: 16
            })
        );
        // A stream whose chunks carry nothing: no figures, and no invented one.
        assert_eq!(
            of_stream("data: {\"choices\":[]}\n\ndata: [DONE]\n\n".as_bytes()),
            None
        );
        assert_eq!(of_stream(b""), None);
        assert_eq!(of_stream(b"garbage\n"), None);
    }

    #[test]
    fn one_row_names_the_contractor_the_job_and_the_three_figures() {
        let row = Row {
            contractor: "gpu1".to_string(),
            job: 7,
            usage: Usage {
                prompt: 1,
                completion: 2,
                total: 3,
            },
        };
        assert_eq!(
            args(&row),
            vec![
                VERB,
                "--contractor",
                "gpu1",
                "--job",
                "7",
                "--prompt",
                "1",
                "--completion",
                "2",
                "--total",
                "3"
            ]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
        );
        let said = args(&row).join(" ");
        assert!(!said.contains("tok-"), "{said}");
        assert!(!said.contains('/'), "{said} names a path");
    }

    #[test]
    fn a_row_of_no_figures_is_still_a_row_of_the_contractor_and_the_job() {
        let row = Row {
            contractor: "gpu1".to_string(),
            job: 7,
            usage: Usage::default(),
        };
        assert!(row.usage.is_empty());
        assert!(args(&row)
            .join(" ")
            .ends_with("--prompt 0 --completion 0 --total 0"));
    }
}
