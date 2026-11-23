//! The fake's code host port, over the world.

use std::io::Read as _;

use super::faulty::{CodeHostOp, Op};
use super::site::{
    comments_of, edit_markers, known_mark, text_of, whole, window, writable, Ctx, Handle,
};
use super::state::{ChangeRec, ProposalRec, ReleaseRec, State};
use super::tickets::{close_item, page, since, token};
use crate::{
    Access, Actor, Applied, Blobs, Branch, Change, ChangeDelta, ChangeDigest, ChangeEvent,
    ChangeFilter, ChangeHappened, ChangeNo, ChangeState, ChangeStates, ChangeToken, ChangedFile,
    CheckStatus, Checks, CloseReason, Closer, CodeHost, CodeHostWrites, CommentId, CommitId,
    Compared, Conclusion, Cursor, Did, Facts, FileName, GitCredential, GitRemote, Got, Lost, Mark,
    Marker, MarkerEdit, MergeHow, Mergeable, NewChange, NewRelease, Outcome, Overall, Pool, Posted,
    Proposal, ProposalPart, Proposed, Ref, Refusal, Release, ReleaseFile, ReleaseId, Remote,
    RepoPath, Rev, Secret, Status, Sum, Tag, TagName, Takeover, Token, Took, Unread, WebLink,
    Whole, Window, Write,
};
use crate::{Author, Bound};

/// A change's fields, its body and what its text closes when `bodies`.
pub(crate) fn change_digest(s: &State, c: &ChangeRec, bodies: bool) -> ChangeDigest {
    let closes: Vec<_> = c.closes.iter().map(|n| Ref::Here(*n)).collect();
    let mut d = ChangeDigest::new(
        c.no,
        c.state.clone(),
        c.head.clone(),
        c.head_commit.clone(),
        c.base.clone(),
        c.base_commit.clone(),
        c.created,
        updated_of(s, c),
        s.change_link(c.no),
    );
    d.title = c.title.clone();
    d.author = c.author.clone();
    d.draft = c.draft;
    d.merge_state = c.merge_state;
    d.marks = c.marks.clone();
    if bodies {
        d.says_closes = closes.clone();
        d.body = Some(c.body.clone());
    }
    d.closes = closes;
    d.size = Some(crate::Size {
        files: c.files.len() as u32,
        added: c.files.iter().map(|f| f.added).sum(),
        deleted: c.files.iter().map(|f| f.deleted).sum(),
    });
    d.edited = c.edited;
    d.editor = c.editor.clone();
    d
}

/// When the fake last saw a change change: its making, edit, end, events
/// and comments.
fn updated_of(s: &State, c: &ChangeRec) -> crate::At {
    let comments = c
        .comments
        .iter()
        .filter_map(|id| s.comment_at(id))
        .flat_map(|at| {
            let k = &s.comments[at];
            [Some(k.created), k.edited]
        });
    [Some(c.created), c.edited, ended_at(c)]
        .into_iter()
        .chain(c.events.iter().map(|e| Some(e.at)))
        .chain(comments)
        .flatten()
        .max()
        .unwrap_or(c.created)
}

fn at_change(s: &State, no: ChangeNo) -> Got<usize> {
    s.change_at(no).ok_or(Outcome::NotFound)
}

fn resolve(s: &State, at: &Rev) -> Got<CommitId> {
    match at {
        Rev::Commit(c) => s.commit(c).map(|c| c.id.clone()).ok_or(Outcome::NotFound),
        Rev::Branch(b) => s.branch_at(b).ok_or(Outcome::NotFound),
        Rev::Tag(t) => s
            .tags
            .iter()
            .find(|x| x.name == *t)
            .map(|x| x.commit.clone())
            .ok_or(Outcome::NotFound),
    }
}

fn ended_at(c: &ChangeRec) -> Option<crate::At> {
    match c.state {
        ChangeState::Open => None,
        ChangeState::Merged { at, .. } | ChangeState::Closed { at } => Some(at),
    }
}

