//! The start order `tehas order` sets: the issues to start first, kept in the
//! state store (table `start_order`, store verbs `order-show`, `order-set` and
//! `order-clear`). The store answers a reading as the object `value` gives;
//! the console, the dispatcher and the health page read it back with
//! `Reading::of`, so all of them give the same answer for the same row.
//!
//! - `issues`: the issue numbers to start first, in order, at most 200
//! - `only`: true holds every ready issue that is not in `issues`
//! - `until`: when it expires; none while it lasts until cleared
//! - `reason`: one line of at most 200 characters; none when empty
//!
//! `state` is `none` with no row, `ok`, `expired` once `until` has passed,
//! and `unreadable` when the store could not be read. Only `ok` orders
//! anything: an expired or unreadable order is no order, so a factory starts
//! ready issues as it would alone.

use crate::limit::{epoch, oneline};
use crate::time::iso;
use serde_json::{json, Value};

/// The most issues an order holds.
pub const MAX_ISSUES: usize = 200;

/// The order as kept.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Order {
    pub issues: Vec<u64>,
    pub only: bool,
    /// Seconds since the epoch.
    pub until: Option<i64>,
    pub reason: Option<String>,
}

/// A reading at a moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reading {
    None,
    Ok(Order),
    Expired(Order),
    Unreadable,
}

impl Reading {
    /// The order on record, read at `now`.
    pub fn at(order: Option<Order>, now: i64) -> Reading {
        match order {
            None => Reading::None,
            Some(o) if o.until.is_some_and(|u| u <= now) => Reading::Expired(o),
            Some(o) => Reading::Ok(o),
        }
    }

    pub fn state(&self) -> &'static str {
        match self {
            Reading::None => "none",
            Reading::Ok(_) => "ok",
            Reading::Expired(_) => "expired",
            Reading::Unreadable => "unreadable",
        }
    }

    /// The order in force: only an `ok` reading has one.
    pub fn in_force(&self) -> Option<&Order> {
        match self {
            Reading::Ok(o) => Some(o),
            _ => None,
        }
    }

    /// The reading as one compact object, `state` first, then the fields of
    /// the order with no field that has no value: what the store answers and
    /// `tehas status --json` publishes.
    pub fn json(&self) -> String {
        let mut parts = vec![format!("\"state\":\"{}\"", self.state())];
        if let Reading::Ok(o) | Reading::Expired(o) = self {
            parts.push(format!("\"issues\":{}", json!(o.issues)));
            parts.push(format!("\"only\":{}", o.only));
            if let Some(u) = o.until {
                parts.push(format!("\"until\":{}", json!(iso(u))));
            }
            if let Some(r) = &o.reason {
                parts.push(format!("\"reason\":{}", json!(r)));
            }
        }
        format!("{{{}}}", parts.join(","))
    }

    pub fn value(&self) -> Value {
        serde_json::from_str(&self.json()).unwrap_or_else(|_| json!({"state": "unreadable"}))
    }

    /// The reading of an object `json` made. Anything else is unreadable.
    pub fn of(v: &Value) -> Reading {
        let state = v.get("state").and_then(Value::as_str);
        if state == Some("none") {
            return Reading::None;
        }
        let order = || -> Option<Order> {
            let issues: Option<Vec<u64>> = v
                .get("issues")?
                .as_array()?
                .iter()
                .map(Value::as_u64)
                .collect();
            let until = match v.get("until") {
                None => None,
                Some(u) => Some(epoch(u.as_str()?)?),
            };
            let reason = match v.get("reason") {
                None => None,
                Some(r) => Some(r.as_str()?.to_string()),
            };
            let o = Order {
                issues: issues?,
                only: v.get("only")?.as_bool()?,
                until,
                reason,
            };
            o.check().ok().map(|()| o)
        };
        match (state, order()) {
            (Some("ok"), Some(o)) => Reading::Ok(o),
            (Some("expired"), Some(o)) => Reading::Expired(o),
            _ => Reading::Unreadable,
        }
    }

    /// The words `tehas order` and `tehas status` print.
    pub fn said(&self) -> String {
        let words = match self {
            Reading::None => "none: ready issues start lowest number first".to_string(),
            Reading::Unreadable => {
                "unreadable: taken as none, so ready issues start lowest number first".to_string()
            }
            Reading::Expired(o) => format!(
                "expired at {}: ready issues start lowest number first",
                o.until.map(iso).unwrap_or_default()
            ),
            Reading::Ok(o) => {
                let list: Vec<String> = o.issues.iter().map(|n| format!("#{n}")).collect();
                let first = if list.is_empty() {
                    "no issue".to_string()
                } else {
                    list.join(", ")
                };
                let rest = if o.only {
                    "nothing else starts"
                } else {
                    "then the rest"
                };
                let until = match o.until {
                    Some(u) => format!("until {}", iso(u)),
                    None => "until cleared".to_string(),
                };
                match &o.reason {
                    Some(r) => format!("{first} first, {rest}, {until}: {r}"),
                    None => format!("{first} first, {rest}, {until}"),
                }
            }
        };
        format!("order     {words}")
    }
}

