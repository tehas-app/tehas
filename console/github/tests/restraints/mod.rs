//! One marker of each kind that holds the factory back, for the tests
//! that hold the writer to posting each live or not at all.

use std::num::{NonZeroU32, NonZeroU64};
use tehas_ports::{
    BlueprintNo, CommentId, FinishMarker, FinishResult, LastAttemptMarker, Marker,
    MillwrightAction, MillwrightMarker, ParkMarker, QuestionKind, Ref, Sha256, TicketKey,
};

/// One marker of each kind that holds the factory back.
pub fn restraints() -> Vec<Marker> {
    let no = BlueprintNo::new(7).unwrap();
    let sig = Sha256::new("a".repeat(64)).unwrap();
    let mut last = LastAttemptMarker::new(sig.clone());
    last.note = "the last try".to_string();
    vec![
        Marker::Blocked {
            by: vec![Ref::Here(TicketKey::Number(NonZeroU64::new(3).unwrap()))],
        },
        Marker::Declined {
            question: CommentId::new("12"),
        },
        Marker::Park(ParkMarker::new(no, sig)),
        Marker::Finish(FinishMarker::new(no, FinishResult::Opened)),
        Marker::LastAttempt(last),
        Marker::Millwright(MillwrightMarker::new(no, MillwrightAction::Person)),
        Marker::Millwright(MillwrightMarker::new(no, MillwrightAction::Ask)),
        Marker::Question {
            kind: QuestionKind::Plan,
            blueprint: no,
            turn: NonZeroU32::new(1).unwrap(),
        },
    ]
}
