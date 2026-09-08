use super::score::{Syllable, Thresholds, find_rhymes, syllables_from_pronunciation};
use crate::setting::Theme;
use cmudict_fast::{Cmudict, Symbol};
use gtk::TextTag;
use gtk::prelude::*;
use hypher::Lang;
use rphonetic::{DoubleMetaphone, Encoder};
use sourceview5::Buffer as SourceBuffer;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::str::FromStr;
use std::sync::OnceLock;

/// How many lines back from the current one to compare against when
/// looking for rhymes — bounds the O(lines²) blowup a whole-document
/// comparison would otherwise have, and matches Hirjee & Brown's own
/// "current and previous lines" window.
const LINE_WINDOW: usize = 3;

/// How long to wait after the last keystroke before recomputing rhyme
/// groups — mirrors the autosave debounce in `editor/mod.rs`.
const RHYME_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(400);

const CMUDICT_TXT: &str = include_str!("../../assets/dictionary/cmudict.dict");

/// Foreground colors cycled across rhyme groups, in the order groups first
/// appear in the document — JetBrains-style, distinguishing rhyme groups
/// by *text* color the way an IDE colors keyword vs string vs number,
/// rather than a highlighter-pen background fill (issue #1). 24
/// evenly-spaced hues so real documents (which can easily have 15-20+
/// distinct rhyme groups) mostly get a unique color instead of two
/// unrelated groups coincidentally sharing one; past 24 the assignment
/// wraps (`color_cursor % tags.len()`).
///
/// Two hand-tuned sets: `DARK` is bright/pastel to sit on the Darcula
/// editor background (`#2b2b2b`) next to its `#a9b7c6` body text; `LIGHT`
/// is deeper and more saturated for the IntelliJ-Light background
/// (`#ffffff`). These are `GtkTextTag` `foreground` values picked in Rust
/// (like `editor::vcs_colors` / `syllable_green`), *not* CSS variables —
/// the "three things stay in lockstep" rule in CLAUDE.md governs only the
/// CSS palette / libadwaita / GtkSourceView-scheme triad, not this.
const RHYME_PALETTE_DARK: [&str; 24] = [
    "#d57b7b", "#d5927b", "#d5a87b", "#d5bf7b", "#d5d57b", "#bfd57b", "#a8d57b", "#92d57b",
    "#7bd57b", "#7bd592", "#7bd5a8", "#7bd5bf", "#7bd5d5", "#7bbfd5", "#7ba8d5", "#7b92d5",
    "#7b7bd5", "#927bd5", "#a87bd5", "#bf7bd5", "#d57bd5", "#d57bbf", "#d57ba8", "#d57b92",
];

const RHYME_PALETTE_LIGHT: [&str; 24] = [
    "#a32929", "#a34729", "#a36629", "#a38529", "#a3a329", "#85a329", "#66a329", "#47a329",
    "#29a329", "#29a347", "#29a366", "#29a385", "#29a3a3", "#2985a3", "#2966a3", "#2947a3",
    "#2929a3", "#4729a3", "#6629a3", "#8529a3", "#a329a3", "#a32985", "#a32966", "#a32947",
];

/// The rhyme-group foreground palette for `theme`.
fn rhyme_palette(theme: Theme) -> &'static [&'static str; 24] {
    match theme {
        Theme::Dark => &RHYME_PALETTE_DARK,
        Theme::Light => &RHYME_PALETTE_LIGHT,
    }
}

/// Foreground applied to the *other* rhyme groups while one is hovered, so
/// the hovered group stands out — a low-contrast grey that still reads.
fn dim_grey(theme: Theme) -> &'static str {
    match theme {
        Theme::Dark => "#5c5c5c",
        Theme::Light => "#b0b0b0",
    }
}

/// Common function/filler words excluded from rhyme matching — nearly every
/// document has *some* other word ending in the same sound as "of" or "is"
/// purely by coincidence of English phonology, which buries genuine rhymes
/// under noise instead of highlighting the writer's actual rhyme scheme.
const STOPWORDS: &[&str] = &[
    "a", "an", "the", "of", "in", "on", "at", "to", "is", "it", "if", "as", "so", "no", "do", "be",
    "by", "or", "up", "we", "he", "she", "i", "my", "me", "you", "your", "am", "are", "was",
    "were", "been", "being", "and", "but", "for", "nor", "yet", "with", "from", "into", "onto",
    "than", "then", "this", "that", "these", "those", "there", "their", "they", "them", "its",
    "his", "her", "him", "our", "us", "oh", "ah", "uh", "well", "just", "not", "all", "any", "can",
    "could", "would", "should", "will", "shall", "may", "might", "must", "did", "does", "done",
    "had", "has", "have", "let", "get", "got", "go", "goes", "one", "two", "out", "off", "down",
    "over", "under", "again", "also", "too", "very", "much", "some", "such", "same", "own", "each",
    "every", "both", "few", "more", "most", "other", "only", "which", "who", "whom", "what",
    "when", "where", "why", "how",
];

fn stopwords() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| STOPWORDS.iter().copied().collect())
}

fn cmudict() -> &'static Cmudict {
    static DICT: OnceLock<Cmudict> = OnceLock::new();
    DICT.get_or_init(|| Cmudict::from_str(CMUDICT_TXT).expect("bundled cmudict.dict should parse"))
}

