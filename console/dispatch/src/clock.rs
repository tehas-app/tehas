//! The time now as the dispatcher's logs write it. The shell reads the clock;
//! the core formats a time it is given.

use crate::decide::iso;

pub(crate) fn iso_now() -> String {
    iso(tehas_core::time::now() as u64)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_time_now_is_the_clock_of_the_shell_in_the_cores_format() {
        let before = tehas_core::time::now();
        let at = tehas_core::time::from_iso(&super::iso_now()).unwrap();
        assert!(before <= at && at <= tehas_core::time::now());
    }
}
