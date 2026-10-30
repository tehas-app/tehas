//! The one report type of `live/dispatch.json`: the keys of the file and the
//! words its `why` takes. The dispatcher's poll writes the file through
//! [`Report`]; the health page, the analytics sampler and the poll's own read
//! of the last report name its fields by the constants here, so a key or a
//! word is added in one place.
//!
//! The words each name one reason a poll starts nothing, and the `hold` says
//! it in a line. A report with no hold has no why, and `to_value` writes the
//! empty string for it: every reader of the file asks `jq -r .why` and a key
//! that is missing would answer null where an empty word is expected.
//!
//! The time is the poll's, not the report's: [`KEY_TIME`] is named here
//! because a reader looks the field up, and `to_value` never writes it, so
//! the fixtures of the dispatcher stay byte-for-byte the same for two polls
//! that decide the same thing.

/// `why`: the one word of the hold, empty when nothing holds.
pub const KEY_WHY: &str = "why";
/// `hold`: the line that says the reason in full.
pub const KEY_HOLD: &str = "hold";
/// `count`: the cycles running when the poll decided.
pub const KEY_COUNT: &str = "count";
/// `room`: how many more containers the slice's ceilings fit.
pub const KEY_ROOM: &str = "room";
/// `committed`: the bytes the slice's children have been promised.
pub const KEY_COMMITTED: &str = "committed";
/// `max`: the slice's memory max in bytes.
pub const KEY_MAX: &str = "max";
/// `reserve`: what one cycle's container is given, as the setting writes it.
pub const KEY_RESERVE: &str = "reserve";
/// `free`: the slots the poll found free of its own ceiling.
pub const KEY_FREE: &str = "free";
/// `awaitRoom`: room left under the cap on unreviewed work.
pub const KEY_AWAIT_ROOM: &str = "awaitRoom";
/// `limitRoom`: room left under the limit of `tehas limit`.
pub const KEY_LIMIT_ROOM: &str = "limitRoom";
/// `refreshRoom`: slots free in the refresh lane.
pub const KEY_REFRESH_ROOM: &str = "refreshRoom";
/// `endpoint`: which model endpoint the subagent provider is on. The energy
/// sampler asks only that endpoint for the factory's draw.
pub const KEY_ENDPOINT: &str = "endpoint";
/// `slots`: the two counts and how each came about, when a setting asked for
/// `auto`. A reader finds them here, never at the top level.
pub const KEY_SLOTS: &str = "slots";
/// `time`: the poll's own stamp, added when the file is written.
pub const KEY_TIME: &str = "time";

/// The reason a poll starts nothing, as `why` writes it. A word is stable
/// once the manual names it: a person reads it in `tehas status` and in the
/// health file, and a grep of a host's history is worth the extra arm here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// The slice could not be read or used: nothing is known about room.
    Slice,
    /// The slice's use could not be read.
    Unreadable,
    /// No model endpoint answered.
    Resources,
    /// The disk is below the floor, or its free space could not be read.
    Disk,
    /// The limit of `tehas limit` holds.
    Limit,
    /// The branch's check is red at its head commit.
    Red,
    /// The branch's check finished and none of its runs says it passed.
    UnknownGate,
    /// Every slot of the ceiling is taken by a running cycle.
    Count,
    /// The cap on unreviewed work holds.
    Awaiting,
    /// The slice has no room for another container.
    Room,
    /// The slice is under memory pressure.
    Pressure,
    /// The containers' ceilings fill the slice, so no more fit.
    Full,
    /// The refresh lane keeps the room the slice would otherwise give.
    Lane,
}

impl Why {
    /// The thirteen words, in the order of the reasons a poll meets them.
    pub const ALL: [Why; 13] = [
        Why::Slice,
        Why::Unreadable,
        Why::Resources,
        Why::Disk,
        Why::Limit,
        Why::Red,
        Why::UnknownGate,
        Why::Count,
        Why::Awaiting,
        Why::Room,
        Why::Pressure,
        Why::Full,
        Why::Lane,
    ];