fn symbol_base(symbol: &Symbol) -> &'static str {
    use Symbol::*;
    match symbol {
        AA(_) => "AA",
        AE(_) => "AE",
        AH(_) => "AH",
        AO(_) => "AO",
        AW(_) => "AW",
        AY(_) => "AY",
        B => "B",
        CH => "CH",
        D => "D",
        DH => "DH",
        EH(_) => "EH",
        ER(_) => "ER",
        EY(_) => "EY",
        F => "F",
        G => "G",
        HH => "HH",
        IH(_) => "IH",
        IY(_) => "IY",
        JH => "JH",
        K => "K",
        L => "L",
        M => "M",
        N => "N",
        NG => "NG",
        OW(_) => "OW",
        OY(_) => "OY",
        P => "P",
        R => "R",
        S => "S",
        SH => "SH",
        T => "T",
        TH => "TH",
        UH(_) => "UH",
        UW(_) => "UW",
        V => "V",
        W => "W",
        Y => "Y",
        Z => "Z",
        ZH => "ZH",
    }
}

/// Split a word's CMU pronunciation into syllables, each consisting of a
/// vowel (nucleus) plus every consonant up to — but not including — the
/// next vowel (its coda). Onset consonants (before the vowel) are
/// deliberately left out of each syllable: they never participate in a
/// rhyme, so excluding them here means the caller never has to worry about
/// stripping them back out of a rhyme key or a highlight span.
fn syllabify(pronunciation: &[Symbol]) -> Vec<Vec<Symbol>> {
    let vowel_positions: Vec<usize> = pronunciation
        .iter()
        .enumerate()
        .filter(|(_, s)| s.is_syllable())
        .map(|(i, _)| i)
        .collect();

    if vowel_positions.is_empty() {
        return vec![pronunciation.to_vec()];
    }

    vowel_positions
        .iter()
        .enumerate()
        .map(|(i, &vowel_pos)| {
            let end = vowel_positions
                .get(i + 1)
                .copied()
                .unwrap_or(pronunciation.len());
            pronunciation[vowel_pos..end].to_vec()
        })
        .collect()
}

/// Letter-level syllable spans for `word` from Knuth–Liang hyphenation
/// patterns, used when they agree with `target_count` (the phonetic
/// syllable count). Hyphenation directly encodes English's actual
/// letter-break conventions — "can-dle", not "cand-le" — which the vowel-run
/// heuristic below gets wrong for syllabic-consonant endings like "-le",
/// "-en", "-er" (there's no written vowel to anchor on). Returns `None`
/// when hyphenation disagrees with the phonetic count (it's tuned to avoid
/// ugly typographic breaks, so it sometimes yields fewer pieces than there
/// are phonetic syllables — e.g. "closer" doesn't hyphenate to "clo-ser")
/// or when the word has characters hyphenation patterns aren't meant for
/// (an apostrophe); callers fall back to the vowel-run heuristic then.
fn hyphenation_spans(word: &str, target_count: usize) -> Option<Vec<(usize, usize)>> {
    if !word.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }

    let pieces: Vec<&str> = hypher::hyphenate(word, Lang::English).collect();
    if pieces.len() != target_count {
        return None;
    }

    let mut spans = Vec::with_capacity(pieces.len());
    let mut cursor = 0;
    for piece in pieces {
        let len = piece.chars().count();
        spans.push((cursor, cursor + len));
        cursor += len;
    }
    Some(spans)
}

/// The mirror image of `syllabify`, applied to spelling instead of
/// phonemes: split `word` into `target_count` character spans, each
/// starting at a run of vowel letters and extending to (not including) the
/// next one. Used as a fallback only when `hyphenation_spans` can't help
/// (see above) — it's cheaper but less accurate, since pairing "i-th
/// vowel-letter run" with "i-th phonetic syllable" doesn't know about
/// syllabic consonants, silent letters, etc. The merge/split logic below at
/// least keeps it from desyncing when a word's letter-vowel-run count
/// doesn't match its syllable count.
fn vowel_run_syllables(word: &str, target_count: usize) -> Vec<(usize, usize)> {
    let is_vowel = |c: char| matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u' | 'y');
    let char_count = word.chars().count();

    let mut vowel_run_starts = Vec::new();
    let mut prev_vowel = false;
    for (i, c) in word.chars().enumerate() {
        let v = is_vowel(c);
        if v && !prev_vowel {
            vowel_run_starts.push(i);
        }
        prev_vowel = v;
    }

    let mut spans: Vec<(usize, usize)> = if vowel_run_starts.is_empty() {
        vec![(0, char_count)]
    } else {
        vowel_run_starts
            .iter()
            .enumerate()
            .map(|(i, &s)| {
                (
                    s,
                    vowel_run_starts.get(i + 1).copied().unwrap_or(char_count),
                )
            })
            .collect()
    };

    // More vowel-letter runs than phonetic syllables (e.g. silent e in
    // "like"): fold the extra ones into the syllable before them.
    while spans.len() > target_count && spans.len() > 1 {
        let (_, last_end) = spans.pop().unwrap();
        spans.last_mut().unwrap().1 = last_end;
    }
    // Fewer vowel-letter runs than phonetic syllables (e.g. "our" spelled
    // with one letter-run but pronounced as two syllables): split the
    // trailing run so every syllable still gets a (possibly short) span.
    while spans.len() < target_count {
        let (s, e) = spans.pop().unwrap();
        let mid = s + (e - s) / 2;
        spans.push((s, mid));
        spans.push((mid, e));
    }

    spans
}

