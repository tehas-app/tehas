//! The fake's blueprints port, over the world.

use std::num::NonZeroU32;

use super::faulty::{BlueprintsOp, Op};
use super::site::{
    blueprint, comment, comments_of, edit_markers, first, matches, no_blueprint, revision, text_of,
    whole, whole_kept, writable, Ctx, Handle,
};
use super::state::State;
use super::tickets::{close_item, page, since, token};
use crate::{
    Attachment, AttachmentId, Blueprint, BlueprintChange, BlueprintDelta, BlueprintHit,
    BlueprintNo, BlueprintWrites, Blueprints, Bound, ChangeToken, CloseReason, Comment, CommentId,
    Cursor, Depth, Did, Edit, Got, Lost, Marker, MarkerEdit, NewBlueprint, Outcome, Pool, Posted,
    Revision, Took, Unread, WebLink, Whole, Within, Write,
};

fn hits(s: &State, keep: impl Fn(&super::state::ItemRec) -> bool) -> Vec<BlueprintHit> {
    let mut all: Vec<(crate::At, BlueprintHit)> = s
        .items
        .iter()
        .filter(|i| keep(i))
        .filter_map(|i| {
            Some((
                i.created,
                BlueprintHit {
                    no: i.blueprint?,
                    title: i.title.clone(),
                    open: i.open,
                },
            ))
        })
        .collect();
    all.sort_by_key(|(created, h)| (*created, h.no));
    all.into_iter().map(|(_, h)| h).collect()
}

fn comment_at(s: &State, id: &CommentId) -> Got<usize> {
    let at = s.comment_at(id).ok_or(Outcome::NotFound)?;
    if s.comments[at].gone {
        return Err(Outcome::NotFound);
    }
    Ok(at)
}

impl Blueprints for Handle {
    fn blueprint(&mut self, no: BlueprintNo, depth: Depth) -> Got<Blueprint> {
        self.run(Pool::Calls, |s, _| {
            let at = no_blueprint(s, no)?;
            blueprint(s, &s.items[at], depth)
        })
    }

    fn several(&mut self, nos: &[BlueprintNo], depth: Depth) -> Vec<(BlueprintNo, Got<Blueprint>)> {
        self.each(nos, |s, _, no| {
            let at = no_blueprint(s, *no)?;
            blueprint(s, &s.items[at], depth)
        })
    }

    fn comments(&mut self, no: BlueprintNo, bound: Bound) -> Got<Whole<Comment>> {
        self.run(Pool::Calls, |s, _| {
            let at = no_blueprint(s, no)?;
            whole(comments_of(s, &s.items[at].comments, false), bound)
        })
    }

    fn comments_by_id(&mut self, ids: &[CommentId]) -> Vec<(CommentId, Got<Comment>)> {
        self.each(ids, |s, _, id| {
            let at = comment_at(s, id)?;
            Ok(comment(&s.comments[at], false))
        })
    }

    fn body_edits(&mut self, no: BlueprintNo, bound: Bound) -> Got<Whole<Edit>> {
        self.run(Pool::Calls, |s, _| {
            let at = no_blueprint(s, no)?;
            whole(s.items[at].body_edits.clone(), bound)
        })
    }

    fn comment_edits(&mut self, id: &CommentId, bound: Bound) -> Got<Whole<Edit>> {
        self.run(Pool::Calls, |s, _| {
            let at = comment_at(s, id)?;
            whole(s.comments[at].edits.clone(), bound)
        })
    }

    /// The fake's order of relevance is the order blueprints were opened.
    fn search(
        &mut self,
        words: &str,
        within: Within,
        open: Option<bool>,
        n: NonZeroU32,
    ) -> Got<Took<BlueprintHit>> {
        self.run(Pool::Search, |s, _| {
            let found = hits(s, |i| {
                open.is_none_or(|o| i.open == o)
                    && match within {
                        Within::Titles => matches(words, &i.title),
                        Within::Everything => matches(words, &format!("{}\n{}", i.title, i.text)),
                    }
            });
            Ok(first(found, n.get()))
        })
    }

