//! The reader over a body as long as GitHub takes, of shapes that would
//! cost a quadratic reader dear: it stays quick.

use std::time::{Duration, Instant};
use tehas_github::markers::read;
use tehas_ports::BlueprintNo;

const M: &str = "<!-- tehas:finish issue=7 result=opened -->";

/// The fastest of five reads of `body`.
fn timed(body: &str) -> Duration {
    (0..5)
        .map(|_| {
            let began = Instant::now();
            let _ = read(body, BlueprintNo::new(7));
            began.elapsed()
        })
        .min()
        .unwrap()
}

#[test]
fn a_body_of_64_kib_of_backticks_quotes_and_items_reads_quickly() {
    let pieces = [
        "` ",
        "`` x ",
        "\\\\` ",
        "> q\nlazy ",
        "- i\n  ",
        "<!-- tehas:hold-cleared -->\n",
        ">",
        "<!-- ",
        "<div>\n",
        "```\nx\n",
        "| x | y |\n",
        "x\r",
        "\t",
        "\\",
        "[",
        "](",
        "- x\n",
        "> - x\n",
    ];
    let mut bodies: Vec<String> = pieces.iter().map(|p| p.repeat(65536 / p.len())).collect();
    bodies.push(format!(
        "| a | b |\n|---|---|\n{}",
        "| x | y |\n".repeat(6000)
    ));
    bodies.push(format!("<script>\n{}{M}", "x\n".repeat(32000)));
    bodies.push((0..120).map(|k| " ".repeat(k * 2) + "- x\n").collect());
    bodies.push((0..120).map(|k| "> ".repeat(k) + "- x\n").collect());
    bodies.push(format!("{}x{}", "- ".repeat(16000), "\n".repeat(32000)));
    bodies.push(format!("{}x{}", "- ".repeat(16000), "\nx".repeat(16000)));
    for body in bodies {
        let took = timed(&body);
        assert!(
            took.as_secs() < 5,
            "{:?}: {took:?}",
            &body[..body.len().min(40)]
        );
    }
}

/// Containers nested on one line, each opened past the last: the reader
/// keeps its work per line bounded, so twice the body takes about twice
/// the time, not four times.
#[test]
fn nested_list_markers_on_one_line_read_in_linear_time() {
    for piece in ["- ", "> - "] {
        let body = |kib: usize| format!("{}{M}", piece.repeat(kib * 1024 / piece.len()));
        let quarter = timed(&body(16));
        let (half, whole) = (timed(&body(32)), timed(&body(64)));
        eprintln!("{piece:?}: 16 KiB {quarter:?}, 32 KiB {half:?}, 64 KiB {whole:?}");
        assert!(whole.as_secs() < 2, "{piece:?} at 64 KiB: {whole:?}");
        assert!(
            whole <= half * 5 / 2 + Duration::from_millis(5),
            "{piece:?}: 32 KiB {half:?}, 64 KiB {whole:?}"
        );
    }
}
