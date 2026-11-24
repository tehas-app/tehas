//! The reader over a body as long as GitHub takes, of shapes that would
//! cost a quadratic reader dear: it stays quick.

use std::time::Instant;
use tehas_github::markers::read;
use tehas_ports::BlueprintNo;

#[test]
fn a_body_of_64_kib_of_backticks_quotes_and_items_reads_quickly() {
    let pieces = ["` ", "`` x ", "\\\\` ", "> q\nlazy ", "- i\n  ", "<!-- tehas:hold-cleared -->\n"];
    for p in pieces {
        let body = p.repeat(65536 / p.len());
        let began = Instant::now();
        let _ = read(&body, BlueprintNo::new(7));
        let took = began.elapsed();
        assert!(took.as_secs() < 5, "{p:?}: {took:?}");
    }
}
