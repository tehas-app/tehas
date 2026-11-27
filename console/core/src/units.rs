//! The systemd units tehas names by number. The hook's unit is
//! `plugin::hook::unit`.

use std::fmt::Display;

/// `tehas-cycle@<n>.service`: the unit of the cycle of issue `n`.
pub fn cycle(n: impl Display) -> String {
    format!("tehas-cycle@{n}.service")
}