fn taken(s: &State, ctx: &Ctx, c: &ChangeRec, f: &ChangeFilter) -> bool {
    let state_ok = matches!(
        (f.states, &c.state),
        (ChangeStates::All, _)
            | (ChangeStates::Open, ChangeState::Open)
            | (ChangeStates::Merged, ChangeState::Merged { .. })
            | (ChangeStates::Closed, ChangeState::Closed { .. })
    );
    let author_ok = match &f.author {
        None => true,
        Some(Author::Me) => c.author == Some(ctx.me(s)),
        Some(Author::Account(id)) => c.author.as_ref().and_then(|a| a.id.as_ref()) == Some(id),
    };
    let merged_ok = f
        .merged_since
        .is_none_or(|since_at| matches!(c.state, ChangeState::Merged { at, .. } if at >= since_at));
    let updated = updated_of(s, c);
    state_ok
        && author_ok
        && merged_ok
        && f.head.as_ref().is_none_or(|h| c.head == *h)
        && (f.closing.is_empty() || c.closes.iter().any(|n| f.closing.contains(n)))
        && f.updated_since.is_none_or(|u| updated >= u)
        && (f.numbers.is_empty() || f.numbers.contains(&c.no))
}

/// The bytes of a file a write sends, from the caller's `Blobs` by sum. A
/// sum it does not hold, or bytes of another length, is
/// `Refused(Invalid)`: the fake cannot check the digest, as it never
/// hashes, so it checks the length.
fn blob(files: &dyn Blobs, sum: &Sum) -> Got<Vec<u8>> {
    let mut reader = files.open(sum).ok_or(Outcome::Refused(Refusal::Invalid))?;
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|_| Outcome::Refused(Refusal::Invalid))?;
    if bytes.len() as u64 != sum.len {
        return Err(Outcome::Refused(Refusal::Invalid));
    }
    Ok(bytes)
}

fn write_out(bytes: &[u8], limit: u64, into: &mut dyn std::io::Write) -> Did {
    if bytes.len() as u64 > limit {
        return Err(Outcome::Unread(Unread::TooLarge));
    }
    into.write_all(bytes)
        .map_err(|_| Outcome::Unread(Unread::Malformed))
}

/// Two messages compare as bytes once white space at the end of each is
/// taken off.
fn same_message(a: &str, b: &str) -> bool {
    let end = [' ', '\t', '\n', '\r'];
    a.trim_end_matches(end) == b.trim_end_matches(end)
}

impl CodeHost for Handle {
    fn facts(&mut self) -> Got<Facts> {
        self.run(Pool::Calls, |s, _| {
            Ok(Facts {
                name: s.project.clone(),
                public: s.public,
                tracker_on: s.tracker_on,
                archived: s.archived,
                default_branch: s.default_branch.clone(),
            })
        })
    }

    fn branch(&mut self, name: &Branch) -> Got<Option<CommitId>> {
        self.run(Pool::Calls, |s, _| Ok(s.branch_at(name)))
    }

    fn compare(&mut self, base: &CommitId, head: &CommitId) -> Got<Compared> {
        self.run(Pool::Calls, |s, _| {
            s.commit(base).ok_or(Outcome::NotFound)?;
            s.commit(head).ok_or(Outcome::NotFound)?;
            if base == head {
                return Ok(Compared::Identical);
            }
            let of_base = s.ancestors(base);
            let of_head = s.ancestors(head);
            let ahead = of_head.iter().filter(|c| !of_base.contains(c)).count() as u32;
            let behind = of_base.iter().filter(|c| !of_head.contains(c)).count() as u32;
            Ok(match (ahead, behind) {
                (0, 0) => Compared::Identical,
                (a, 0) => Compared::Ahead(a),
                (0, b) => Compared::Behind(b),
                _ => Compared::Diverged,
            })
        })
    }

    fn file(&mut self, path: &RepoPath, at: &Rev, limit: u64) -> Got<Vec<u8>> {
        self.run(Pool::Calls, |s, _| {
            let commit = resolve(s, at)?;
            let bytes = s
                .commit(&commit)
                .and_then(|c| c.files.iter().find(|(p, _)| p == path))
                .map(|(_, b)| b.clone())
                .ok_or(Outcome::NotFound)?;
            if bytes.len() as u64 > limit {
                return Err(Outcome::Unread(Unread::TooLarge));
            }
            Ok(bytes)
        })
    }

