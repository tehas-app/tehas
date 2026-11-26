//! The watcher's reports at their seam: a thing that did not happen,
//! told as a typed event. The default sink writes the exact bytes the
//! watcher has written all along: the command's stream, and the cycle's
//! own record beside its log. A test substitutes a recorder and holds
//! what was told; the words each event says are held and tested here.
//! Progress lines are not tells: they go to the stream by `say`.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

/// One report of a thing that did not happen, as the watcher saw it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tell {
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
    /// take it either, that is said too.
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
    /// The lines this tells says on the command's stream, in the order
    /// said. `record_takes` answers whether the record took the line of
    /// a `FileLost`: when it did not, the second line says so. It is
    /// true for every other tell, which says no line to a record.
    pub fn lines(&self, record_takes: bool) -> Vec<String> {
        match self {
            Tell::BeatLost { issue, word, line } => vec![format!(
                "tehas work: the heartbeat of #{issue} is not on record ({word}): {line}"
            )],
            Tell::PublishLost { word, line } => {
                vec![format!(
                    "tehas work: the live view is not published ({word}): {line}"
                )]
            }
            Tell::FileLost { file, record } => {
                let said = format!("tehas work: could not write {}", file.display());
                if record_takes {
                    vec![said]
                } else {
                    vec![
                        said,
                        format!(
                            "tehas work: could not say of {} that {} was lost",
                            record.display(),
                            file.display()
                        ),
                    ]
                }
            }
            Tell::RecordLost { record } => {
                vec![format!("tehas work: could not write {}", record.display())]
            }
        }
    }
}

/// The seam of `Tell`s. The default is [`Streams`]; a test substitutes
/// a recorder.
pub trait Tells {
    /// Report one tell.
    fn tell(&self, t: &Tell);
}

/// The watcher's own sink: the words of each tell go to the command's
/// error stream, and the line of a lost file is kept in the record too,
/// as the watcher has always written them.
pub struct Streams;

impl Tells for Streams {
    fn tell(&self, t: &Tell) {
        match t {
            Tell::FileLost { file, record } => {
                let said = format!("tehas work: could not write {}", file.display());
                eprintln!("{said}");
                if !append(record, format!("{said}\n").as_bytes()) {
                    eprintln!(
                        "tehas work: could not say of {} that {} was lost",
                        record.display(),
                        file.display()
                    );
                }
            }
            other => {
                for line in other.lines(true) {
                    eprintln!("{line}");
                }
            }
        }
    }
}

/// Append `line` to `file`, and answer whether it went.
pub fn append(file: &Path, line: &[u8]) -> bool {
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
        let file = PathBuf::from("/run/tehas/state/7/end");
        let record = PathBuf::from("/logs/7.err");
        assert_eq!(
            Tell::BeatLost {
                issue: 7,
                word: "nocycle".into(),
                line: "cycle-heartbeat 7: no start".into(),
            }
            .lines(true),
            ["tehas work: the heartbeat of #7 is not on record (nocycle): cycle-heartbeat 7: no start"]
        );
        assert_eq!(
            Tell::PublishLost {
                word: "busy".into(),
                line: "the database is locked".into(),
            }
            .lines(true),
            ["tehas work: the live view is not published (busy): the database is locked"]
        );
        assert_eq!(
            Tell::FileLost {
                file: file.clone(),
                record: record.clone(),
            }
            .lines(true),
            ["tehas work: could not write /run/tehas/state/7/end"]
        );
        assert_eq!(
            Tell::FileLost {
                file: file.clone(),
                record: record.clone(),
            }
            .lines(false),
            [
                "tehas work: could not write /run/tehas/state/7/end",
                "tehas work: could not say of /logs/7.err that /run/tehas/state/7/end was lost",
            ]
        );
        assert_eq!(
            Tell::RecordLost { record }.lines(true),
            ["tehas work: could not write /logs/7.err"]
        );
    }

    /// The default sink keeps the line of a lost file in the record, as
    /// `lost` did.
    #[test]
    fn a_lost_file_is_kept_in_the_record() {
        let dir = TestDir::new("tells-record");
        let record = dir.join("7.err");
        Streams.tell(&Tell::FileLost {
            file: PathBuf::from("/run/tehas/state/7/end"),
            record: record.clone(),
        });
        assert_eq!(
            fs::read_to_string(&record).unwrap(),
            "tehas work: could not write /run/tehas/state/7/end\n"
        );
        assert!(append(&record, b"again\n"));
        assert_eq!(
            fs::read_to_string(&record).unwrap(),
            "tehas work: could not write /run/tehas/state/7/end\nagain\n"
        );
    }
}
