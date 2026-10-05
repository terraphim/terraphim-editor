//! "Make the cuts" (R-8.5): delete the faded spans, then a mechanical
//! punctuation and capitalisation tidy-up at the joins.
//!
//! The deletions are exactly the cuts (merged where they touch or overlap).
//! The tidy-up is the product decision of 2026-10-05: a fix-up of the same
//! kind as the a/an rule, which may edit a character or two **next to** a cut
//! and nothing else. Every tidy edit lies within two characters of a cut
//! boundary, except [`EditKind::EmptyBlock`] (below). The rules, applied at
//! each join in this order:
//!
//! 1. **New sentence start**: when a cut removed the start of a sentence, a
//!    comma, semicolon or colon left at the front goes, and a lower-case
//!    first word is capitalised ("~~Perhaps~~ these" becomes "These").
//! 2. **Doubled punctuation**: `, ,` loses its second comma; a comma,
//!    semicolon or colon left before a terminator goes (`, .` becomes `.`).
//! 3. **Comma after a conjunction**: "but, natural" becomes "but natural"
//!    when the cut brought the conjunction and the comma together.
//! 4. **Space before punctuation**: "is ." becomes "is.".
//! 5. **Doubled space**: a space on both sides of a join becomes one.
//!
//! **Emptied paragraphs** ([`EditKind::EmptyBlock`]): when the cuts remove
//! every word of a paragraph (all its sentences), its block goes too: the
//! line's indentation and list marker and the blank lines after it, so no
//! empty bullet is left. Those edits hold only whitespace and list markers
//! and touch a cut, but can be longer than two characters.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::lists::{Connective, find_connectives};
use crate::offset::{bytes_to_utf16, utf16_to_bytes};
use crate::text::{Doc, full_stop_ends_sentence};
use crate::trim::Cut;

/// What an [`Edit`] does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditKind {
    /// Deletes a faded span (cuts merged where they touch).
    Cut,
    /// Removes a stray comma, semicolon or colon at a join.
    Punctuation,
    /// Capitalises the first letter of a new sentence start.
    Capitalisation,
    /// Removes a doubled space, or a space left before punctuation.
    Whitespace,
    /// Removes the indentation, list marker and blank lines of a paragraph
    /// the cuts emptied.
    EmptyBlock,
}

/// One edit on the original body: replace UTF-16 range `start..end` with
/// `insert` (empty for a deletion).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edit {
    /// UTF-16 code-unit offset of the first unit replaced.
    pub start: usize,
    /// UTF-16 code-unit offset one past the last unit replaced.
    pub end: usize,
    /// Replacement text.
    pub insert: String,
    /// Which rule produced it.
    pub kind: EditKind,
}

/// The result of "Make the cuts".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MadeCuts {
    /// The body after every edit.
    pub text: String,
    /// The edits, sorted by `start` and non-overlapping, all on the
    /// **original** body's UTF-16 offsets. Applying them from last to first
    /// gives [`MadeCuts::text`]; the editor applies them through its own edit
    /// path as one undo step so spans and ghosts re-anchor.
    pub edits: Vec<Edit>,
}

/// Delete `cuts` from `body` and tidy the joins (see the module docs).
///
/// `cuts` are usually [`crate::TrimPlan::active`] for the level shown. They
/// may come in any order and may overlap or nest. A cut whose offsets lie
/// outside `body`, split a surrogate pair, or are reversed is ignored.
pub fn make_cuts(body: &str, cuts: &[Cut]) -> MadeCuts {
    make_cuts_from_ranges(body, cuts.iter().map(|c| (c.start, c.end)))
}

/// [`make_cuts`] on bare UTF-16 `(start, end)` ranges.
pub fn make_cuts_from_ranges<I>(body: &str, ranges: I) -> MadeCuts
where
    I: IntoIterator<Item = (usize, usize)>,
{
    let ranges: Vec<(usize, usize)> = ranges.into_iter().collect();
    let flat: Vec<usize> = ranges.iter().flat_map(|&(s, e)| [s, e]).collect();
    let bytes = utf16_to_bytes(body, &flat);
    let mut cuts: Vec<(usize, usize)> = bytes
        .chunks_exact(2)
        .filter_map(|pair| match (pair[0], pair[1]) {
            (Some(s), Some(e)) if e > s => Some((s, e)),
            _ => None,
        })
        .collect();
    cuts.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(cuts.len());
    for (s, e) in cuts {
        if let Some(last) = merged.last_mut()
            && s <= last.1
        {
            last.1 = last.1.max(e);
            continue;
        }
        merged.push((s, e));
    }
    let mut tidy = Tidy::new(body, merged);
    tidy.empty_blocks();
    for j in 0..tidy.joins.len() {
        tidy.at_join(j);
    }
    tidy.finish()
}

