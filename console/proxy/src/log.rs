//! The proxy's log: one line per request and one per refusal, and nothing
//! else. A body holds source code, so a body is never part of a line, and so
//! is a token, which holds the right to the factory's model.
//!
//! The line names the outcome, the path, the status, the time, and the job and
//! contractor the token named. A path is what a person reads to find a broken
//! contractor; it is public, and it is the only part of the request any line
//! holds.
//!
//! The writer is a function, not a file, so a test reads the lines the proxy
//! would have written, and the assertion that a marker is nowhere in them is
//! made over the same bytes a host's journal gets.

/// One line of the proxy's log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line(pub String);

impl Line {
    /// The line a request that was answered gets: the method, the path, the
    /// status, the time in milliseconds, and whose job it was.
    pub fn answered(
        method: &str,
        path: &str,
        status: u16,
        ms: u128,
        job: i64,
        contractor: &str,
    ) -> Line {
        Line(format!(
            "{method} {path} {status} {ms}ms job {job} contractor {contractor}"
        ))
    }
}

/// What the proxy writes, and where. `line` is what one request left behind:
/// the host's journal on a host, and a vector in a test.
pub trait Sink {
    fn line(&self, line: &Line);
}

impl<F: Fn(&Line)> Sink for F {
    fn line(&self, line: &Line) {
        self(line)
    }
}

/// The writer of a host: standard error, where the unit's journal keeps it. A
/// function is a Sink, and the request threads of the listener share one.
pub struct Journal;

impl Sink for Journal {
    fn line(&self, line: &Line) {
        eprintln!("tehas-proxy: {}", line.0);
    }
}

/// The same writer as a function, for a caller that has one thread.
pub fn stderr(line: &Line) {
    eprintln!("tehas-proxy: {}", line.0);
}

/// A line that is asked to hold no more than this many bytes of a path, so a
/// request for a path of a megabyte cannot write a megabyte to the journal.
pub const PATH_MAX: usize = 200;

/// The path as the log holds it: the characters a person needs, the rest cut,
/// and no line end that would make one line of the journal into two.
pub fn shown(path: &str) -> String {
    let flat: String = path
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(PATH_MAX)
        .collect();
    flat
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::Refused;

    #[test]
    fn one_line_names_the_answer_and_nobody_else() {
        let line = Line::answered("POST", "/v1/chat/completions", 200, 12, 7, "gpu1");
        assert_eq!(
            line.0,
            "POST /v1/chat/completions 200 12ms job 7 contractor gpu1"
        );
    }

    #[test]
    fn a_shown_path_writes_one_line_and_no_more_than_its_bound() {
        let long = "/".to_string() + &"a".repeat(500);
        let _ = super::shown;
        let cut = shown(&long);
        assert_eq!(cut.len(), PATH_MAX);
        assert!(!cut.contains(['\n', '\r', '\0']));
        assert_eq!(shown("a\r\nb"), "a  b");
        assert_eq!(shown("/v1/models"), "/v1/models");
    }

    #[test]
    fn a_sink_takes_what_a_host_s_journal_would() {
        use std::sync::{Mutex, PoisonError};
        let kept = Mutex::new(Vec::new());
        let sink = |line: &Line| {
            kept.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(line.0.clone())
        };
        sink.line(&Line::answered("GET", "/v1/models", 200, 1, 3, "gpu1"));
        sink.line(&Line(
            Refused::Path("/v1/embeddings".to_string()).line(Some(3), Some("gpu1")),
        ));
        let held = kept.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(held.len(), 2);
        assert!(held[0].starts_with("GET /v1/models 200 "));
        assert!(held[1].starts_with("refused job 3 of contractor gpu1: "));
    }
}
