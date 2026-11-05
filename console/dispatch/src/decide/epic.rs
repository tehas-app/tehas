//! The `epic` label for an open issue whose title starts with the epic marker
//! (text::epic_marker). The label is for people: what makes an epic for the
//! dispatcher and the epics panel stays GitHub's sub-issue links. tehas only
//! adds it, and a removal by any account but the bot's is final.

use super::Poll;
use crate::text::EPIC;
use serde_json::json;
use tehas_core::events::{label_state, Events, LabelState};

const WHY: &str = "its title starts with an epic marker";

impl Poll<'_> {
    pub(super) fn epic_titles(&mut self) {
        for n in self.s.epic_titled.clone() {
            let events = self.s.issues.get(&n).map(|i| &i.events);
            let state = match events {
                Some(Events::Read(ev)) => label_state(ev, EPIC),
                _ => {
                    self.log(format!(
                        "WARN: the label events of #{n} could not be read: it is not labelled {EPIC} this poll"
                    ));
                    continue;
                }
            };
            match state {
                LabelState::Absent => {}
                LabelState::Unlabeled { actor, .. } if actor == self.bot => {}
                // A person took it off, or the timeline says it is on while
                // the issue does not carry it: either way not this poll's.
                _ => continue,
            }
            if self.dry {
                self.log(format!("would label #{n} {EPIC}: {WHY}"));
                continue;
            }
            self.act(json!({"issue": n, "remove": [], "add": [EPIC],
                "log": format!("label #{n} {EPIC}: {WHY}"),
                "fail": Self::label_fail(n, &[], &[EPIC])}));
        }
    }
}