/// A kept character of the body.
#[derive(Debug, Clone, Copy)]
struct OutChar {
    pos: usize,
    ch: char,
    deleted: Option<EditKind>,
}

/// One join: the cut `cs..ce` sits just before `out[k]`.
#[derive(Debug, Clone, Copy)]
struct Join {
    k: usize,
    cs: usize,
    ce: usize,
}

struct Tidy<'a> {
    text: &'a str,
    doc: Doc<'a>,
    cuts: Vec<(usize, usize)>,
    out: Vec<OutChar>,
    joins: Vec<Join>,
    /// Replacement text by `out` index (capitalisation).
    repl: BTreeMap<usize, String>,
}

fn is_ws(c: char) -> bool {
    c.is_whitespace()
}

impl<'a> Tidy<'a> {
    fn new(text: &'a str, cuts: Vec<(usize, usize)>) -> Tidy<'a> {
        let mut out = Vec::with_capacity(text.len());
        let mut joins = Vec::with_capacity(cuts.len());
        let mut next_cut = 0usize;
        for (pos, ch) in text.char_indices() {
            while next_cut < cuts.len() && cuts[next_cut].1 <= pos {
                let (cs, ce) = cuts[next_cut];
                joins.push(Join {
                    k: out.len(),
                    cs,
                    ce,
                });
                next_cut += 1;
            }
            if next_cut < cuts.len() && cuts[next_cut].0 <= pos {
                continue;
            }
            out.push(OutChar {
                pos,
                ch,
                deleted: None,
            });
        }
        for &(cs, ce) in &cuts[next_cut..] {
            joins.push(Join {
                k: out.len(),
                cs,
                ce,
            });
        }
        Tidy {
            text,
            doc: Doc::parse(text),
            cuts,
            out,
            joins,
            repl: BTreeMap::new(),
        }
    }

    fn live(&self, i: usize) -> bool {
        self.out[i].deleted.is_none()
    }

    fn ch(&self, i: usize) -> char {
        self.out[i].ch
    }

    fn delete(&mut self, i: usize, kind: EditKind) {
        self.out[i].deleted = Some(kind);
    }

    /// True when `out[i]` is within two characters of a cut boundary: at
    /// most two characters lie between it and the nearest cut.
    fn near(&self, i: usize) -> bool {
        let pos = self.out[i].pos;
        let end = pos + self.out[i].ch.len_utf8();
        let close = |gap: &str| gap.len() <= 8 && gap.chars().count() <= 2;
        let after = self.cuts.partition_point(|c| c.0 < end);
        if self
            .cuts
            .get(after)
            .is_some_and(|c| close(&self.text[end..c.0]))
        {
            return true;
        }
        let before = self.cuts.partition_point(|c| c.1 <= pos);
        before > 0 && close(&self.text[self.cuts[before - 1].1..pos])
    }

    /// The last live non-whitespace index before `k`, and the live
    /// whitespace indices between it and `k`.
    fn left(&self, k: usize) -> (Option<usize>, Vec<usize>) {
        let mut ws = Vec::new();
        for i in (0..k).rev() {
            if !self.live(i) {
                continue;
            }
            if is_ws(self.ch(i)) {
                ws.push(i);
            } else {
                return (Some(i), ws);
            }
        }
        (None, ws)
    }

    /// The live whitespace indices from `k`, and the first live
    /// non-whitespace index after them.
    fn right(&self, k: usize) -> (Vec<usize>, Option<usize>) {
        let mut ws = Vec::new();
        for i in k..self.out.len() {
            if !self.live(i) {
                continue;
            }
            if is_ws(self.ch(i)) {
                ws.push(i);
            } else {
                return (ws, Some(i));
            }
        }
        (ws, None)
    }

    fn prev_live(&self, i: usize) -> Option<usize> {
        (0..i).rev().find(|&j| self.live(j))
    }

    /// The live alphabetic run ending at `i` (inclusive), as a string.
    fn word_ending_at(&self, i: usize) -> String {
        let mut chars = Vec::new();
        let mut j = Some(i);
        while let Some(x) = j {
            if !self.ch(x).is_alphabetic() {
                break;
            }
            chars.push(self.ch(x));
            j = self.prev_live(x);
        }
        chars.iter().rev().collect()
    }

    /// The live word starting at `i`: alphanumerics joined by `-`, `'`, `’`,
    /// `.`, `/` or `_` (the crate's word characters).
    fn word_starting_at(&self, i: usize) -> String {
        let mut w = String::new();
        for x in i..self.out.len() {
            if !self.live(x) {
                continue;
            }
            let c = self.ch(x);
            if c.is_alphanumeric() || matches!(c, '-' | '\'' | '\u{2019}' | '.' | '/' | '_') {
                w.push(c);
            } else {
                break;
            }
        }
        w
    }

    /// A coordinating conjunction ("and", "but", ...), matched with the
    /// connective automaton.
    fn is_conjunction(word: &str) -> bool {
        !word.is_empty()
            && find_connectives(word).iter().any(|&(s, e, k)| {
                s == 0
                    && e == word.len()
                    && matches!(k, Connective::Conjunction | Connective::Contrast)
            })
    }

    /// True when the live text before index `li` (inclusive) ends a
    /// sentence: a terminator, possibly followed by closing quotes,
    /// brackets or emphasis. A full stop counts only when the sentence
    /// splitter's rule says it can end a sentence (not "e.g.", "Dr.", an
    /// ellipsis or a decimal point).
    fn ends_sentence(&self, li: usize) -> bool {
        let mut j = Some(li);
        while let Some(x) = j {
            match self.ch(x) {
                '.' => return full_stop_ends_sentence(self.text, self.out[x].pos),
                '!' | '?' => return true,
                '"' | '\u{201D}' | '\u{2019}' | ')' | ']' | '*' | '_' | '`' => {
                    j = self.prev_live(x);
                }
                _ => return false,
            }
        }
        false
    }

    /// Remove the block of every paragraph a single cut emptied.
    fn empty_blocks(&mut self) {
        for (para, p) in self.doc.paragraphs.clone().into_iter().enumerate() {
            let i = self.cuts.partition_point(|c| c.1 < p.end);
            let Some(&(cs, ce)) = self.cuts.get(i) else {
                continue;
            };
            if !(cs <= p.start && p.end <= ce) {
                continue;
            }
            let Some((a, b)) = self.doc.block_extent(para) else {
                continue;
            };
            let lo = self.out.partition_point(|o| o.pos < a);
            let hi = self.out.partition_point(|o| o.pos < b);
            for x in lo..hi {
                let pos = self.out[x].pos;
                if (pos < cs || pos >= ce) && self.live(x) {
                    self.delete(x, EditKind::EmptyBlock);
                }
            }
        }
    }

    fn at_join(&mut self, j: usize) {
        let Join { k, cs, ce } = self.joins[j];

        // 0. A cut from a paragraph's start leaves no space at the front.
        let at_line_start = match self.prev_live(k) {
            None => true,
            Some(p) => self.ch(p) == '\n',
        };
        if at_line_start
            && self
                .doc
                .paragraphs
                .binary_search_by_key(&cs, |p| p.start)
                .is_ok()
        {
            let (rws, _) = self.right(k);
            if !rws.is_empty()
                && rws
                    .iter()
                    .all(|&x| !matches!(self.ch(x), '\n' | '\r') && self.near(x))
            {
                for x in rws {
                    self.delete(x, EditKind::Whitespace);
                }
            }
        }

        // 1. New sentence start.
        let sentences = &self.doc.sentences;
        let first = sentences.partition_point(|s| s.start < cs);
        let new_sentence = sentences[first..]
            .iter()
            .take_while(|s| s.start < ce)
            .any(|s| ce < s.end);
        if new_sentence {
            let (li, _) = self.left(k);
            let para_start = sentences[first..]
                .iter()
                .take_while(|s| s.start < ce)
                .last()
                .map(|s| self.doc.paragraphs[s.para].start)
                .unwrap_or(ce);
            let starts = match li {
                None => true,
                Some(li) => self.out[li].pos < para_start || self.ends_sentence(li),
            };
            if starts {
                let (_, ri) = self.right(k);
                if let Some(ri) = ri
                    && matches!(self.ch(ri), ',' | ';' | ':')
                    && self.near(ri)
                {
                    self.delete(ri, EditKind::Punctuation);
                    // The space after it goes only when the left side
                    // already supplies a separator (or there is no left
                    // side): "end., of" must become "end. Of", never
                    // "end.Of", which would join two words.
                    let (li, lws) = self.left(k);
                    let separated = li.is_none() || !lws.is_empty();
                    let (ws, _) = self.right(ri + 1);
                    for x in ws.into_iter().filter(|_| separated) {
                        if self.ch(x) == '\n' || !self.near(x) {
                            break;
                        }
                        self.delete(x, EditKind::Whitespace);
                    }
                }
                let (_, ri) = self.right(k);
                if let Some(ri) = ri {
                    let c = self.ch(ri);
                    let word = self.word_starting_at(ri);
                    let plain_lower = word.chars().all(|w| {
                        (w.is_alphabetic() && w.is_lowercase())
                            || matches!(w, '-' | '\'' | '\u{2019}')
                    });
                    if c.is_lowercase() && plain_lower && self.near(ri) {
                        self.repl.insert(ri, c.to_uppercase().collect());
                    }
                }
            }
        }

        // 2. Doubled punctuation.
        let (li, _) = self.left(k);
        let (_, ri) = self.right(k);
        if let (Some(li), Some(ri)) = (li, ri) {
            let (lc, rc) = (self.ch(li), self.ch(ri));
            if lc == ',' && rc == ',' && self.near(ri) {
                self.delete(ri, EditKind::Punctuation);
            } else if matches!(lc, ',' | ';' | ':')
                && matches!(rc, '.' | '!' | '?' | ';' | ':')
                && self.near(li)
            {
                self.delete(li, EditKind::Punctuation);
            }
        }

        // 3. Comma after a conjunction.
        let (li, _) = self.left(k);
        let (_, ri) = self.right(k);
        if let (Some(li), Some(ri)) = (li, ri) {
            if self.ch(ri) == ',' && self.near(ri) {
                let after = self.right(ri + 1).1;
                if Self::is_conjunction(&self.word_ending_at(li))
                    && after.is_some_and(|a| self.ch(a).is_alphabetic())
                {
                    self.delete(ri, EditKind::Punctuation);
                }
            } else if self.ch(li) == ','
                && self.near(li)
                && self.ch(ri).is_alphabetic()
                && let Some(before) = self.prev_live(li)
                && Self::is_conjunction(&self.word_ending_at(before))
            {
                self.delete(li, EditKind::Punctuation);
            }
        }

        // 4. Space before punctuation.
        let (li, lws) = self.left(k);
        let (rws, ri) = self.right(k);
        if let (Some(_), Some(ri)) = (li, ri)
            && matches!(self.ch(ri), ',' | '.' | ';' | ':' | '!' | '?')
            && (!lws.is_empty() || !rws.is_empty())
            && lws
                .iter()
                .chain(&rws)
                .all(|&x| self.ch(x) != '\n' && self.ch(x) != '\r' && self.near(x))
        {
            for x in lws.into_iter().chain(rws) {
                self.delete(x, EditKind::Whitespace);
            }
        }

        // 5. Doubled space.
        let (_, lws) = self.left(k);
        let (rws, _) = self.right(k);
        if !lws.is_empty()
            && !rws.is_empty()
            && lws
                .iter()
                .chain(&rws)
                .all(|&x| self.ch(x) != '\n' && self.ch(x) != '\r')
            && rws.iter().all(|&x| self.near(x))
        {
            for x in rws {
                self.delete(x, EditKind::Whitespace);
            }
        }
    }

    fn finish(self) -> MadeCuts {
        let mut text = String::with_capacity(self.text.len());
        let mut edits: Vec<(usize, usize, String, EditKind)> = self
            .cuts
            .iter()
            .map(|&(s, e)| (s, e, String::new(), EditKind::Cut))
            .collect();
        let mut i = 0;
        while i < self.out.len() {
            let o = self.out[i];
            match o.deleted {
                None => {
                    match self.repl.get(&i) {
                        Some(r) => {
                            text.push_str(r);
                            edits.push((
                                o.pos,
                                o.pos + o.ch.len_utf8(),
                                r.clone(),
                                EditKind::Capitalisation,
                            ));
                        }
                        None => text.push(o.ch),
                    }
                    i += 1;
                }
                Some(kind) => {
                    // One edit per run of contiguous characters of one kind.
                    let start = o.pos;
                    let mut end = o.pos + o.ch.len_utf8();
                    i += 1;
                    while i < self.out.len()
                        && self.out[i].deleted == Some(kind)
                        && self.out[i].pos == end
                    {
                        end += self.out[i].ch.len_utf8();
                        i += 1;
                    }
                    edits.push((start, end, String::new(), kind));
                }
            }
        }
        edits.sort_by_key(|a| (a.0, a.1));
        let mut edits: Vec<Edit> = edits
            .into_iter()
            .map(|(start, end, insert, kind)| Edit {
                start,
                end,
                insert,
                kind,
            })
            .collect();
        let mut offsets: Vec<&mut usize> = edits
            .iter_mut()
            .flat_map(|e| [&mut e.start, &mut e.end])
            .collect();
        bytes_to_utf16(self.text, &mut offsets);
        MadeCuts { text, edits }
    }
}
