//! What a failed curl call comes to, as one of the ports' named outcomes, so
//! an error tehas prints is its own context and `Outcome::line()`, never
//! curl's or the site's words (the ports spec, Decided 69, B7b).

use tehas_ports::{At, Outcome, Passing, Pool, Refusal, Sent, Unread};

/// The outcome of a curl run that did not end 0: `code` is its exit status,
/// `said` its standard error, read for the status `-f` names and nothing
/// else. `sent` is whether a request that went out and got no answer may
/// have reached the site: `Sent::Maybe` for a write.
pub fn failed(code: Option<i32>, said: &str, sent: Sent) -> Outcome {
    let status = said
        .rsplit_once("returned error: ")
        .and_then(|(_, s)| s.get(..3))
        .and_then(|s| s.parse::<u16>().ok());
    match (status, code) {
        (Some(401), _) => Outcome::SignedOut,
        (Some(403), _) => Outcome::Forbidden,
        (Some(404 | 410), _) => Outcome::NotFound,
        // When the budget is back is not read here; only the line is.
        (Some(429), _) => Outcome::Spent {
            pool: Pool::Unknown,
            until: At::new(0),
        },
        (Some(400..=499), _) => Outcome::Refused(Refusal::Unknown),
        (Some(500..=599), _) => Outcome::Unavailable {
            why: Passing::Server,
            sent,
        },
        // Timed out.
        (_, Some(28)) => Outcome::Unavailable {
            why: Passing::Timeout,
            sent,
        },
        // No proxy, no host, no connection, or no TLS: nothing went out.
        (_, Some(5 | 6 | 7 | 35 | 51 | 58 | 59 | 60)) => Outcome::Unavailable {
            why: Passing::Unreachable,
            sent: Sent::No,
        },
        // The connection broke, or an empty answer, after the request went.
        (_, Some(52 | 55 | 56)) => Outcome::Unavailable {
            why: Passing::Server,
            sent,
        },
        // Cut off in transit.
        (_, Some(18)) => Outcome::Unread(Unread::Malformed),
        _ => Outcome::Unread(Unread::Unknown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(n: u16) -> String {
        format!("curl: (22) The requested URL returned error: {n}\n")
    }

    #[test]
    fn a_status_is_its_outcome() {
        let read = |n| failed(Some(22), &status(n), Sent::No);
        assert_eq!(read(401), Outcome::SignedOut);
        assert_eq!(read(403), Outcome::Forbidden);
        assert_eq!(read(404), Outcome::NotFound);
        assert_eq!(read(410), Outcome::NotFound);
        assert!(matches!(read(429), Outcome::Spent { .. }));
        assert_eq!(read(422), Outcome::Refused(Refusal::Unknown));
        assert_eq!(
            read(502),
            Outcome::Unavailable {
                why: Passing::Server,
                sent: Sent::No
            }
        );
        assert_eq!(
            failed(Some(22), &status(500), Sent::Maybe).line(),
            "the site failed; it may have been sent"
        );
    }

    #[test]
    fn curl_s_own_failures_are_outcomes_and_its_words_are_not_kept() {
        let host = "curl: (6) Could not resolve host: api.example.test";
        let out = failed(Some(6), host, Sent::Maybe);
        assert_eq!(
            out,
            Outcome::Unavailable {
                why: Passing::Unreachable,
                sent: Sent::No
            }
        );
        assert!(!out.line().contains("example"), "{}", out.line());
        assert_eq!(
            failed(Some(28), "curl: (28) Operation timed out", Sent::Maybe),
            Outcome::Unavailable {
                why: Passing::Timeout,
                sent: Sent::Maybe
            }
        );
        assert_eq!(
            failed(Some(56), "curl: (56) Recv failure", Sent::No),
            Outcome::Unavailable {
                why: Passing::Server,
                sent: Sent::No
            }
        );
        assert_eq!(
            failed(Some(18), "curl: (18) transfer closed", Sent::No),
            Outcome::Unread(Unread::Malformed)
        );
        assert_eq!(
            failed(Some(2), "curl: (2) Failed initialization", Sent::No),
            Outcome::Unread(Unread::Unknown)
        );
        assert_eq!(failed(None, "", Sent::No), Outcome::Unread(Unread::Unknown));
    }
}