    /// In the order of their numbers.
    fn changes(&mut self, filter: &ChangeFilter, bound: Bound) -> Got<Whole<ChangeDigest>> {
        self.run(Pool::Calls, |s, ctx| {
            let mut all: Vec<&ChangeRec> = s
                .changes
                .iter()
                .filter(|c| taken(s, ctx, c, filter))
                .collect();
            all.sort_by_key(|c| c.no);
            whole(
                all.into_iter()
                    .map(|c| change_digest(s, c, filter.bodies))
                    .collect(),
                bound,
            )
        })
    }

    fn change(&mut self, no: ChangeNo, bound: Bound) -> Got<Change> {
        self.run(Pool::Calls, |s, _| {
            let c = &s.changes[at_change(s, no)?];
            Ok(Change {
                digest: change_digest(s, c, true),
                body: c.body.clone(),
                files: whole(c.files.clone(), bound)?.into_items(),
                reviews: whole(c.reviews.clone(), bound)?.into_items(),
                comments: whole(comments_of(s, &c.comments, false), bound)?.into_items(),
            })
        })
    }

    fn changed_files(&mut self, no: ChangeNo, bound: Bound) -> Got<Whole<ChangedFile>> {
        self.run(Pool::Calls, |s, _| {
            whole(s.changes[at_change(s, no)?].files.clone(), bound)
        })
    }

    fn activity(&mut self, no: ChangeNo, w: Window) -> Got<Took<ChangeEvent>> {
        self.run(Pool::Calls, |s, _| {
            window(&s.changes[at_change(s, no)?].events, w)
        })
    }

    /// The overall is `Fail` when a check failed or was cancelled,
    /// `Pending` while one runs, `Pass` when all passed or were skipped,
    /// `None` with no check.
    fn checks(&mut self, at: &CommitId, name: Option<&str>) -> Got<Checks> {
        self.run(Pool::Calls, |s, _| {
            s.commit(at).ok_or(Outcome::NotFound)?;
            let runs: Vec<crate::Check> = s
                .checks
                .iter()
                .filter(|(c, k)| c == at && name.is_none_or(|n| k.name == n))
                .map(|(_, k)| k.clone())
                .collect();
            let failed = runs.iter().any(|r| {
                matches!(
                    r.status,
                    CheckStatus::Done(Conclusion::Failed | Conclusion::Cancelled)
                )
            });
            let pending = runs
                .iter()
                .any(|r| !matches!(r.status, CheckStatus::Done(_)));
            let overall = if runs.is_empty() {
                Overall::None
            } else if failed {
                Overall::Fail
            } else if pending {
                Overall::Pending
            } else {
                Overall::Pass
            };
            Ok(Checks { runs, overall })
        })
    }

    fn tags(&mut self, bound: Bound) -> Got<Whole<Tag>> {
        self.run(Pool::Calls, |s, _| whole(s.tags.clone(), bound))
    }

    fn resolve_tag(&mut self, name: &TagName) -> Got<CommitId> {
        self.run(Pool::Calls, |s, _| resolve(s, &Rev::Tag(name.clone())))
    }

    fn releases(&mut self, bound: Bound) -> Got<Whole<Release>> {
        self.run(Pool::Calls, |s, _| {
            whole(
                s.releases.iter().map(|r| r.release.clone()).collect(),
                bound,
            )
        })
    }

    /// The newest published release that is not a prerelease.
    fn latest_release(&mut self) -> Got<Option<Release>> {
        self.run(Pool::Calls, |s, _| {
            Ok(s.releases
                .iter()
                .map(|r| &r.release)
                .filter(|r| !r.draft && !r.prerelease && r.published.is_some())
                .max_by_key(|r| r.published)
                .cloned())
        })
    }

    fn release(&mut self, tag: &TagName) -> Got<Release> {
        self.run(Pool::Calls, |s, _| {
            s.releases
                .iter()
                .find(|r| r.release.tag == *tag)
                .map(|r| r.release.clone())
                .ok_or(Outcome::NotFound)
        })
    }

    fn download(
        &mut self,
        tag: &TagName,
        file: &FileName,
        limit: u64,
        into: &mut dyn std::io::Write,
    ) -> Did {
        let bytes = self.run(Pool::Calls, |s, _| {
            let r = s
                .releases
                .iter()
                .find(|r| r.release.tag == *tag)
                .ok_or(Outcome::NotFound)?;
            let at = r
                .release
                .files
                .iter()
                .position(|f| f.name == *file)
                .ok_or(Outcome::NotFound)?;
            Ok(r.bytes[at].clone())
        })?;
        write_out(&bytes, limit, into)
    }