/// Letter spans for each of `word`'s `target_count` syllables — preferring
/// `hyphenation_spans`, falling back to `vowel_run_syllables`. Either way,
/// the first syllable's leading onset consonants (e.g. the "c" in "candle")
/// get trimmed off afterward: onset never participates in a rhyme, and
/// `vowel_run_syllables` already excludes it by construction, so this just
/// makes `hyphenation_spans` (which includes it, since a hyphenation piece
/// is a full orthographic syllable, onset and all) consistent with that.
fn orthographic_syllables(word: &str, target_count: usize) -> Vec<(usize, usize)> {
    let target_count = target_count.max(1);
    let mut spans = hyphenation_spans(word, target_count)
        .unwrap_or_else(|| vowel_run_syllables(word, target_count));

    if let Some(first) = spans.first_mut() {
        let is_vowel =
            |c: char| matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u' | 'y');
        let onset_len = word
            .chars()
            .skip(first.0)
            .take(first.1 - first.0)
            .take_while(|&c| !is_vowel(c))
            .count();
        first.0 += onset_len;
    }

    spans
}

/// One syllable's rhyme key plus the character span (into the word) it
/// covers.
#[derive(Debug)]
struct RhymeUnit {
    key: String,
    start: usize,
    end: usize,
}

/// Every rhymeable syllable in `word`, each independently comparable
/// against every other syllable in the document — so a multi-syllable word
/// can rhyme with different words on different syllables (e.g. "closer"
/// might share its first syllable with one word and its last with
/// another). Prefers the CMU pronouncing dictionary; falls back to a single
/// double-metaphone-keyed unit covering the word's last vowel-letter run
/// onward for words the dictionary doesn't know (proper nouns, slang) —
/// there's no phoneme data to syllabify in that case.
fn rhyme_units(word: &str) -> Vec<RhymeUnit> {
    let lower = word.to_lowercase();
    if !lower.chars().any(|c| c.is_alphabetic()) {
        return Vec::new();
    }

    if let Some(rule) = cmudict().get(&lower).and_then(|rules| rules.first()) {
        let syllables = syllabify(rule.pronunciation());
        let spans = orthographic_syllables(&lower, syllables.len());
        return syllables
            .iter()
            .zip(spans)
            .filter(|(_, (start, end))| end > start)
            .map(|(phonemes, (start, end))| RhymeUnit {
                key: phonemes
                    .iter()
                    .map(symbol_base)
                    .collect::<Vec<_>>()
                    .join("-"),
                start,
                end,
            })
            .collect();
    }

    let is_vowel = |c: char| matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u' | 'y');
    let mut start = 0;
    let mut prev_vowel = false;
    for (i, c) in lower.chars().enumerate() {
        let v = is_vowel(c);
        if v && !prev_vowel {
            start = i;
        }
        prev_vowel = v;
    }
    // `rphonetic`'s double-metaphone is ASCII-only and byte-slices its
    // input — feeding it a diacritic (`café`) or non-Latin script panics.
    // Key on just the ASCII-letter tail; if nothing's left, there's no
    // metaphone to compute.
    let ascii_tail: String = lower
        .chars()
        .skip(start)
        .filter(|c| c.is_ascii_alphabetic())
        .collect();
    if ascii_tail.is_empty() {
        return Vec::new();
    }
    vec![RhymeUnit {
        key: format!("dm:{}", DoubleMetaphone::default().encode(&ascii_tail)),
        start,
        end: lower.chars().count(),
    }]
}

struct WordSpan {
    /// Character offsets (not byte offsets — `TextIter` counts characters).
    start: usize,
    end: usize,
    text: String,
}

fn tokenize(text: &str) -> Vec<WordSpan> {
    let mut spans = Vec::new();
    let mut start = None;
    let mut current = String::new();

    for (i, c) in text.chars().enumerate() {
        let is_word_char = c.is_alphabetic() || (c == '\'' && start.is_some());
        if is_word_char {
            if start.is_none() {
                start = Some(i);
            }
            current.push(c);
        } else if let Some(s) = start.take() {
            spans.push(WordSpan {
                start: s,
                end: i,
                text: std::mem::take(&mut current),
            });
        }
    }
    if let Some(s) = start.take() {
        spans.push(WordSpan {
            start: s,
            end: text.chars().count(),
            text: current,
        });
    }

    spans
}

/// One line's rhymeable syllables, in reading order across its
/// non-stopword words, paired with each syllable's absolute character span
/// in the document (parallel to `syllables`).
struct LineSyllables {
    syllables: Vec<Syllable>,
    char_spans: Vec<(usize, usize)>,
    /// Whether the line's raw text is empty/whitespace-only — a stanza
    /// break the comparison window shouldn't cross (see `score_lines`).
    /// Independent of whether `syllables` is empty, since a line of pure
    /// stopwords is also syllable-empty but isn't a stanza boundary.
    is_blank: bool,
}

/// Character offset of the start of each line (line 0 always starts at 0).
fn line_start_offsets(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, c) in text.chars().enumerate() {
        if c == '\n' {
            starts.push(i + 1);
        }
    }
    starts
}

fn line_of(starts: &[usize], offset: usize) -> usize {
    starts.partition_point(|&s| s <= offset) - 1
}

