//! The watcher's reports at their seam: a thing that did not happen,
//! told as a typed event. The default sink writes the exact bytes the
//! watcher has written all along: the command's stream, and the cycle's
//! own record beside its log. A test substitutes a recorder and holds
//! what was told; the words each event says are held and tested here,
//! as the only copy the sink reads. Progress lines are not tells: they
//! go to the stream by `say`.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

/// One report of a thing that did not happen, as the watcher saw it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)] // the loss is the report: every variant says one thing went missing
pub(crate) enum Tell {
    /// The heartbeat of issue `issue` did not reach the store: `word`
    /// and `line` are the store's answer, as it said them.
    BeatLost {
        /// The issue whose heartbeat is lost.
        issue: u64,
        /// The store's word for its failure.
        word: String,
        /// The store's line for its failure.
        line: String,
    },
    /// The live view is not published: `word` and `line` are the
    /// store's answer, as it said them.
    PublishLost {
        /// The store's word for its failure.
        word: String,
        /// The store's line for its failure.
        line: String,
    },
    /// A file of the protocol was not written. Its loss is said on the
    /// stream and kept in the cycle's record; when the record cannot
    /// take it either, `record_missed` says that too.
    FileLost {
        /// The file that was lost.
        file: PathBuf,
        /// The cycle's record, which is told as well.
        record: PathBuf,
    },
    /// A line could not reach the file it was meant for.
    RecordLost {
        /// The file that took nothing.
        record: PathBuf,
    },
}

impl Tell {
    /// The lines this tell says on the command's stream, in the order
    /// said. The first line of a `FileLost` is kept in the record too;
    /// when the record takes nothing, `record_missed` is said after.
    pub(crate) fn lines(&self) -> Vec<String> {
        match self {
            Tell::BeatLost { issue, word, line } => vec![format!(
                "tehas work: the heartbeat of #{issue} is not on record ({word}): {line}"
            )],
            Tell::PublishLost { word, line } => {
                vec![format!(
                    "tehas work: the live view is not published ({word}): {line}"
                )]
            }
            Tell::FileLost { file, .. } => {
                vec![format!("tehas work: could not write {}", file.display())]
            }
            Tell::RecordLost { record } => {
                vec![format!("tehas work: could not write {}", record.display())]
            }
        }
    }

    /// The line a `FileLost` says when its record took nothing. None for
    /// every other tell, which says nothing to a record.
    pub(crate) fn record_missed(&self) -> Option<String> {
        match self {
            Tell::FileLost { file, record } => Some(format!(
                "tehas work: could not say of {} that {} was lost",
                record.display(),
                file.display()
            )),
            _ => None,
        }
    }
}

/// The seam of `Tell`s. The default is [`Streams`]; a test substitutes
/// a recorder.
pub(crate) trait Tells {
    /// Report one tell.
    fn tell(&self, t: &Tell);
}

/// The watcher's own sink: the words of each tell go to the command's
/// error stream, and the line of a lost file is kept in the record too,
/// as the watcher has always written them. The words are read from
/// `Tell::lines` alone.
pub(crate) struct Streams;

impl Tells for Streams {
    fn tell(&self, t: &Tell) {
        let lines = t.lines();
        let took = match t {
            Tell::FileLost { record, .. } => append(record, format!("{}\n", lines[0]).as_bytes()),
            _ => true,
        };
        for line in lines {
            eprintln!("{line}");
        }
        if !took {
            if let Some(line) = t.record_missed() {
                eprintln!("{line}");
            }
        }
    }
}

/// Append `line` to `file`, and answer whether it went.
pub(crate) fn append(file: &Path, line: &[u8]) -> bool {
    let done = OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
        .and_then(|mut f| f.write_all(line));
    done.is_ok()
}

#[cfg(test)]
mod tests {
    use super::{append, Streams, Tell, Tells};
    use std::fs;
    use std::path::PathBuf;
    use tehas_core::testdir::TestDir;

    /// The bytes each tell says, held as the watcher wrote them before
    /// the seam existed.
    #[test]
    fn each_tell_says_the_words_the_watcher_wrote() {
        let file = PathBuf::from("/x/7/end");
        let record = PathBuf::from("/x/7.err");
        assert_eq!(
            Tell::BeatLost {
                issue: 7,
                word: "nocycle".into(),
                line: "cycle-heartbeat 7: no start".into(),
            }
            .lines(),
            ["tehas work: the heartbeat of #7 is not on record (nocycle): cycle-heartbeat 7: no start"]
        );
        assert_eq!(
            Tell::PublishLost {
                word: "busy".into(),
                line: "the database is locked".into(),
            }
            .lines(),
            ["tehas work: the live view is not published (busy): the database is locked"]
        );
        let lost = Tell::FileLost {
            file: file.clone(),
            record: record.clone(),
        };
        assert_eq!(lost.lines(), ["tehas work: could not write /x/7/end"]);
        assert_eq!(
            lost.record_missed().as_deref(),
            Some("tehas work: could not say of /x/7.err that /x/7/end was lost")
        );
        assert_eq!(
            Tell::RecordLost {
                record: record.clone()
            }
            .lines(),
            ["tehas work: could not write /x/7.err"]
        );
        assert_eq!(
            Tell::RecordLost { record }.record_missed(),
            None,
            "only a lost file is kept in the record"
        );
    }

    /// The default sink keeps the line of a lost file in the record, as
    /// `lost` did: the very line it says, which the test above pins.
    #[test]
    fn a_lost_file_is_kept_in_the_record() {
        let d = TestDir::new("tells-record");
        let record = d.join("7.err");
        let lost = Tell::FileLost {
            file: PathBuf::from("/x/7/end"),
            record: record.clone(),
        };
        Streams.tell(&lost);
        let said = format!("{}\n", lost.lines()[0]);
        assert_eq!(fs::read_to_string(&record).unwrap(), said);
        assert!(append(&record, b"again\n"));
        let kept = format!("{said}again\n");
        assert_eq!(fs::read_to_string(&record).unwrap(), kept);
    }
}