    /// The fake's archive: each file's path on a line, then its bytes, in
    /// the commit's order.
    fn archive(&mut self, at: &CommitId, limit: u64, into: &mut dyn std::io::Write) -> Did {
        let bytes = self.run(Pool::Calls, |s, _| {
            let c = s.commit(at).ok_or(Outcome::NotFound)?;
            let mut out = Vec::new();
            for (path, b) in &c.files {
                out.extend_from_slice(path.as_str().as_bytes());
                out.push(b'\n');
                out.extend_from_slice(b);
            }
            Ok(out)
        })?;
        write_out(&bytes, limit, into)
    }

    fn changed(&mut self, since_token: Option<&ChangeToken>) -> Got<(bool, ChangeToken)> {
        self.run(Pool::Calls, |s, _| {
            let now = token(s);
            Ok((since_token != Some(&now), now))
        })
    }

    fn delta(&mut self, from: Option<&Cursor>, bound: Bound) -> Got<ChangeDelta> {
        self.run(Pool::Calls, |s, _| {
            let after = since(from);
            let mut changes = Vec::new();
            let mut comments = Vec::new();
            for c in &s.changes {
                if c.seq > after {
                    changes.push((c.seq, change_digest(s, c, true)));
                }
                for id in &c.comments {
                    if let Some(at) = s.comment_at(id) {
                        let k = &s.comments[at];
                        if k.seq > after && !k.gone {
                            comments.push((k.seq, (c.no, super::site::comment(k, false))));
                        }
                    }
                }
            }
            let (changes, comments, next, more) = page(s, changes, comments, bound);
            let mut delta = ChangeDelta::new(next);
            delta.changes = changes;
            delta.comments = comments;
            delta.more = more;
            Ok(delta)
        })
    }

    /// A push needs an actor that may push: the factory's full, cycle or
    /// session scope, or a person of `Write` standing. An anonymous fetch
    /// has no credential.
    fn remote(&mut self, access: Access) -> Got<Remote> {
        let actor = self.actor.clone();
        let scope = self.scope;
        let project = self.project();
        let now = self.world.now();
        let world = self.world.clone();
        self.run(Pool::Calls, move |_, ctx| {
            let may_push = match &actor {
                Actor::Ours(_) | Actor::Given(_) => matches!(
                    scope,
                    Some(
                        crate::Scope::Full
                            | crate::Scope::Cycle
                            | crate::Scope::CycleMerge
                            | crate::Scope::Session
                    )
                ),
                Actor::Person(_) => !ctx.below_write,
                Actor::Operator | Actor::Deploy(_) => true,
                Actor::Anonymous => false,
            };
            if access == Access::Push && !may_push {
                return Err(Outcome::Forbidden);
            }
            let url = GitRemote::new(format!("https://fake.test/{}.git", project.as_str()))
                .map_err(|_| Outcome::Unread(Unread::Malformed))?;
            let token = |secret: String| {
                GitCredential::Token(Token {
                    secret: Secret::new(secret),
                    until: crate::At::new(now.secs() + super::site::TOKEN_LIFE),
                })
            };
            let with = match (&actor, access) {
                (Actor::Anonymous, Access::Fetch) => None,
                (Actor::Deploy(key), _) => match crate::Keyring::deploy(&world, key) {
                    Some(crate::DeployCredential::Key(k)) => Some(GitCredential::Key(k)),
                    Some(crate::DeployCredential::Token(t)) => Some(GitCredential::Token(Token {
                        secret: t,
                        until: crate::At::new(now.secs() + super::site::TOKEN_LIFE),
                    })),
                    None => return Err(Outcome::SignedOut),
                },
                _ => Some(token("remote-token".to_string())),
            };
            Ok(Remote { url, with })
        })
    }
}