/// Builds each line's syllable sequence for the Hirjee & Brown scorer. Only
/// words the CMU dictionary knows contribute — there's no phoneme data to
/// score an out-of-dictionary word against, so those are left to the
/// separate exact-match fallback in `recompute`.
fn build_lines(text: &str, spans: &[WordSpan]) -> Vec<LineSyllables> {
    let starts = line_start_offsets(text);
    let mut lines: Vec<LineSyllables> = text
        .split('\n')
        .map(|raw| LineSyllables {
            syllables: Vec::new(),
            char_spans: Vec::new(),
            is_blank: raw.trim().is_empty(),
        })
        .collect();
    debug_assert_eq!(lines.len(), starts.len());

    for span in spans {
        if span.end - span.start < 2 || stopwords().contains(span.text.to_lowercase().as_str()) {
            continue;
        }
        let lower = span.text.to_lowercase();
        let Some(rule) = cmudict().get(&lower).and_then(|rules| rules.first()) else {
            continue;
        };
        let syllables = syllables_from_pronunciation(rule.pronunciation());
        if syllables.is_empty() {
            continue;
        }
        let char_spans = orthographic_syllables(&lower, syllables.len());
        let line = &mut lines[line_of(&starts, span.start)];
        for (syllable, (s, e)) in syllables.into_iter().zip(char_spans) {
            if e <= s {
                continue;
            }
            line.syllables.push(syllable);
            line.char_spans.push((span.start + s, span.start + e));
        }
    }

    lines
}

/// Minimal union-find over a growable set of nodes, used to collapse
/// pairwise rhyme matches (possibly chained across several lines) into
/// connected groups for coloring.
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl UnionFind {
    fn new() -> Self {
        Self {
            parent: Vec::new(),
            rank: Vec::new(),
        }
    }

    fn push(&mut self) -> usize {
        let id = self.parent.len();
        self.parent.push(id);
        self.rank.push(0);
        id
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return;
        }
        match self.rank[ra].cmp(&self.rank[rb]) {
            std::cmp::Ordering::Less => self.parent[ra] = rb,
            std::cmp::Ordering::Greater => self.parent[rb] = ra,
            std::cmp::Ordering::Equal => {
                self.parent[rb] = ra;
                self.rank[ra] += 1;
            }
        }
    }
}

/// Minimum length-normalized score for a detected span to actually merge
/// its syllables into a color group — distinctly higher than
/// `Thresholds::anchor`, which only governs whether `find_rhymes` detects a
/// match at all.
///
/// This matters because rhyme isn't transitive: union-find is, so a chain
/// of individually-plausible matches (e.g. every unstressed word in the
/// document ending in the same common open vowel, like "-y") transitively
/// fuses into one giant component covering most of the document, even
/// though most of those pairs are bare assonance rather than a rhyme
/// scheme the writer intended — the low-scoring "bridge" pairs are exactly
/// the ones that shouldn't be allowed to merge unrelated families
/// together. A bare open, unstressed vowel match (no coda, no stress
/// bonus) tops out around the self-score of the vowel alone — mid-2s for
/// most vowels. It also needs to sit above the weakest *legitimate* bare
/// open rhyme (a matched primary-stressed vowel with no coda on either
/// side, e.g. "day"/"way"), whose floor across the common open vowels is
/// ~3.3 (vowel self-score plus the 1.0 matched-primary-stress bonus),
/// while still excluding matches that only clear a lower bar by combining
/// a so-so vowel score with a weakly-positive but not really
/// similar-sounding consonant pair (e.g. /v/:/g/ scores a mild +0.3
/// despite not sounding alike, per Table 2) — that shouldn't be enough on
/// its own to bridge two otherwise-unrelated words together.
const MERGE_THRESHOLD: f32 = 3.25;

/// How far back line `i` may be compared against: up to `LINE_WINDOW`
/// lines, and — when `stop_at_blank_line` is set (see
/// `Settings::rhyme_stop_at_blank_line`) — never crossing a blank line,
/// since rhyme schemes don't usually reach across a stanza break and
/// stopping there keeps a long verse from comparing against an unrelated
/// stanza just because it's within the flat line count.
fn stanza_bounded_window_start(
    lines: &[LineSyllables],
    i: usize,
    stop_at_blank_line: bool,
) -> usize {
    let mut start = i;
    for k in (0..i).rev() {
        if i - k > LINE_WINDOW || (stop_at_blank_line && lines[k].is_blank) {
            break;
        }
        start = k;
    }
    start
}

/// Runs Hirjee & Brown anchor-and-extend detection over every line against
/// itself and its predecessors within `stanza_bounded_window_start`, unions
/// the syllables on either side of every match scoring above
/// `MERGE_THRESHOLD`, and returns
/// each resulting connected component as its members' character spans —
/// one group per rhyme scheme color. Groups are sorted by their earliest
/// character span so recomputes keep assigning the same colors to the same
/// rhymes instead of reshuffling (a plain `HashMap`'s iteration order isn't
/// stable across runs).
fn score_lines(lines: &[LineSyllables], stop_at_blank_line: bool) -> Vec<Vec<(usize, usize)>> {
    let mut uf = UnionFind::new();
    let mut node_of: HashMap<(usize, usize), usize> = HashMap::new();
    let thresholds = Thresholds::default();

    for i in 0..lines.len() {
        let window_start = stanza_bounded_window_start(lines, i, stop_at_blank_line);
        for j in window_start..=i {
            for span in find_rhymes(&lines[i].syllables, &lines[j].syllables, &thresholds) {
                // Comparing a line against itself always scores a trivial
                // full match on the identity span — not a real rhyme.
                if i == j && span.a == span.b {
                    continue;
                }
                let len = (span.a.end - span.a.start) as f32;
                if span.score / len < MERGE_THRESHOLD {
                    continue;
                }

                // Every syllable in this span — on both sides — is one
                // rhyme instance, so it should render as a single color,
                // not one color per aligned syllable pair: union each pair
                // together, and chain each pair to the previous one so the
                // whole span ends up in one component.
                let mut previous: Option<usize> = None;
                for (a_idx, b_idx) in span.a.clone().zip(span.b.clone()) {
                    let na = *node_of.entry((i, a_idx)).or_insert_with(|| uf.push());
                    let nb = *node_of.entry((j, b_idx)).or_insert_with(|| uf.push());
                    uf.union(na, nb);
                    if let Some(prev) = previous {
                        uf.union(prev, na);
                    }
                    previous = Some(na);
                }
            }
        }
    }

    let mut by_root: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();
    for (&(line, syl), &node) in &node_of {
        let root = uf.find(node);
        by_root
            .entry(root)
            .or_default()
            .push(lines[line].char_spans[syl]);
    }

    let mut groups: Vec<Vec<(usize, usize)>> = by_root
        .into_values()
        .filter(|members| members.len() >= 2)
        .collect();
    for members in &mut groups {
        members.sort_unstable();
        members.dedup();
    }
    groups.sort_by_key(|members| members[0]);
    groups
}

