//! The review issue, rendered: the title and body the filing half of
//! `tehasbot review` puts in the issue it opens for triage.
//!
//!   review-issue <path> <branch> <commit>
//!     The issue's title and body, from the document of one review run. The
//!     rule of what is listed and how much of it is shown is
//!     tehas_core::review, where it is tested; this verb is the way the shell
//!     reaches it and the way the two values arrive as one line the store's
//!     caller can read. The words of a finding come from the model, so both
//!     values are escaped by the line builder and the answer stays one line
//!     whatever a finding says. Masking them is the filing half's work, on the
//!     way to GitHub.
//!
//! Exit: 0 the document was read and the issue rendered; 64 the document is
//! not a plain file, is over 1 MiB, is no JSON, or is no object. A `<branch>`
//! or `<commit>` outside its grammar is 64 too, and is refused by `parse`
//! before the file is read.

use crate::verbs;
use std::path::Path;

/// A review's document is one object with the findings of the six lenses. This
/// is far above it, and is the bound the verb agrees to read: the same bound
/// the import of the older records reads a file at.
const MAX_BYTES: u64 = 1 << 20;

/// The line of a refusal about one of the verb's arguments. `main` prints it
/// with the `store: ` the store's failures carry, so the word of the verb
/// here is what makes the line read `store: review-issue: ...`.
fn refuse(path: &Path, why: &str) -> (u8, String) {
    (64, format!("review-issue: {} {why}", path.display()))
}

pub fn run(path: &Path, branch: &str, commit: &str) -> Result<String, (u8, String)> {
    // The document comes from the run's log, which the caller owns: a link or
    // a directory is refused, as every host file the store reads is.
    let (bytes, _) = verbs::read_file(path, MAX_BYTES).map_err(|e| {
        refuse(
            path,
            match e {
                verbs::FileErr::TooLarge => "is over 1 MiB",
                verbs::FileErr::Refused => "is not a plain file",
            },
        )
    })?;
    // Bytes that are no UTF-8 are no JSON either, so they answer as a document
    // that does not parse, and one reader covers both.
    let text = String::from_utf8_lossy(&bytes);
    let document: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| refuse(path, &format!("is not JSON: {e}")))?;
    // A document of the run is the object the runner answers with. An array,
    // a string or a number is no document, whatever it holds inside it.
    if !document.is_object() {
        return Err(refuse(path, "is not a review document"));
    }
    let issue = tehas_core::review::review_issue(&document, branch, commit);
    Ok(verbs::line(
        "review-issue",
        &[
            ("title", verbs::j(issue.title)),
            ("body", verbs::j(issue.body)),
        ],
    ))
}