/// Open a change, numbered by the world.
fn open_change(s: &mut State, ctx: &Ctx, new: &NewChange) -> Got<Posted<ChangeNo>> {
    let body = text_of(&new.body)?.to_string();
    let head_commit = s
        .branch_at(&new.head)
        .ok_or(Outcome::Refused(Refusal::Invalid))?;
    let base_commit = s
        .branch_at(&new.base)
        .ok_or(Outcome::Refused(Refusal::Invalid))?;
    if s.changes
        .iter()
        .any(|c| c.head == new.head && c.state == ChangeState::Open)
    {
        return Err(Outcome::Refused(Refusal::Exists));
    }
    let no = s.next_change_no();
    let author = Some(ctx.me(s));
    let seq = s.bump();
    s.changes.push(ChangeRec {
        no,
        title: new.title.clone(),
        body,
        markers: Vec::new(),
        state: ChangeState::Open,
        head: new.head.clone(),
        head_commit,
        base: new.base.clone(),
        base_commit,
        author,
        draft: new.draft,
        merge_state: Mergeable::Yes,
        marks: Vec::new(),
        closes: new.closes.clone(),
        files: Vec::new(),
        reviews: Vec::new(),
        comments: Vec::new(),
        events: Vec::new(),
        created: ctx.now,
        edited: None,
        editor: None,
        seq,
    });
    for b in &new.closes {
        if let Some(at) = s.blueprint_at(*b) {
            s.items[at].mentioned_by.push(no);
        }
    }
    Ok(Posted {
        id: no,
        link: s.change_link(no),
    })
}

fn change_event(s: &mut State, ctx: &Ctx, at: usize, what: ChangeHappened) {
    let by = Some(ctx.me(s));
    let seq = s.bump();
    let c = &mut s.changes[at];
    c.events.push(ChangeEvent {
        what,
        by,
        at: ctx.now,
        committed: None,
    });
    c.seq = seq;
}

/// The commits on `branch` past `base`'s history, newest first, or None
/// when the branch is not there.
fn past_base(s: &State, tip: &CommitId, base: &CommitId) -> Vec<CommitId> {
    let of_base = s.ancestors(base);
    s.ancestors(tip)
        .into_iter()
        .filter(|c| !of_base.contains(c))
        .collect()
}

/// May `p` move its branch, as its takeover says?
fn may_move(s: &State, ctx: &Ctx, p: &Proposal, tip: &CommitId) -> Got<()> {
    let exists = Err(Outcome::Refused(Refusal::Exists));
    if s.changes
        .iter()
        .any(|c| c.head == p.branch && c.state == ChangeState::Open)
    {
        return exists;
    }
    let past = past_base(s, tip, &p.base);
    match &p.takeover {
        Takeover::Never => exists,
        Takeover::OwnWithoutChange => {
            let me = ctx.me.as_deref().unwrap_or(super::world::FACTORY);
            if past.iter().all(|c| s.commit(c).is_some_and(|c| c.by == me)) {
                Ok(())
            } else {
                exists
            }
        }
        Takeover::SameShape { message } => match past.as_slice() {
            [one] => match s.commit(one) {
                Some(c) if c.parents.len() == 1 && same_message(&c.message, message) => Ok(()),
                _ => exists,
            },
            _ => exists,
        },
        Takeover::Unknown => Err(Outcome::Refused(Refusal::Invalid)),
    }
}