    fn attachments(&mut self, no: BlueprintNo) -> Got<Whole<Attachment>> {
        self.run(Pool::Calls, |s, _| {
            let at = no_blueprint(s, no)?;
            whole_kept(
                s.items[at]
                    .attachments
                    .iter()
                    .map(|(a, _)| a.clone())
                    .collect(),
            )
        })
    }

    fn attachments_in(&self, text: &str) -> Vec<Attachment> {
        self.world.lock().attachments_in(text)
    }

    /// Past `limit` is `Unread(TooLarge)`, and nothing is written. A sink
    /// that will not take the bytes is `Unread(Malformed)`: what it holds
    /// is not whole.
    fn fetch(&mut self, a: &AttachmentId, limit: u64, into: &mut dyn std::io::Write) -> Did {
        let bytes = self.run(Pool::Calls, |s, _| {
            s.items
                .iter()
                .flat_map(|i| i.attachments.iter())
                .find(|(x, _)| x.id == *a)
                .map(|(_, b)| b.clone())
                .ok_or(Outcome::NotFound)
        })?;
        if bytes.len() as u64 > limit {
            return Err(Outcome::Unread(Unread::TooLarge));
        }
        into.write_all(&bytes)
            .map_err(|_| Outcome::Unread(Unread::Malformed))
    }

    fn changed(&mut self, since_token: Option<&ChangeToken>) -> Got<(bool, ChangeToken)> {
        self.run(Pool::Calls, |s, _| {
            let now = token(s);
            Ok((since_token != Some(&now), now))
        })
    }

    fn delta(&mut self, from: Option<&Cursor>, bound: Bound) -> Got<BlueprintDelta> {
        self.run(Pool::Calls, |s, _| {
            let after = since(from);
            let mut blueprints = Vec::new();
            let mut comments = Vec::new();
            for item in &s.items {
                let Some(no) = item.blueprint else { continue };
                if item.seq > after {
                    let mut change = BlueprintChange::new(no, revision(item.revision));
                    change.title = item.title.clone();
                    change.body = item.text.clone();
                    change.open = item.open;
                    change.closed = item.closed;
                    blueprints.push((item.seq, change));
                }
                for id in &item.comments {
                    if let Some(at) = s.comment_at(id) {
                        let c = &s.comments[at];
                        if c.seq > after && !c.gone {
                            comments.push((c.seq, (no, comment(c, false))));
                        }
                    }
                }
            }
            let (blueprints, comments, next, more) = page(s, blueprints, comments, bound);
            let mut delta = BlueprintDelta::new(next);
            delta.blueprints = blueprints;
            delta.comments = comments;
            delta.more = more;
            Ok(delta)
        })
    }
}

/// Edit an item's title and body if still at `since`, as both ports'
/// edits do.
pub(crate) fn edit_item(
    s: &mut State,
    ctx: &Ctx,
    at: usize,
    title: Option<&str>,
    body: Option<&str>,
    markers: &MarkerEdit,
    since_rev: &Revision,
) -> Did {
    markers.check()?;
    if revision(s.items[at].revision) != *since_rev {
        return Err(Outcome::Conflict(Lost::Changed));
    }
    let me = ctx.me(s);
    let item = &mut s.items[at];
    if let Some(t) = title {
        item.title = t.to_string();
    }
    if let Some(b) = body {
        item.text = b.to_string();
    }
    edit_markers(&mut item.markers, markers);
    item.editor = Some(me.clone());
    item.edited = Some(ctx.now);
    item.body_edits.push(Edit {
        by: Some(me),
        at: ctx.now,
        after: item.text.clone(),
        markers: item.markers.clone(),
    });
    s.touch(at, ctx.now);
    Ok(())
}

impl BlueprintWrites for Handle {
    fn open(&mut self, new: &NewBlueprint) -> Got<Posted<BlueprintNo>> {
        self.write(Op::Blueprints(BlueprintsOp::Open), |s, ctx| {
            ctx.text_alone(new.markers.is_empty())?;
            let body = text_of(&new.body)?.to_string();
            writable(&new.markers)?;
            let (no, ticket) = s.next_blueprint_no();
            let author = Some(ctx.me(s));
            let at = s.new_item(ticket, Some(no), author, ctx.now);
            let item = &mut s.items[at];
            item.title = new.title.clone();
            item.text = body;
            item.markers = new.markers.clone();
            ctx.wrote(s, Write::OpenBlueprint(new.clone()));
            Ok(Posted {
                id: no,
                link: s.blueprint_link(no),
            })
        })
    }

