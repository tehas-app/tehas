//! `CodeHost::changes` of a filter that names numbers: those pull requests
//! alone, asked by number, an alias each, 100 a question, so a repository
//! with many pull requests is not paged to find a few. Each is read as a
//! page of the list reads it (`PullNode::read`), and the rest of the filter
//! is applied to what the numbers answer: the states, the author, the
//! branch, the marks carried, the blueprints closed and the times. The
//! question is read as `items.rs` reads an aliased one; a number that
//! names no pull request is left out (Decided 121).

use serde_json::Value;

use super::{pull_fields, PullNode};
use crate::items::alias;
use crate::GitHub;
use tehas_ports::{
    AccountId, Author, Bound, ChangeDigest, ChangeFilter, Got, Outcome, Unread, Whole, ALL_MAX,
};

/// The most numbers one question asks: more go in another question.
const PER_QUESTION: usize = 100;

/// The question of the pull requests `numbers`, an alias each, with the
/// repository's id, which the handle learns as this project's, and the
/// account asking when `me`.
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
    /// filter takes.
    fn numbers_asked(
        &mut self,
        numbers: &[u64],
        me: bool,
        filter: &ChangeFilter,
    ) -> Got<Vec<ChangeDigest>> {
        let question = question(numbers, me, filter.bodies);
        let aliases: Vec<String> = numbers.iter().map(|n| alias(*n)).collect();
        let asked = self.ask_aliased_beside::<Value>("the pull requests", &question, &aliases);
        // A failure of the question, or of a number, fails the list before
        // anything else is read of it.
        for node in &asked.nodes {
            match node {
                Ok(_) | Err(Outcome::NotFound) => {}
                Err(o) => return Err(o.clone()),
            }
        }
        let viewer = match (me, asked.viewer_id) {
            (false, _) => None,
            (true, Some(id)) => Some(AccountId::new(id.to_string())),
            (true, None) => return Err(Outcome::Unread(Unread::Malformed)),
        };
        let mut taken = Vec::new();
        for (n, node) in numbers.iter().zip(asked.nodes) {
            let Ok(node) = node else {
                continue;
            };
            let read = PullNode { node: &node }.read(self, viewer.as_ref(), filter)?;
            if read.no.get() != *n {
                return Err(Outcome::Unread(Unread::Inconsistent));
            }
            let Some(digest) = read.pull else {
                continue;
            };
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