fn propose(
    s: &mut State,
    ctx: &Ctx,
    p: &Proposal,
    files: &dyn Blobs,
) -> Result<Proposed, Applied<ProposalPart>> {
    let stop = |part: ProposalPart, done: Vec<ProposalPart>, o: Outcome| Applied {
        done,
        stopped: Some((part, o)),
    };
    let before = |o: Outcome| stop(ProposalPart::Branch, Vec::new(), o);
    if p.takeover == Takeover::Unknown {
        return Err(before(Outcome::Refused(Refusal::Invalid)));
    }
    text_of(&p.change.body).map_err(before)?;
    let mut bytes = Vec::new();
    for (path, sum) in &p.files {
        bytes.push((path.clone(), blob(files, sum).map_err(before)?));
    }
    if s.commit(&p.base).is_none() {
        return Err(before(Outcome::Refused(Refusal::Invalid)));
    }
    if let Some(rec) = s.proposals.iter().rev().find(|r| r.proposal == *p) {
        let open_or_merged = s
            .change_at(rec.change)
            .is_some_and(|at| !matches!(s.changes[at].state, ChangeState::Closed { .. }));
        if open_or_merged {
            return Ok(Proposed {
                change: Posted {
                    id: rec.change,
                    link: s.change_link(rec.change),
                },
                head: rec.head.clone(),
            });
        }
    } else if let Some(tip) = s.branch_at(&p.branch) {
        may_move(s, ctx, p, &tip).map_err(before)?;
    }
    let me = ctx
        .me
        .clone()
        .unwrap_or_else(|| super::world::FACTORY.into());
    let head = s.new_commit(vec![p.base.clone()], &p.message, &me, bytes);
    s.set_branch(&p.branch, head.clone());
    let done = vec![ProposalPart::Branch, ProposalPart::Commit];
    let posted = open_change(s, ctx, &p.change).map_err(|o| stop(ProposalPart::Change, done, o))?;
    s.proposals.push(ProposalRec {
        proposal: p.clone(),
        change: posted.id,
        head: head.clone(),
    });
    ctx.wrote(s, Write::Propose(p.clone()));
    Ok(Proposed {
        change: posted,
        head,
    })
}

impl CodeHostWrites for Handle {
    fn open_change(&mut self, new: &NewChange) -> Got<Posted<ChangeNo>> {
        self.write(Op::CodeHost(CodeHostOp::OpenChange), |s, ctx| {
            let posted = open_change(s, ctx, new)?;
            ctx.wrote(s, Write::OpenChange(new.clone()));
            Ok(posted)
        })
    }

    fn edit_change(
        &mut self,
        no: ChangeNo,
        title: Option<&str>,
        body: Option<&str>,
        markers: &MarkerEdit,
    ) -> Got<WebLink> {
        self.write(Op::CodeHost(CodeHostOp::EditChange), |s, ctx| {
            markers.check()?;
            let at = at_change(s, no)?;
            let me = ctx.me(s);
            let seq = s.bump();
            let c = &mut s.changes[at];
            if let Some(t) = title {
                c.title = t.to_string();
            }
            if let Some(b) = body {
                c.body = b.to_string();
            }
            edit_markers(&mut c.markers, markers);
            c.edited = Some(ctx.now);
            c.editor = Some(me);
            c.seq = seq;
            ctx.wrote(
                s,
                Write::EditChange {
                    no,
                    title: title.map(str::to_string),
                    body: body.map(|b| crate::Body::Text(b.to_string())),
                    markers: markers.clone(),
                },
            );
            Ok(s.change_link(no))
        })
    }

    /// A mark on a change that is not open is `Refused(Invalid)`.
    fn mark_change(&mut self, no: ChangeNo, mark: &Mark) -> Did {
        self.write(Op::CodeHost(CodeHostOp::MarkChange), |s, ctx| {
            known_mark(mark)?;
            let at = at_change(s, no)?;
            if s.changes[at].state != ChangeState::Open {
                return Err(Outcome::Refused(Refusal::Invalid));
            }
            if !s.changes[at].marks.contains(mark) {
                s.changes[at].marks.push(mark.clone());
                change_event(s, ctx, at, ChangeHappened::Marked(mark.clone()));
            }
            ctx.wrote(
                s,
                Write::MarkChange {
                    no,
                    mark: mark.clone(),
                },
            );
            Ok(())
        })
    }

    fn unmark_change(&mut self, no: ChangeNo, mark: &Mark) -> Did {
        self.write(Op::CodeHost(CodeHostOp::UnmarkChange), |s, ctx| {
            known_mark(mark)?;
            let at = at_change(s, no)?;
            if let Some(i) = s.changes[at].marks.iter().position(|m| m == mark) {
                s.changes[at].marks.remove(i);
                change_event(s, ctx, at, ChangeHappened::Unmarked(mark.clone()));
            }
            ctx.wrote(
                s,
                Write::UnmarkChange {
                    no,
                    mark: mark.clone(),
                },
            );
            Ok(())
        })
    }