    /// The word `why` holds.
    pub fn as_str(self) -> &'static str {
        match self {
            Why::Slice => "slice",
            Why::Unreadable => "unreadable",
            Why::Resources => "resources",
            Why::Disk => "disk",
            Why::Limit => "limit",
            Why::Red => "red",
            Why::UnknownGate => "unknown-gate",
            Why::Count => "count",
            Why::Awaiting => "awaiting",
            Why::Room => "room",
            Why::Pressure => "pressure",
            Why::Full => "full",
            Why::Lane => "lane",
        }
    }

    /// The word back, or None for anything that is not one of them: a word a
    /// reader does not know is no reason, and the reader keeps its own copy
    /// of what a report of the future might say.
    pub fn parse(word: &str) -> Option<Why> {
        Why::ALL.into_iter().find(|w| w.as_str() == word)
    }
}

/// The report of one poll: what held it, and the numbers it decided by. The
/// fields the poll may not know are Options, and `to_value` leaves them out
/// rather than write a null, so a reader asks whether a key is there.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    /// The reason nothing starts; None when cycles may start.
    pub why: Option<Why>,
    /// The line of that reason, empty when nothing holds.
    pub hold: String,
    /// The cycles running when the poll decided.
    pub count: i64,
    /// The containers the slice's ceilings fit, from its memory.
    pub room: u64,
    /// The bytes the slice's children have been promised.
    pub committed: u64,
    /// The slice's memory max in bytes.
    pub max: u64,
    /// What one cycle's container is given, as the setting writes it.
    pub reserve: String,
    /// The slots free of the poll's own ceiling.
    pub free: i64,
    /// Room left under the cap on unreviewed work; `to_value` writes null.
    pub await_room: Option<i64>,
    /// Room left under the limit; left out when the limit did not answer.
    pub limit_room: Option<i64>,
    /// Slots free in the refresh lane; left out when the lane is not used.
    pub refresh_room: Option<i64>,
    /// The endpoint the subagent provider is on; left out when none answered.
    pub endpoint: Option<String>,
    /// The derived counts, as `slots::Slots::report` writes them; left out
    /// when neither setting asked for `auto`.
    pub slots: Option<serde_json::Value>,
}

impl Report {
    /// The report as the file holds it: every key of a poll that got this
    /// far, the optional ones only when the poll knew them. No `time` - the
    /// poll stamps its own when it writes the file.
    pub fn to_value(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        m.insert(
            KEY_WHY.into(),
            serde_json::Value::from(self.why.map_or("", |w| w.as_str())),
        );
        m.insert(KEY_HOLD.into(), serde_json::Value::from(self.hold.as_str()));
        m.insert(KEY_COUNT.into(), serde_json::Value::from(self.count));
        m.insert(KEY_ROOM.into(), serde_json::Value::from(self.room));
        m.insert(
            KEY_COMMITTED.into(),
            serde_json::Value::from(self.committed),
        );
        m.insert(KEY_MAX.into(), serde_json::Value::from(self.max));
        m.insert(
            KEY_RESERVE.into(),
            serde_json::Value::from(self.reserve.as_str()),
        );
        m.insert(KEY_FREE.into(), serde_json::Value::from(self.free));
        m.insert(
            KEY_AWAIT_ROOM.into(),
            self.await_room
                .map_or(serde_json::Value::Null, |n| serde_json::Value::from(n)),
        );
        if let Some(n) = self.limit_room {
            m.insert(KEY_LIMIT_ROOM.into(), serde_json::Value::from(n));
        }
        if let Some(n) = self.refresh_room {
            m.insert(KEY_REFRESH_ROOM.into(), serde_json::Value::from(n));
        }
        if let Some(e) = &self.endpoint {
            m.insert(KEY_ENDPOINT.into(), serde_json::Value::from(e.as_str()));
        }
        if let Some(s) = &self.slots {
            m.insert(KEY_SLOTS.into(), s.clone());
        }
        serde_json::Value::Object(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_word_is_a_distinct_lowercase_word() {
        let words: Vec<&str> = Why::ALL.iter().map(|w| w.as_str()).collect();
        assert_eq!(words.len(), Why::ALL.len());
        let mut sorted = words.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), words.len(), "{words:?}");
        for w in words {
            assert!(!w.is_empty() && w.len() <= 40, "{w}");
            assert!(
                w.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'),
                "{w}"
            );
        }
    }