fn create_tags(buffer: &SourceBuffer, theme: Theme) -> Vec<TextTag> {
    rhyme_palette(theme)
        .iter()
        .enumerate()
        .map(|(i, &color)| {
            buffer
                .create_tag(Some(&format!("rhymr-rhyme-{i}")), &[("foreground", &color)])
                .expect("tag name is unique per buffer")
        })
        .collect()
}

fn apply_group(buffer: &SourceBuffer, tag: &TextTag, members: &[(usize, usize)]) {
    for &(s, e) in members {
        let start = buffer.iter_at_offset(s as i32);
        let end = buffer.iter_at_offset(e as i32);
        buffer.apply_tag(tag, &start, &end);
    }
}

/// One rhyme group: which palette slot colors it, that color's hex (for
/// the current `theme`), a representative word (for the legend), and every
/// character span it covers (for hover-to-emphasise).
#[derive(Clone, Debug)]
pub struct RhymeGroup {
    pub color_index: usize,
    pub color: String,
    pub label: String,
    pub spans: Vec<(usize, usize)>,
}

/// The word `offset` falls inside, lower-cased — for the legend label. A
/// group's stored spans start mid-word (onset consonants are trimmed by
/// `orthographic_syllables`), so widen out to the surrounding word.
fn word_at(buffer: &SourceBuffer, offset: usize) -> String {
    let mut start = buffer.iter_at_offset(offset as i32);
    if !start.starts_word() {
        start.backward_word_start();
    }
    let mut end = start;
    if !end.ends_word() {
        end.forward_word_end();
    }
    buffer.text(&start, &end, false).trim().to_lowercase()
}

/// Recompute rhyme groups for `buffer`, repaint `tags`, and return the
/// groups (color slot + hex for `theme` + a representative word) in the
/// same order the colors were assigned — so the legend and the buffer
/// agree on which group is which color.
fn recompute(buffer: &SourceBuffer, tags: &[TextTag], theme: Theme) -> Vec<RhymeGroup> {
    let start_iter = buffer.start_iter();
    let end_iter = buffer.end_iter();
    for tag in tags {
        buffer.remove_tag(tag, &start_iter, &end_iter);
    }

    let text = buffer.text(&start_iter, &end_iter, false).to_string();
    let word_spans = tokenize(&text);

    // Out-of-dictionary words (slang, proper nouns) have no phoneme data to
    // score against the Hirjee & Brown model, so they keep the original
    // exact-key matching, document-wide. In-dictionary words are excluded
    // here — they're handled by `score_lines` below instead.
    //
    // Grouped by rhyme key, preserving the order each key first appears in
    // the document so re-runs assign the same colors to the same groups
    // instead of reshuffling (unlike a plain HashMap, whose iteration order
    // is randomized per-process).
    let mut key_index: HashMap<String, usize> = HashMap::new();
    let mut fallback_groups: Vec<Vec<(usize, usize)>> = Vec::new();

    for span in &word_spans {
        if span.end - span.start < 2 {
            continue;
        }
        let lower = span.text.to_lowercase();
        if stopwords().contains(lower.as_str()) || cmudict().get(&lower).is_some() {
            continue;
        }
        for unit in rhyme_units(&span.text) {
            let idx = *key_index.entry(unit.key).or_insert_with(|| {
                fallback_groups.push(Vec::new());
                fallback_groups.len() - 1
            });
            fallback_groups[idx].push((span.start + unit.start, span.start + unit.end));
        }
    }

    let lines = build_lines(&text, &word_spans);
    let stop_at_blank_line = crate::setting::Settings::load().rhyme_stop_at_blank_line;
    let scored_groups = score_lines(&lines, stop_at_blank_line);

    let all_groups = fallback_groups
        .iter()
        .filter(|m| m.len() >= 2)
        .chain(scored_groups.iter().filter(|m| m.len() >= 2));
    let palette = rhyme_palette(theme);
    let mut groups = Vec::new();
    for (color_cursor, members) in all_groups.enumerate() {
        let color_index = color_cursor % tags.len();
        apply_group(buffer, &tags[color_index], members);
        let anchor = members.iter().map(|&(s, _)| s).min().unwrap_or(0);
        groups.push(RhymeGroup {
            color_index,
            color: palette[color_index].to_string(),
            label: word_at(buffer, anchor),
            spans: members.clone(),
        });
    }
    groups
}