    fn comment_change(
        &mut self,
        no: ChangeNo,
        body: &str,
        markers: &[Marker],
    ) -> Got<Posted<CommentId>> {
        self.write(Op::CodeHost(CodeHostOp::CommentChange), |s, ctx| {
            writable(markers)?;
            let at = at_change(s, no)?;
            let author = Some(ctx.me(s));
            let id = s.new_comment(author, body, markers.to_vec(), ctx.now);
            s.changes[at].comments.push(id.clone());
            change_event(s, ctx, at, ChangeHappened::Commented);
            ctx.wrote(
                s,
                Write::CommentChange {
                    no,
                    body: crate::Body::Text(body.to_string()),
                    markers: markers.to_vec(),
                },
            );
            let link = s.comment_link(&id);
            Ok(Posted { id, link })
        })
    }

    /// Closed already is done; merged is `Refused(Invalid)`.
    fn close_change(&mut self, no: ChangeNo) -> Did {
        self.write(Op::CodeHost(CodeHostOp::CloseChange), |s, ctx| {
            let at = at_change(s, no)?;
            match s.changes[at].state {
                ChangeState::Open => {
                    s.changes[at].state = ChangeState::Closed { at: ctx.now };
                    change_event(s, ctx, at, ChangeHappened::Closed);
                }
                ChangeState::Closed { .. } => {}
                ChangeState::Merged { .. } => return Err(Outcome::Refused(Refusal::Invalid)),
            }
            ctx.wrote(s, Write::CloseChange { no });
            Ok(())
        })
    }

    /// At a head that moved, `Conflict(HeadMoved)`. A change that cannot
    /// merge, or is not open, is `Refused(NotMergeable)`. Merged, its base
    /// moves to a new commit and the blueprints it closes close.
    fn merge(&mut self, no: ChangeNo, head: &CommitId, how: MergeHow) -> Did {
        self.write(Op::CodeHost(CodeHostOp::Merge), |s, ctx| {
            let at = at_change(s, no)?;
            let c = s.changes[at].clone();
            // A cycle's merge grant covers a change its account opened, the
            // factory's; which cycle opened it the site cannot tell.
            if ctx.scope == Some(crate::Scope::CycleMerge)
                && c.author.as_ref().map(|a| &a.login) != Some(&ctx.me(s).login)
            {
                return Err(Outcome::Forbidden);
            }
            if c.state != ChangeState::Open {
                return Err(Outcome::Refused(Refusal::NotMergeable));
            }
            if c.head_commit != *head {
                return Err(Outcome::Conflict(Lost::HeadMoved));
            }
            if let Mergeable::No(_) = c.merge_state {
                return Err(Outcome::Refused(Refusal::NotMergeable));
            }
            if c.draft {
                return Err(Outcome::Refused(Refusal::NotMergeable));
            }
            let base_tip = s
                .branch_at(&c.base)
                .ok_or(Outcome::Refused(Refusal::NotMergeable))?;
            let me = ctx
                .me
                .clone()
                .unwrap_or_else(|| super::world::FACTORY.into());
            let parents = match how {
                MergeHow::Merge => vec![base_tip, head.clone()],
                MergeHow::Squash | MergeHow::Rebase => vec![base_tip],
            };
            let files = s.commit(head).map(|k| k.files.clone()).unwrap_or_default();
            let commit = s.new_commit(parents, &c.title, &me, files);
            s.set_branch(&c.base, commit.clone());
            let by = Some(ctx.me(s));
            s.changes[at].state = ChangeState::Merged {
                at: ctx.now,
                by,
                commit,
            };
            change_event(s, ctx, at, ChangeHappened::Merged);
            for b in &c.closes {
                if let Some(i) = s.blueprint_at(*b) {
                    if s.items[i].open {
                        close_item(s, ctx, i, &CloseReason::Done);
                        s.items[i].closer = Some(Closer::Change { no, merged: true });
                    }
                }
            }
            ctx.wrote(
                s,
                Write::Merge {
                    no,
                    head: head.clone(),
                    how,
                },
            );
            Ok(())
        })
    }

    fn propose(
        &mut self,
        p: &Proposal,
        files: &dyn Blobs,
    ) -> Result<Proposed, Applied<ProposalPart>> {
        let answer = self.write(Op::CodeHost(CodeHostOp::Propose), |s, ctx| {
            Ok(propose(s, ctx, p, files))
        });
        match answer {
            Ok(r) => r,
            Err(o) => Err(Applied {
                done: Vec::new(),
                stopped: Some((ProposalPart::Branch, o)),
            }),
        }
    }