    #[test]
    fn parse_inverts_as_str() {
        for w in Why::ALL {
            assert_eq!(Why::parse(w.as_str()), Some(w), "{w:?}");
        }
        for not in [
            "",
            "Slice",
            "unknown_gate",
            "unknow-gate",
            "lane ",
            " lane",
            "resource",
            "limit-ed",
        ] {
            assert_eq!(Why::parse(not), None, "{not:?}");
        }
    }

    #[test]
    fn the_words_the_readers_name_are_there() {
        // `resources` is the word the status page reads to tell a hold of the
        // models from any other; `endpoint` is the key the energy sampler asks
        // about. Both are named by a reader outside the dispatcher, so a
        // rename that dropped either would be a silent break.
        assert_eq!(Why::parse("resources"), Some(Why::Resources));
        assert_eq!(Why::Resources.as_str(), "resources");
        assert_eq!(KEY_ENDPOINT, "endpoint");
        assert_eq!(KEY_WHY, "why");
        assert_eq!(KEY_SLOTS, "slots");
        assert_eq!(KEY_TIME, "time");
    }

    #[test]
    fn a_report_with_no_why_writes_the_empty_word() {
        let v = Report::default().to_value();
        assert_eq!(v[KEY_WHY], json!(""));
        assert_eq!(v[KEY_HOLD], json!(""));
        assert_eq!(v[KEY_AWAIT_ROOM], json!(null));
        // Keys the poll did not know are not in the file at all.
        for k in [
            KEY_LIMIT_ROOM,
            KEY_REFRESH_ROOM,
            KEY_ENDPOINT,
            KEY_SLOTS,
            KEY_TIME,
        ] {
            assert!(v.get(k).is_none(), "{k}");
        }
    }

    #[test]
    fn a_full_report_writes_every_key_with_the_slots_nested() {
        let r = Report {
            why: Some(Why::Count),
            hold: "holding: 2 cycles are running (limit 2)".into(),
            count: 2,
            room: 3,
            committed: 1024,
            max: 2048,
            reserve: "2g".into(),
            free: 0,
            await_room: Some(1),
            limit_room: Some(0),
            refresh_room: Some(1),
            endpoint: Some("backup".into()),
            slots: Some(json!({"cycles": 2, "cyclesHow": "auto"})),
        };
        let v = r.to_value();
        assert_eq!(v[KEY_WHY], json!("count"));
        assert_eq!(v[KEY_COUNT], json!(2));
        assert_eq!(v[KEY_ROOM], json!(3));
        assert_eq!(v[KEY_COMMITTED], json!(1024));
        assert_eq!(v[KEY_MAX], json!(2048));
        assert_eq!(v[KEY_RESERVE], json!("2g"));
        assert_eq!(v[KEY_FREE], json!(0));
        assert_eq!(v[KEY_AWAIT_ROOM], json!(1));
        assert_eq!(v[KEY_LIMIT_ROOM], json!(0));
        assert_eq!(v[KEY_REFRESH_ROOM], json!(1));
        assert_eq!(v[KEY_ENDPOINT], json!("backup"));
        assert_eq!(v[KEY_SLOTS]["cyclesHow"], json!("auto"));
        // The slots are the report's own key: never merged at the top level.
        assert!(v.get("cycles").is_none());
        assert!(v.get("cyclesHow").is_none());
    }
}