type GroupsCallback = Box<dyn Fn(&[RhymeGroup])>;

/// A live `attach()` — dropping/`detach()`-ing this stops recoloring the
/// buffer and clears whatever rhyme-group colors are currently applied, so
/// toggling the setting off doesn't leave stale highlights behind.
pub struct RhymeHighlight {
    buffer: SourceBuffer,
    tags: Vec<TextTag>,
    /// Painted over every *other* group's spans while one group is hovered
    /// (see [`emphasise_group`]). Created after `tags`, so it out-prioritises
    /// them where they overlap.
    ///
    /// [`emphasise_group`]: RhymeHighlight::emphasise_group
    dim_tag: TextTag,
    /// The group index currently emphasised, so repeated motion over the
    /// same word is a no-op. Shared with the recompute closure, which
    /// resets it (and clears the dim tag) when the groups change.
    emphasised: Rc<Cell<Option<usize>>>,
    /// The theme the `tags` are currently colored for — see [`set_theme`].
    ///
    /// [`set_theme`]: RhymeHighlight::set_theme
    theme: Rc<Cell<Theme>>,
    /// The groups from the last `recompute`, in color-assignment order —
    /// what the legend renders and what a live theme switch re-colors.
    groups: Rc<RefCell<Vec<RhymeGroup>>>,
    /// Notified with the current groups after every recompute / theme
    /// change, and with an empty slice on `detach`.
    on_groups: Rc<RefCell<Option<GroupsCallback>>>,
    handler_id: Option<glib::SignalHandlerId>,
}

impl RhymeHighlight {
    pub fn detach(mut self) {
        if let Some(id) = self.handler_id.take() {
            self.buffer.disconnect(id);
        }
        let start = self.buffer.start_iter();
        let end = self.buffer.end_iter();
        for tag in &self.tags {
            self.buffer.remove_tag(tag, &start, &end);
        }
        self.buffer.remove_tag(&self.dim_tag, &start, &end);
        self.groups.borrow_mut().clear();
        if let Some(cb) = self.on_groups.borrow().as_ref() {
            cb(&[]);
        }
    }

    /// Re-point every rhyme tag at `theme`'s palette. A `GtkTextTag`'s
    /// `foreground` recolors every range it's already applied to, so a live
    /// theme switch (Settings → Appearance) needs no recompute — just this
    /// plus refreshing the legend's stored hex colors.
    pub fn set_theme(&self, theme: Theme) {
        if self.theme.replace(theme) == theme {
            return;
        }
        let palette = rhyme_palette(theme);
        for (i, tag) in self.tags.iter().enumerate() {
            tag.set_foreground(Some(palette[i % palette.len()]));
        }
        self.dim_tag.set_foreground(Some(dim_grey(theme)));
        for group in self.groups.borrow_mut().iter_mut() {
            group.color = palette[group.color_index].to_string();
        }
        self.notify_groups();
    }

    /// Emphasise one rhyme group by dimming every *other* group's spans;
    /// `None` clears the emphasis. O(spans) only on a group transition —
    /// repeated motion within the same word does nothing, and it never
    /// recomputes. Call from a pointer-motion handler on the view.
    pub fn emphasise_group(&self, group: Option<usize>) {
        if self.emphasised.get() == group {
            return;
        }
        self.emphasised.set(group);

        let start = self.buffer.start_iter();
        let end = self.buffer.end_iter();
        self.buffer.remove_tag(&self.dim_tag, &start, &end);

        if let Some(target) = group {
            for (i, g) in self.groups.borrow().iter().enumerate() {
                if i == target {
                    continue;
                }
                for &(s, e) in &g.spans {
                    let s = self.buffer.iter_at_offset(s as i32);
                    let e = self.buffer.iter_at_offset(e as i32);
                    self.buffer.apply_tag(&self.dim_tag, &s, &e);
                }
            }
        }
    }

    /// Index of the rhyme group whose spans contain character `offset`, if
    /// any — for turning a hover position into a group to emphasise.
    pub fn group_at_offset(&self, offset: usize) -> Option<usize> {
        self.groups
            .borrow()
            .iter()
            .position(|g| g.spans.iter().any(|&(s, e)| offset >= s && offset < e))
    }

    /// Register `f` to receive the active rhyme groups whenever they
    /// change; fires once immediately with the current set.
    pub fn connect_groups_changed(&self, f: impl Fn(&[RhymeGroup]) + 'static) {
        *self.on_groups.borrow_mut() = Some(Box::new(f));
        self.notify_groups();
    }

    fn notify_groups(&self) {
        if let Some(cb) = self.on_groups.borrow().as_ref() {
            cb(&self.groups.borrow());
        }
    }
}

