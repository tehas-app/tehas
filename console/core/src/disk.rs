//! The free space a command holds work on: the free kilobytes of the file
//! system of a path and of the one Docker keeps its images on, whichever has
//! less. No figure is never the same as plenty: a command that starts work on
//! a guess would start it on a host whose df or Docker is broken.
//!
//! The caller runs the programs, each under its own bound.

const DOCKER_HOST: &str = "/var/lib/docker";

/// A figure df prints for a real file system: a whole number that still fits
/// a 64-bit one, so the caller's arithmetic cannot wrap it into another.
pub fn figure(word: &str) -> Option<u64> {
    let plain = word == "0"
        || (!word.is_empty()
            && word.len() <= 18
            && !word.starts_with('0')
            && word.bytes().all(|b| b.is_ascii_digit()));
    plain.then(|| word.parse().ok()).flatten()
}

/// The free column of what `df -Pk <path>` printed: the fourth of its second
/// line.
pub fn df_free(said: &str) -> Option<u64> {
    figure(said.lines().nth(1)?.split_whitespace().nth(3)?)
}

/// `ask` runs a program and gives what it printed, or None when it failed or
/// ran out of time. A Docker that does not name its directory is the host of
/// /var/lib/docker: what Docker says is not guessed for it.
pub fn free_kb(path: &str, mut ask: impl FnMut(&str, &[&str]) -> Option<String>) -> Option<u64> {
    let home = df_free(&ask("df", &["-Pk", path])?)?;
    let said = ask("docker", &["info", "--format", "{{.DockerRootDir}}"]).unwrap_or_default();
    let dir = match said.trim() {
        "" => DOCKER_HOST,
        dir => dir,
    };
    let docker = df_free(&ask("df", &["-Pk", dir])?)?;
    Some(home.min(docker))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "Filesystem 1024-blocks Used Available Capacity Mounted on\n";

    fn df(free: &str) -> String {
        format!("{HEAD}/dev/x 60000000 1 {free} 1% /\n")
    }

    #[test]
    fn a_figure_is_a_plain_whole_number_that_fits() {
        assert_eq!(figure("0"), Some(0));
        assert_eq!(figure("41943040"), Some(41943040));
        assert_eq!(figure("999999999999999999"), Some(999999999999999999));
        assert_eq!(figure("1000000000000000000"), None);
        for word in ["", "00", "012", "-1", "+1", "1.5", "12k", "1 2", "٣"] {
            assert_eq!(figure(word), None, "{word}");
        }
    }

    #[test]
    fn the_fourth_column_of_the_second_line_is_the_figure() {
        assert_eq!(df_free(&df("123")), Some(123));
        assert_eq!(df_free(&format!("{HEAD}/dev/x\t1  2\t77 1% /\n")), Some(77));
    }

    #[test]
    fn df_with_no_second_line_or_too_few_columns_says_nothing() {
        assert_eq!(df_free(""), None);
        assert_eq!(df_free(HEAD), None);
        assert_eq!(df_free(&format!("{HEAD}/dev/x 1 2\n")), None);
        assert_eq!(df_free(&df("many")), None);
    }

    #[test]
    fn a_third_line_of_df_is_not_read() {
        assert_eq!(df_free(&format!("{}/dev/y 1 1 9 1% /\n", df("5"))), Some(5));
    }

    /// A host: `df` of the home, what Docker says, `df` of its directory.
    fn on(
        home: Option<&str>,
        docker: Option<&str>,
        images: Option<&str>,
    ) -> (Option<u64>, Vec<String>) {
        let mut calls = Vec::new();
        let kb = free_kb("/home/tehas", |program, args| {
            calls.push(format!("{program} {}", args.join(" ")));
            match (program, args[1]) {
                ("df", "/home/tehas") => home.map(df),
                ("df", _) => images.map(df),
                _ => docker.map(str::to_string),
            }
        });
        (kb, calls)
    }

    #[test]
    fn the_smaller_of_the_two_file_systems_is_the_answer() {
        assert_eq!(on(Some("5"), Some("/srv\n"), Some("9")).0, Some(5));
        assert_eq!(on(Some("9"), Some("/srv\n"), Some("5")).0, Some(5));
    }

    #[test]
    fn docker_is_asked_where_its_images_are_and_that_is_what_df_is_given() {
        let (_, calls) = on(Some("5"), Some("/srv/docker\n"), Some("9"));
        assert_eq!(
            calls,
            [
                "df -Pk /home/tehas",
                "docker info --format {{.DockerRootDir}}",
                "df -Pk /srv/docker"
            ]
        );
    }

    #[test]
    fn a_docker_that_does_not_answer_is_the_host_of_its_default() {
        for said in [None, Some(""), Some("\n")] {
            let (kb, calls) = on(Some("5"), said, Some("9"));
            assert_eq!(kb, Some(5));
            assert_eq!(calls[2], "df -Pk /var/lib/docker");
        }
    }

    #[test]
    fn no_figure_of_either_is_no_answer() {
        assert_eq!(on(None, Some("/srv\n"), Some("9")).0, None);
        assert_eq!(on(Some("5"), Some("/srv\n"), None).0, None);
        assert_eq!(on(Some("word"), Some("/srv\n"), Some("9")).0, None);
        assert_eq!(on(Some("5"), Some("/srv\n"), Some("word")).0, None);
    }

    #[test]
    fn a_home_with_no_figure_asks_nothing_more() {
        assert_eq!(on(None, Some("/srv\n"), Some("9")).1.len(), 1);
    }
}