    fn edit(
        &mut self,
        no: BlueprintNo,
        title: Option<&str>,
        body: Option<&str>,
        markers: &MarkerEdit,
        since_rev: &Revision,
    ) -> Got<WebLink> {
        self.write(Op::Blueprints(BlueprintsOp::Edit), |s, ctx| {
            ctx.text_alone(markers.set.is_empty() && markers.drop.is_empty())?;
            let at = no_blueprint(s, no)?;
            ctx.own(s, s.items[at].author.as_ref())?;
            edit_item(s, ctx, at, title, body, markers, since_rev)?;
            ctx.wrote(
                s,
                Write::EditBlueprint {
                    no,
                    title: title.map(str::to_string),
                    body: body.map(|b| crate::Body::Text(b.to_string())),
                    markers: markers.clone(),
                    since: since_rev.clone(),
                },
            );
            Ok(s.blueprint_link(no))
        })
    }

    fn comment(
        &mut self,
        no: BlueprintNo,
        body: &str,
        markers: &[Marker],
    ) -> Got<Posted<CommentId>> {
        self.write(Op::Blueprints(BlueprintsOp::Comment), |s, ctx| {
            ctx.text_alone(markers.is_empty())?;
            writable(markers)?;
            let at = no_blueprint(s, no)?;
            let author = Some(ctx.me(s));
            let id = s.new_comment(author, body, markers.to_vec(), ctx.now);
            s.items[at].comments.push(id.clone());
            s.touch(at, ctx.now);
            ctx.wrote(
                s,
                Write::Comment {
                    on: no,
                    body: crate::Body::Text(body.to_string()),
                    markers: markers.to_vec(),
                },
            );
            let link = s.comment_link(&id);
            Ok(Posted { id, link })
        })
    }

    /// A comment deleted since it was read is `Conflict(Gone)`; one that
    /// changed, `Conflict(Changed)`.
    fn edit_comment(
        &mut self,
        id: &CommentId,
        body: &str,
        markers: &MarkerEdit,
        since_rev: &Revision,
    ) -> Got<WebLink> {
        self.write(Op::Blueprints(BlueprintsOp::EditComment), |s, ctx| {
            markers.check()?;
            let at = s.comment_at(id).ok_or(Outcome::NotFound)?;
            if s.comments[at].gone {
                return Err(Outcome::Conflict(Lost::Gone));
            }
            ctx.own(s, s.comments[at].author.as_ref())?;
            if revision(s.comments[at].revision) != *since_rev {
                return Err(Outcome::Conflict(Lost::Changed));
            }
            let me = ctx.me(s);
            let seq = s.bump();
            let c = &mut s.comments[at];
            c.body = body.to_string();
            edit_markers(&mut c.markers, markers);
            c.editor = Some(me.clone());
            c.edited = Some(ctx.now);
            c.revision += 1;
            c.seq = seq;
            c.edits.push(Edit {
                by: Some(me),
                at: ctx.now,
                after: body.to_string(),
                markers: c.markers.clone(),
            });
            let link = c.link.clone();
            ctx.wrote(
                s,
                Write::EditComment {
                    id: id.clone(),
                    body: crate::Body::Text(body.to_string()),
                    markers: markers.clone(),
                    since: since_rev.clone(),
                },
            );
            Ok(link)
        })
    }

    fn close(&mut self, no: BlueprintNo, done: bool) -> Did {
        self.write(Op::Blueprints(BlueprintsOp::Close), |s, ctx| {
            let at = no_blueprint(s, no)?;
            let reason = if done {
                CloseReason::Done
            } else {
                CloseReason::NotPlanned
            };
            close_item(s, ctx, at, &reason);
            ctx.wrote(s, Write::CloseBlueprint { no, done });
            Ok(())
        })
    }
}