    fn tag(&mut self, name: &TagName, at: &CommitId) -> Did {
        self.write(Op::CodeHost(CodeHostOp::Tag), |s, ctx| {
            s.commit(at).ok_or(Outcome::NotFound)?;
            match s.tags.iter().find(|t| t.name == *name) {
                Some(t) if t.commit == *at => {}
                Some(_) => return Err(Outcome::Conflict(Lost::Elsewhere)),
                None => s.tags.push(Tag {
                    name: name.clone(),
                    commit: at.clone(),
                    at: Some(ctx.now),
                    annotated: false,
                }),
            }
            ctx.wrote(
                s,
                Write::Tag {
                    name: name.clone(),
                    at: at.clone(),
                },
            );
            Ok(())
        })
    }

    fn set_status(&mut self, at: &CommitId, status: &Status) -> Did {
        self.write(Op::CodeHost(CodeHostOp::SetStatus), |s, ctx| {
            s.commit(at).ok_or(Outcome::NotFound)?;
            s.statuses
                .retain(|(c, x)| !(c == at && x.name == status.name));
            s.statuses.push((at.clone(), status.clone()));
            ctx.wrote(
                s,
                Write::SetStatus {
                    at: at.clone(),
                    status: status.clone(),
                },
            );
            Ok(())
        })
    }

    /// The tag must be there. The same tag twice is the same release.
    fn new_release(&mut self, new: &NewRelease) -> Got<Posted<ReleaseId>> {
        self.write(Op::CodeHost(CodeHostOp::NewRelease), |s, ctx| {
            text_of(&new.notes)?;
            if !s.tags.iter().any(|t| t.name == new.tag) {
                return Err(Outcome::NotFound);
            }
            let posted = match s.releases.iter().find(|r| r.release.tag == new.tag) {
                Some(r) => Posted {
                    id: r.release.id.clone(),
                    link: r.release.link.clone(),
                },
                None => {
                    let id = ReleaseId::new(s.fresh_id("release"));
                    let link = s.release_link(&new.tag);
                    s.releases.push(ReleaseRec {
                        release: Release {
                            id: id.clone(),
                            tag: new.tag.clone(),
                            title: new.title.clone(),
                            draft: new.draft,
                            prerelease: new.prerelease,
                            published: (!new.draft).then_some(ctx.now),
                            files: Vec::new(),
                            link: link.clone(),
                        },
                        bytes: Vec::new(),
                    });
                    Posted { id, link }
                }
            };
            ctx.wrote(s, Write::NewRelease(new.clone()));
            Ok(posted)
        })
    }

    fn upload(
        &mut self,
        release: &ReleaseId,
        name: &FileName,
        file: &Sum,
        files: &dyn Blobs,
    ) -> Did {
        self.write(Op::CodeHost(CodeHostOp::Upload), |s, ctx| {
            let at = s
                .releases
                .iter()
                .position(|r| r.release.id == *release)
                .ok_or(Outcome::NotFound)?;
            let bytes = blob(files, file)?;
            let r = &mut s.releases[at];
            match r.release.files.iter().find(|f| f.name == *name) {
                Some(f) if f.size == file.len => {}
                Some(_) => return Err(Outcome::Refused(Refusal::Exists)),
                None => {
                    r.release.files.push(ReleaseFile {
                        name: name.clone(),
                        size: file.len,
                    });
                    r.bytes.push(bytes);
                }
            }
            ctx.wrote(
                s,
                Write::Upload {
                    release: release.clone(),
                    name: name.clone(),
                    file: file.clone(),
                },
            );
            Ok(())
        })
    }

    fn publish_release(&mut self, release: &ReleaseId) -> Did {
        self.write(Op::CodeHost(CodeHostOp::PublishRelease), |s, ctx| {
            let r = s
                .releases
                .iter_mut()
                .find(|r| r.release.id == *release)
                .ok_or(Outcome::NotFound)?;
            if r.release.draft {
                r.release.draft = false;
                r.release.published = Some(ctx.now);
            }
            ctx.wrote(
                s,
                Write::PublishRelease {
                    release: release.clone(),
                },
            );
            Ok(())
        })
    }
}
