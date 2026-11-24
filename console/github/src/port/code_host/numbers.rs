//! `CodeHost::changes` of a filter that names numbers: those pull requests
//! alone, asked by number, an alias each, 100 a question, so a repository
//! with many pull requests is not paged to find a few. Each is read as a
//! page of the list reads it (`read_node`), and the rest of the filter is
//! applied to what the numbers answer: the states, the author, the branch,
//! the marks carried, the blueprints closed and the times. A number that
//! names no pull request is left out (Decided 121).

use serde_json::Value;

use super::{pull_fields, read_node};
use crate::aliases::alias;
use crate::GitHub;
use tehas_ports::{
    AccountId, Author, Bound, ChangeDigest, ChangeFilter, Got, Outcome, Pool, Unread, Whole,
    ALL_MAX,
};

/// The most numbers one question asks: more go in another question.
const PER_QUESTION: usize = 100;

/// The question of the pull requests `numbers`, an alias each, with the
/// repository's id, and the account asking when `me`.
fn question(numbers: &[u64], me: bool, bodies: bool) -> String {
    let viewer = if me { "viewer { databaseId } " } else { "" };
    let fields = pull_fields(bodies);
    let mut q = format!(
        "query($owner: String!, $name: String!) {{ {viewer}repository(owner: $owner, name: $name) {{ databaseId"
    );
    for n in numbers {
        q.push_str(&format!(
            " {}: pullRequest(number: {n}) {{ {fields} }}",
            alias(*n)
        ));
    }
    q.push_str(" } }");
    q
}

impl GitHub {
    /// The pull requests of the numbers the filter names that it takes, in
    /// number order. More distinct numbers than `ALL_MAX`, or more taken
    /// than `bound`, is `Unread(Cut)`, the first before any request. A
    /// number GitHub does not know is left out; any other error at a
    /// number, or of the question, fails the read, never a partial list.
    pub(super) fn changes_by_number(
        &mut self,
        filter: &ChangeFilter,
        bound: Bound,
    ) -> Got<Whole<ChangeDigest>> {
        let mut numbers: Vec<u64> = filter.numbers.iter().map(|n| n.get()).collect();
        numbers.sort_unstable();
        numbers.dedup();
        if numbers.len() > ALL_MAX.get() as usize {
            return Err(Outcome::Unread(Unread::Cut));
        }
        let me = matches!(filter.author, Some(Author::Me));
        let mut taken = Vec::new();
        for chunk in numbers.chunks(PER_QUESTION) {
            for digest in self.numbers_asked(chunk, me, filter)? {
                taken.push(digest);
                if taken.len() > bound.get() as usize {
                    return Err(Outcome::Unread(Unread::Cut));
                }
            }
        }
        Ok(Whole::new(taken))
    }

    /// One question of distinct numbers, in order: the pull requests the
    /// filter takes, with their marks.
    fn numbers_asked(
        &mut self,
        numbers: &[u64],
        me: bool,
        filter: &ChangeFilter,
    ) -> Got<Vec<ChangeDigest>> {
        let malformed = Outcome::Unread(Unread::Malformed);
        let (owner, name) = self.owner_name();
        let query = format!("query={}", question(numbers, me, filter.bodies));
        let args = ["api", "graphql", "-f", &owner, "-f", &name, "-f", &query];
        let aliases: Vec<String> = numbers.iter().map(|n| alias(*n)).collect();
        let asked = self
            .ask_aliased::<Value>("the pull requests", &args, &aliases)
            .map_err(|o| match o {
                Outcome::Spent { until, .. } => Outcome::Spent {
                    pool: Pool::Priced,
                    until,
                },
                o => o,
            })?;
        let here = match asked.repository.get("databaseId") {
            Some(Some(id)) => Some(id.as_u64().ok_or(malformed.clone())?),
            _ => None,
        };
        let viewer = match (me, asked.viewer.as_ref().map(|v| v.get("databaseId"))) {
            (false, _) => None,
            (true, Some(Some(id))) => Some(AccountId::new(
                id.as_u64().ok_or(malformed.clone())?.to_string(),
            )),
            (true, _) => return Err(malformed),
        };
        let mut taken = Vec::new();
        for (n, node) in numbers.iter().zip(asked.nodes) {
            let node = match node {
                Ok(node) => node,
                Err(Outcome::NotFound) => continue,
                Err(o) => return Err(o),
            };
            let read = read_node(&node, here, viewer.as_ref(), filter, self.bot.as_ref())?;
            if read.no.get() != *n {
                return Err(Outcome::Unread(Unread::Inconsistent));
            }
            let Some(pull) = read.pull else {
                continue;
            };
            let mut digest = pull.digest;
            digest.marks = self.marks_of(&pull.labels);
            if filter.carrying.iter().all(|m| digest.marks.contains(m)) {
                taken.push(digest);
            }
        }
        Ok(taken)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_number_is_asked_under_its_own_alias_with_the_lists_fields() {
        let q = question(&[12, 34], false, false);
        let fields = pull_fields(false);
        assert!(q.contains(&format!(" n12: pullRequest(number: 12) {{ {fields} }}")));
        assert!(q.contains(&format!(" n34: pullRequest(number: 34) {{ {fields} }}")));
        assert!(q.contains("repository(owner: $owner, name: $name) { databaseId n12:"));
        assert!(!q.contains("viewer"));
        assert!(question(&[1], true, false).contains("{ viewer { databaseId } repository("));
        assert!(question(&[1], false, true).contains(" body "));
    }
}