/// Recolors words in `buffer` by rhyme group, recomputing (debounced) on
/// every edit. `theme` picks the foreground palette; keep it current with
/// [`RhymeHighlight::set_theme`]. Subscribe to the active-group list with
/// [`RhymeHighlight::connect_groups_changed`].
pub fn attach(buffer: &SourceBuffer, theme: Theme) -> RhymeHighlight {
    let tags = create_tags(buffer, theme);
    // Created last, so it wins over the color tags where they overlap.
    let dim_tag = buffer
        .create_tag(Some("rhymr-rhyme-dim"), &[("foreground", &dim_grey(theme))])
        .expect("tag name is unique per buffer");
    let theme = Rc::new(Cell::new(theme));
    let emphasised = Rc::new(Cell::new(None));
    let groups = Rc::new(RefCell::new(recompute(buffer, &tags, theme.get())));
    let on_groups: Rc<RefCell<Option<GroupsCallback>>> = Rc::new(RefCell::new(None));

    let generation = Rc::new(Cell::new(0u64));
    let tags_for_signal = tags.clone();
    let dim_for_signal = dim_tag.clone();
    let theme_for_signal = theme.clone();
    let emphasised_for_signal = emphasised.clone();
    let groups_for_signal = groups.clone();
    let on_groups_for_signal = on_groups.clone();
    let handler_id = buffer.connect_changed(move |buf| {
        let this_generation = generation.get() + 1;
        generation.set(this_generation);

        let generation_for_timeout = generation.clone();
        let buf_owned = buf.clone();
        let tags_for_timeout = tags_for_signal.clone();
        let dim_for_timeout = dim_for_signal.clone();
        let theme_for_timeout = theme_for_signal.clone();
        let emphasised_for_timeout = emphasised_for_signal.clone();
        let groups_for_timeout = groups_for_signal.clone();
        let on_groups_for_timeout = on_groups_for_signal.clone();
        glib::timeout_add_local_once(RHYME_DEBOUNCE, move || {
            if generation_for_timeout.get() != this_generation {
                return;
            }
            let fresh = recompute(&buf_owned, &tags_for_timeout, theme_for_timeout.get());
            groups_for_timeout.replace(fresh);
            // The old hover-emphasis is now stale — clear it.
            buf_owned.remove_tag(
                &dim_for_timeout,
                &buf_owned.start_iter(),
                &buf_owned.end_iter(),
            );
            emphasised_for_timeout.set(None);
            if let Some(cb) = on_groups_for_timeout.borrow().as_ref() {
                cb(&groups_for_timeout.borrow());
            }
        });
    });

    RhymeHighlight {
        buffer: buffer.clone(),
        tags,
        dim_tag,
        emphasised,
        theme,
        groups,
        on_groups,
        handler_id: Some(handler_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rhyme_units_handles_non_ascii_words_without_panicking() {
        // `rphonetic`'s double-metaphone byte-slices its input; accented
        // and non-Latin words used to panic here (café → inside 'é').
        for word in [
            "café",
            "naïve",
            "façade",
            "résumé",
            "émeute",
            "Montréal",
            "двойной",
            "переносе",
            "🔥bars🔥",
            "señor",
        ] {
            for unit in rhyme_units(word) {
                assert!(unit.start <= unit.end);
                assert!(unit.end <= word.chars().count());
            }
        }
    }

    #[test]
    fn rhyme_units_never_panics_or_produces_invalid_spans_across_the_dictionary() {
        for word in dictionary_sample() {
            for unit in rhyme_units(word) {
                assert!(
                    unit.start <= unit.end,
                    "{word:?}: unit {unit:?} has start > end"
                );
                assert!(
                    unit.end <= word.chars().count(),
                    "{word:?}: unit {unit:?} runs past the word"
                );
            }
        }
    }

    /// A large, deterministic sample of real dictionary words pulled
    /// straight out of the bundled `cmudict.dict` — cheaper than iterating
    /// all ~135k entries on every test run, but still broad enough to catch
    /// off-by-one bugs in the syllable/span reconciliation logic.
    fn dictionary_sample() -> Vec<&'static str> {
        CMUDICT_TXT
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .step_by(7)
            .collect()
    }

    #[test]
    fn orthographic_syllables_always_returns_target_count_spans_covering_no_more_than_the_word() {
        for word in dictionary_sample() {
            for target in 1..=4 {
                let spans = orthographic_syllables(word, target);
                assert_eq!(spans.len(), target, "{word:?} with target {target}");
                let char_count = word.chars().count();
                for &(s, e) in &spans {
                    assert!(
                        s <= e && e <= char_count,
                        "{word:?} target {target}: span ({s}, {e})"
                    );
                }
            }
        }
    }

    #[test]
    fn hyphenation_fixes_syllabic_consonant_endings() {
        // "candle" was the motivating failure case: CMU pronounces it as
        // two syllables (K AE1 N / D AH0 L), and the vowel-run-only
        // heuristic assigned the "l" to the wrong syllable since the
        // second syllable's nucleus is a syllabic /l/ with no written
        // vowel to anchor on — it produced ("andl", "e") instead of
        // ("can", "dle"). Hyphenation patterns encode the real
        // letter-break convention and get this right.
        let units = rhyme_units("candle");
        assert_eq!(units.len(), 2, "{units:?}");
        let word: Vec<char> = "candle".chars().collect();
        let text = |u: &RhymeUnit| word[u.start..u.end].iter().collect::<String>();
        assert_eq!(text(&units[0]), "an", "{units:?}");
        assert_eq!(text(&units[1]), "dle", "{units:?}");
    }

    #[test]
    fn multi_syllable_word_can_produce_multiple_independent_rhyme_units() {
        // "closer" is two syllables (CMU: K L OW1 Z ER0) — confirms a
        // multi-syllable word yields more than one independently-keyed unit
        // instead of collapsing to a single whole-word key.
        let units = rhyme_units("closer");
        assert!(
            units.len() >= 2,
            "expected multiple syllable units for \"closer\", got {units:?}"
        );
    }

    /// True if `span` (an absolute character range, as stored in a
    /// `LineSyllables::char_spans` entry) falls within `word`'s own
    /// character range — looser than an exact-offset comparison since
    /// `orthographic_syllables` trims a syllable's leading onset
    /// consonants off, so a monosyllabic word's stored span is a strict
    /// subrange of its full `WordSpan`, not identical to it.
    fn falls_within(span: &(usize, usize), word: &WordSpan) -> bool {
        span.0 >= word.start && span.1 <= word.end
    }

    #[test]
    fn score_lines_groups_a_simple_end_rhyme_across_two_lines() {
        let text = "I saw a cat\nI saw a hat";
        let spans = tokenize(text);
        let lines = build_lines(text, &spans);
        assert_eq!(lines.len(), 2);

        let groups = score_lines(&lines, true);
        let cat = spans.iter().find(|s| s.text == "cat").unwrap();
        let hat = spans.iter().find(|s| s.text == "hat").unwrap();
        assert!(
            groups.iter().any(|g| g.iter().any(|m| falls_within(m, cat))
                && g.iter().any(|m| falls_within(m, hat))),
            "expected \"cat\"/\"hat\" to form a rhyme group, got {groups:?}"
        );
    }

    #[test]
    fn score_lines_does_not_link_rhymes_outside_the_line_window() {
        let mut lines: Vec<String> = vec!["first line with cat".to_string()];
        for i in 0..(LINE_WINDOW + 2) {
            lines.push(format!("filler line number {i}"));
        }
        lines.push("last line with hat".to_string());
        let text = lines.join("\n");

        let spans = tokenize(&text);
        let built = build_lines(&text, &spans);
        assert_eq!(built.len(), lines.len());

        let groups = score_lines(&built, true);
        let cat = spans.iter().find(|s| s.text == "cat").unwrap();
        let hat = spans.iter().find(|s| s.text == "hat").unwrap();
        assert!(
            !groups.iter().any(|g| g.iter().any(|m| falls_within(m, cat))
                && g.iter().any(|m| falls_within(m, hat))),
            "\"cat\"/\"hat\" are {} lines apart, outside LINE_WINDOW={LINE_WINDOW} — should not be grouped: {groups:?}",
            LINE_WINDOW + 2
        );
    }

    #[test]
    fn score_lines_does_not_cross_a_blank_line_even_within_the_line_window() {
        // "cat" and "hat" are only 2 lines apart (well inside LINE_WINDOW),
        // but a blank line — a stanza break — sits between them, so they
        // still shouldn't be linked.
        let text = "first line with cat\n\nlast line with hat";
        let spans = tokenize(text);
        let lines = build_lines(text, &spans);
        assert_eq!(lines.len(), 3);
        assert!(lines[1].is_blank);

        let groups = score_lines(&lines, true);
        let cat = spans.iter().find(|s| s.text == "cat").unwrap();
        let hat = spans.iter().find(|s| s.text == "hat").unwrap();
        assert!(
            !groups.iter().any(|g| g.iter().any(|m| falls_within(m, cat))
                && g.iter().any(|m| falls_within(m, hat))),
            "\"cat\"/\"hat\" are separated by a blank line and should not be grouped: {groups:?}"
        );
    }

    #[test]
    fn score_lines_can_cross_a_blank_line_when_stop_at_blank_line_is_off() {
        let text = "first line with cat\n\nlast line with hat";
        let spans = tokenize(text);
        let lines = build_lines(text, &spans);

        let groups = score_lines(&lines, false);
        let cat = spans.iter().find(|s| s.text == "cat").unwrap();
        let hat = spans.iter().find(|s| s.text == "hat").unwrap();
        assert!(
            groups.iter().any(|g| g.iter().any(|m| falls_within(m, cat))
                && g.iter().any(|m| falls_within(m, hat))),
            "with stop_at_blank_line disabled, \"cat\"/\"hat\" are still within LINE_WINDOW and should be grouped: {groups:?}"
        );
    }

    #[test]
    fn score_lines_does_not_merge_bare_open_unstressed_vowel_matches() {
        // Regression test: "heavy" and "legacy" both end in an unstressed,
        // coda-less /i/ (HH EH1 V IY0 / L EH1 G AH0 S IY0) — real lyrics
        // from a bug report had many such line endings ("heavy", "legacy",
        // "energy", "mentally", ...) which, before MERGE_THRESHOLD
        // existed, all transitively fused into one color spanning most of
        // the document via union-find. A bare vowel-only match like this
        // must stay ungrouped.
        let text = "the vibe felt heavy\nprotecting the legacy";
        let spans = tokenize(text);
        let lines = build_lines(text, &spans);
        let groups = score_lines(&lines, true);

        let heavy = spans.iter().find(|s| s.text == "heavy").unwrap();
        let legacy = spans.iter().find(|s| s.text == "legacy").unwrap();
        assert!(
            !groups
                .iter()
                .any(|g| g.iter().any(|m| falls_within(m, heavy))
                    && g.iter().any(|m| falls_within(m, legacy))),
            "\"heavy\"/\"legacy\" only share a bare unstressed vowel and should not be grouped: {groups:?}"
        );
    }

    #[test]
    fn build_lines_skips_stopwords_and_out_of_dictionary_words() {
        let text = "the cat xyzzyplonk";
        let spans = tokenize(text);
        let lines = build_lines(text, &spans);
        assert_eq!(lines.len(), 1);
        // "the" is a stopword and "xyzzyplonk" isn't in the dictionary —
        // only "cat"'s syllable should make it in.
        assert_eq!(lines[0].syllables.len(), 1);
    }
}