impl Order {
    /// Whether the order may be kept: distinct numbers from 1, at most
    /// `MAX_ISSUES`, a reason already on one line.
    pub fn check(&self) -> Result<(), String> {
        if self.issues.len() > MAX_ISSUES {
            return Err(format!("at most {MAX_ISSUES} issues"));
        }
        if self.issues.contains(&0) {
            return Err("issue numbers start at 1".into());
        }
        let mut seen = self.issues.clone();
        seen.sort_unstable();
        seen.dedup();
        if seen.len() != self.issues.len() {
            return Err("an issue named twice".into());
        }
        if let Some(r) = &self.reason {
            if r.is_empty() || oneline(r) != *r {
                return Err(
                    "a reason of one line, at most 200 characters, with no space at either end"
                        .into(),
                );
            }
        }
        Ok(())
    }
}

/// `12,8,31` as numbers, or None. An empty text is the empty list: with
/// `only`, an order that starts nothing. Distinctness is `Order::check`'s.
pub fn list(v: &str) -> Option<Vec<u64>> {
    if v.is_empty() {
        return Some(Vec::new());
    }
    let out: Option<Vec<u64>> = v
        .split(',')
        .map(|w| {
            (!w.is_empty() && w.len() <= 12 && w.bytes().all(|c| c.is_ascii_digit()))
                .then(|| w.parse().ok())
                .flatten()
        })
        .collect();
    out.filter(|o| o.len() <= MAX_ISSUES)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_793_404_800; // 2026-10-31T00:00:00Z

    fn order(issues: &[u64], only: bool) -> Order {
        Order {
            issues: issues.to_vec(),
            only,
            ..Order::default()
        }
    }

    #[test]
    fn a_reading_reads_back() {
        let o = Order {
            until: Some(NOW + 86_400),
            reason: Some("held for review".into()),
            ..order(&[12, 8], true)
        };
        let r = Reading::at(Some(o.clone()), NOW);
        assert_eq!(
            r.json(),
            "{\"state\":\"ok\",\"issues\":[12,8],\"only\":true,\"until\":\"2026-11-01T00:00:00Z\",\"reason\":\"held for review\"}"
        );
        assert_eq!(Reading::of(&r.value()), Reading::Ok(o));
    }

    #[test]
    fn an_order_past_its_until_is_expired() {
        let o = Order {
            until: Some(NOW - 1),
            ..order(&[3], false)
        };
        let r = Reading::at(Some(o), NOW);
        assert_eq!(r.state(), "expired");
        assert!(r.in_force().is_none());
        assert_eq!(
            r.said(),
            "order     expired at 2026-10-30T23:59:59Z: ready issues start lowest number first"
        );
        assert_eq!(Reading::of(&r.value()), r);
    }

    #[test]
    fn an_order_with_no_until_lasts() {
        let r = Reading::at(Some(order(&[3], false)), NOW + 10_000_000);
        assert_eq!(r.state(), "ok");
        assert_eq!(r.said(), "order     #3 first, then the rest, until cleared");
    }

    #[test]
    fn anything_else_reads_as_unreadable() {
        for bad in [
            json!(null),
            json!([]),
            json!({"issues": [1], "only": false}),
            json!({"state": "ok", "issues": [1, 1], "only": false}),
            json!({"state": "ok", "issues": [0], "only": false}),
            json!({"state": "ok", "issues": ["1"], "only": false}),
            json!({"state": "ok", "issues": [1]}),
            json!({"state": "ok", "issues": [1], "only": false, "until": "soon"}),
            json!({"state": "ok", "issues": [1], "only": false, "reason": 7}),
            json!({"state": "unreadable"}),
        ] {
            assert_eq!(Reading::of(&bad), Reading::Unreadable, "{bad}");
        }
        assert_eq!(Reading::of(&json!({"state": "none"})), Reading::None);
    }

    #[test]
    fn no_value_is_left_in() {
        let r = Reading::Ok(order(&[5], true));
        assert_eq!(r.json(), "{\"state\":\"ok\",\"issues\":[5],\"only\":true}");
        assert_eq!(Reading::None.json(), "{\"state\":\"none\"}");
    }

    #[test]
    fn lists_and_bounds() {
        assert_eq!(list("12,8"), Some(vec![12, 8]));
        assert_eq!(list(""), Some(vec![]));
        for bad in ["1,,2", ",", "-1", "a", "1 2"] {
            assert_eq!(list(bad), None, "{bad}");
        }
        let many: Vec<u64> = (1..=201).collect();
        assert!(order(&many, false).check().is_err());
        let enough: Vec<u64> = (1..=200).collect();
        assert!(order(&enough, false).check().is_ok());
        assert!(order(&[1, 1], false).check().is_err());
    }
}
