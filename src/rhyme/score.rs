//! Syllable-level rhyme scoring and detection, following Hirjee & Brown,
//! *Using Automated Rhyme Detection to Characterize Rhyming Style in Rap
//! Music* (Empirical Musicology Review 5(4), 2010). The vowel and consonant
//! log-odds matrices below are transcribed from the paper's Tables 1 and 2;
//! the anchor-and-extend detection scheme is described in its "Scoring
//! Potential Rhymes" section.
//!
//! This module is deliberately independent of `rhyme_highlight`'s
//! exact-key grouping — it scores and aligns syllables, and is unaware of
//! GTK, buffers, or character spans. Wiring it into live highlighting
//! (turning `RhymeSpan`s across many line pairs into colored groups) is a
//! separate step.

use cmudict_fast::{Stress as CmuStress, Symbol};
use std::ops::Range;

/// The 15 CMU vowel phonemes (stress stripped), in the row/column order of
/// Hirjee & Brown's Table 1: AA AE AH AO AW AY EH ER EY IH IY OW OY UH UW.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vowel {
    Aa,
    Ae,
    Ah,
    Ao,
    Aw,
    Ay,
    Eh,
    Er,
    Ey,
    Ih,
    Iy,
    Ow,
    Oy,
    Uh,
    Uw,
}

/// The 21 consonants that can appear in a coda (i.e. everything but the
/// glides/aspirate HH, W, Y, which never end an English syllable), in the
/// row/column order of Hirjee & Brown's Table 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consonant {
    B,
    Ch,
    D,
    Dh,
    F,
    G,
    Jh,
    K,
    L,
    M,
    N,
    Ng,
    P,
    R,
    S,
    Sh,
    T,
    Th,
    V,
    Z,
    Zh,
}

const N_VOWELS: usize = 15;
const N_CONSONANTS: usize = 21;

/// Log-odds scoring matrix for stressed vowels (Table 1). Symmetric;
/// `M[i][j] = ln(Pr[i,j|Rhyme] / Pr[i,j|Random])`.
#[rustfmt::skip]
const VOWEL_MATRIX: [[f32; N_VOWELS]; N_VOWELS] = [
    // AA    AE    AH    AO    AW    AY    EH    ER    EY    IH    IY    OW    OY    UH    UW
    [  2.3, -3.2, -0.8,  1.6, -1.7, -2.7, -7.2, -0.6, -3.9, -4.8, -3.9, -1.0, -1.7, -3.3, -3.9 ], // AA
    [ -3.2,  2.1, -1.5, -6.6, -1.9, -3.3, -1.5, -3.4, -1.8, -2.0, -4.3, -4.6, -4.5, -3.7, -6.7 ], // AE
    [ -0.8, -1.5,  2.2, -1.2, -1.4, -1.4, -0.6, -0.2, -1.7, -0.3, -3.0, -1.0, -0.6, -0.9, -1.5 ], // AH
    [  1.6, -6.6, -1.2,  3.1, -1.0, -3.8, -6.5, -1.1, -3.9, -4.2, -6.3, -0.3, -0.4,  1.1, -3.3 ], // AO
    [ -1.7, -1.9, -1.4, -1.0,  3.8, -0.3, -6.0, -4.2, -5.7, -6.0, -5.7, -2.0, -2.9, -4.5, -1.4 ], // AW
    [ -2.7, -3.3, -1.4, -3.8, -0.3,  2.5, -4.2, -1.1, -7.0, -1.8, -3.2, -4.3, -1.1, -5.7, -6.4 ], // AY
    [ -7.2, -1.5, -0.6, -6.5, -6.0, -4.2,  1.9, -1.2, -1.5,  0.2, -2.0, -7.0, -4.5, -6.1, -4.3 ], // EH
    [ -0.6, -3.4, -0.2, -1.1, -4.2, -1.1, -1.2,  3.9, -5.6, -1.5, -5.5, -1.6, -2.7, -1.3, -2.6 ], // ER
    [ -3.9, -1.8, -1.7, -3.9, -5.7, -7.0, -1.5, -5.6,  2.5, -3.4, -2.7, -4.4, -4.3, -5.8, -6.5 ], // EY
    [ -4.8, -2.0, -0.3, -4.2, -6.0, -1.8,  0.2, -1.5, -3.4,  2.0, -0.9, -7.1,  0.2, -2.2, -3.7 ], // IH
    [ -3.9, -4.3, -3.0, -6.3, -5.7, -3.2, -2.0, -5.5, -2.7, -0.9,  2.4, -4.4, -4.2, -5.8, -6.4 ], // IY
    [ -1.0, -4.6, -1.0, -0.3, -2.0, -4.3, -7.0, -1.6, -4.4, -7.1, -4.4,  2.8, -4.0, -2.5, -1.5 ], // OW
    [ -1.7, -4.5, -0.6, -0.4, -2.9, -1.1, -4.5, -2.7, -4.3,  0.2, -4.2, -4.0,  4.9,  0.1, -3.6 ], // OY
    [ -3.3, -3.7, -0.9,  1.1, -4.5, -5.7, -6.1, -1.3, -5.8, -2.2, -5.8, -2.5,  0.1,  2.6, -0.5 ], // UH
    [ -3.9, -6.7, -1.5, -3.3, -1.4, -6.4, -4.3, -2.6, -6.5, -3.7, -6.4, -1.5, -3.6, -0.5,  3.1 ], // UW
];

/// Log-odds scoring matrix for coda consonants (Table 2). Symmetric.
#[rustfmt::skip]
const CONSONANT_MATRIX: [[f32; N_CONSONANTS]; N_CONSONANTS] = [
    //  B     CH    D     DH    F     G     JH    K     L     M     N     NG    P     R     S     SH    T     TH    V     Z     ZH
    [  4.3, -4.8,  1.1,  0.4, -5.5,  1.9,  1.9, -6.9, -0.3, -0.5, -1.6, -5.5,  0.1, -0.9, -1.6, -4.6, -1.0, -4.3,  2.3,  0.3, -2.5 ], // B
    [ -4.8,  4.2, -1.6, -4.9, -0.3,  0.2,  0.4,  1.5, -6.8, -6.6, -2.8, -5.5,  1.1, -6.7,  0.3,  0.6,  0.9,  1.4, -6.1, -2.0, -2.5 ], // CH
    [  1.1, -1.6,  2.3, -7.0, -7.6,  0.1,  0.2, -3.1, -1.7, -2.2, -2.2, -3.0, -1.8, -0.9, -9.0, -2.1,  0.2,  0.0, -0.2,  0.0, -4.6 ], // D
    [  0.4, -4.9, -7.0,  3.5, -5.6, -5.1, -4.2, -0.4, -0.2, -2.0, -7.5, -5.6, -6.2, -1.4, -7.0, -4.8, -0.3,  1.3,  2.8,  1.1, -2.6 ], // DH
    [ -5.5, -0.3, -7.6, -5.6,  3.4, -1.2, -4.9, -0.3, -1.5, -1.3, -3.5, -1.6,  1.1, -2.7,  1.0,  1.2, -0.9,  4.0,  0.6, -7.3, -3.2 ], // F
    [  1.9,  0.2,  0.1, -5.1, -1.2,  4.1,  1.8,  0.0, -0.2, -1.0, -1.9, -5.7, -0.7, -0.8, -2.5, -4.9, -1.1, -4.5,  0.3, -0.3, -2.7 ], // G
    [  1.9,  0.4,  0.2, -4.2, -4.9,  1.8,  5.2, -6.3, -1.5,  0.1, -0.5, -4.8, -0.2, -0.3, -0.6,  0.6, -1.1, -3.6,  1.4,  1.0,  4.1 ], // JH
    [ -6.9,  1.5, -3.1, -0.4, -0.3,  0.0, -6.3,  2.6, -2.9, -2.1, -2.6, -1.3,  1.7, -2.1, -0.7, -0.6,  0.9,  0.5, -1.8, -3.1, -4.7 ], // K
    [ -0.3, -6.8, -1.7, -0.2, -1.5, -0.2, -1.5, -2.9,  2.8, -1.8, -1.8, -2.8, -8.1, -0.5, -2.9, -6.6, -2.9, -6.3, -1.3, -1.6, -4.5 ], // L
    [ -0.5, -6.6, -2.2, -2.0, -1.3, -1.0,  0.1, -2.1, -1.8,  2.7,  1.8,  0.7, -3.2, -1.2, -2.9, -1.1, -2.5,  0.4, -0.6, -3.7, -4.2 ], // M
    [ -1.6, -2.8, -2.2, -7.5, -3.5, -1.9, -0.5, -2.6, -1.8,  1.8,  2.2,  1.2, -2.5, -1.0, -2.3, -0.7, -1.5, -0.6, -1.5, -2.1, -5.1 ], // N
    [ -5.5, -5.5, -3.0, -5.6, -1.6, -5.7, -4.8, -1.3, -2.8,  0.7,  1.2,  4.1, -6.8, -2.7, -2.3, -5.3, -3.5, -5.0, -2.1, -2.0, -3.2 ], // NG
    [  0.1,  1.1, -1.8, -6.2,  1.1, -0.7, -0.2,  1.7, -8.1, -3.2, -2.5, -6.8,  3.3, -2.0, -1.1, -0.7,  1.1,  0.9, -0.5, -7.9, -3.8 ], // P
    [ -0.9, -6.7, -0.9, -1.4, -2.7, -0.8, -0.3, -2.1, -0.5, -1.2, -1.0, -2.7, -2.0,  2.8, -2.3, -0.8, -1.2, -6.1, -2.1, -2.2, -4.3 ], // R
    [ -1.6,  0.3, -9.0, -7.0,  1.0, -2.5, -0.6, -0.7, -2.9, -2.9, -2.3, -2.3, -1.1, -2.3,  2.6,  2.4, -1.0,  1.0, -2.4,  0.5,  0.0 ], // S
    [ -4.6,  0.6, -2.1, -4.8,  1.2, -4.9,  0.6, -0.6, -6.6, -1.1, -0.7, -5.3, -0.7, -0.8,  2.4,  5.2, -0.6, -4.1, -1.3, -0.2,  3.6 ], // SH
    [ -1.0,  0.9,  0.2, -0.3, -0.9, -1.1, -1.1,  0.9, -2.9, -2.5, -1.5, -3.5,  1.1, -1.2, -1.0, -0.6,  1.7,  1.6, -0.8, -9.2, -5.2 ], // T
    [ -4.3,  1.4,  0.0,  1.3,  4.0, -4.5, -3.6,  0.5, -6.3,  0.4, -0.6, -5.0,  0.9, -6.1,  1.0, -4.1,  1.6,  4.5,  0.5, -6.1, -2.0 ], // TH
    [  2.3, -6.1, -0.2,  2.8,  0.6,  0.3,  1.4, -1.8, -1.3, -0.6, -1.5, -2.1, -0.5, -2.1, -2.4, -1.3, -0.8,  0.5,  2.9, -0.4,  1.6 ], // V
    [  0.3, -2.0,  0.0,  1.1, -7.3, -0.3,  1.0, -3.1, -1.6, -3.7, -2.1, -2.0, -7.9, -2.2,  0.5, -0.2, -9.2, -6.1, -0.4,  2.6,  3.0 ], // Z
    [ -2.5, -2.5, -4.6, -2.6, -3.2, -2.7,  4.1, -4.7, -4.5, -4.2, -5.1, -3.2, -3.8, -4.3,  0.0,  3.6, -5.2, -2.0,  1.6,  3.0,  6.8 ], // ZH
];

/// Score for a consonant appearing unmatched at the *start* of a coda
/// (Table 2's `_*` column) — e.g. the /l/ in "mold" when it aligns against
/// "code"'s bare /d/.
#[rustfmt::skip]
const UNMATCHED_START: [f32; N_CONSONANTS] = [
    -0.6, -6.0, -0.2, -6.0, -1.4, -0.9, -5.3, -0.9,  0.4, -0.9, -0.4,  0.2, -0.7,  1.7,  0.5, -5.8,  0.0, -5.4, -1.2, -1.3, -3.7,
];

/// Score for a consonant appearing unmatched at the *end* of a coda
/// (Table 2's `*_` column) — e.g. plural/past-tense endings like the /d/ in
/// "capped" against "trap"'s bare /p/.
#[rustfmt::skip]
const UNMATCHED_END: [f32; N_CONSONANTS] = [
    -1.5, -2.6,  1.2, -3.4, -2.9, -2.8,  0.5, -1.8, -1.0, -1.7, -2.3, -3.9, -0.8, -0.7,  0.6, -7.7,  0.7, -0.6, -1.7,  1.1, -5.6,
];

pub fn vowel_score(a: Vowel, b: Vowel) -> f32 {
    VOWEL_MATRIX[a as usize][b as usize]
}

fn consonant_pair_score(a: Consonant, b: Consonant) -> f32 {
    CONSONANT_MATRIX[a as usize][b as usize]
}

fn unmatched_start_score(c: Consonant) -> f32 {
    UNMATCHED_START[c as usize]
}

fn unmatched_end_score(c: Consonant) -> f32 {
    UNMATCHED_END[c as usize]
}

/// Global alignment of two syllable codas. When they're the same length,
/// consonants line up position by position. When they differ, the extra
/// consonants in the longer one are treated as unmatched at either the
/// start or the end of the cluster — never internally, since English
/// syllable codas are short enough that trying every start/end split and
/// keeping the best is both cheap and matches the paper's worked examples
/// (e.g. "mold" vs "code": the /l/ unmatched at the coda's start, /d/:/d/
/// matched).
pub fn align_coda(a: &[Consonant], b: &[Consonant]) -> f32 {
    let (longer, shorter) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    let diff = longer.len() - shorter.len();

    if diff == 0 {
        return longer
            .iter()
            .zip(shorter)
            .map(|(&x, &y)| consonant_pair_score(x, y))
            .sum();
    }

    (0..=diff)
        .map(|prefix_del| {
            let suffix_del = diff - prefix_del;
            let core = &longer[prefix_del..longer.len() - suffix_del];
            let prefix_score: f32 = longer[..prefix_del]
                .iter()
                .map(|&c| unmatched_start_score(c))
                .sum();
            let suffix_score: f32 = longer[longer.len() - suffix_del..]
                .iter()
                .map(|&c| unmatched_end_score(c))
                .sum();
            let core_score: f32 = core
                .iter()
                .zip(shorter)
                .map(|(&x, &y)| consonant_pair_score(x, y))
                .sum();
            prefix_score + suffix_score + core_score
        })
        .fold(f32::NEG_INFINITY, f32::max)
}

/// Metrical stress: 0 = none, 1 = primary, 2 = secondary (mirrors CMUdict's
/// stress digits).
pub type StressLevel = u8;

/// Only two points of this matrix are confirmed by the paper's worked
/// example: matched primary stress scores 1.0, and matched non-primary
/// stress scores 0.0. The paper doesn't publish a full stress matrix in the
/// text we have, so mismatches and secondary-stress combinations here are
/// reasonable placeholders, not sourced values — revisit if the real
/// numbers turn up.
pub fn stress_score(a: StressLevel, b: StressLevel) -> f32 {
    match (a, b) {
        (1, 1) => 1.0,
        (0, 0) | (2, 2) => 0.0,
        (1, _) | (_, 1) => -1.0,
        _ => -0.2,
    }
}

/// A syllable reduced to the features that matter for rhyme: nucleus,
/// stress, and coda. Onset consonants are never included — they don't
/// participate in rhyme.
#[derive(Debug, Clone)]
pub struct Syllable {
    pub vowel: Vowel,
    pub stress: StressLevel,
    pub coda: Vec<Consonant>,
}

/// The combined log-odds score for two syllables: vowel score, plus the
/// coda alignment score normalized by the longer coda's length (so a
/// syllable with a long coda doesn't automatically dominate one with a
/// short coda — "win"/"gin" should rhyme about as well as "splints"/"mints"),
/// plus the stress score.
pub fn syllable_score(a: &Syllable, b: &Syllable) -> f32 {
    let coda_len = a.coda.len().max(b.coda.len());
    let coda_score = if coda_len == 0 {
        0.0
    } else {
        align_coda(&a.coda, &b.coda) / coda_len as f32
    };
    vowel_score(a.vowel, b.vowel) + coda_score + stress_score(a.stress, b.stress)
}

fn stress_of(stress: &CmuStress) -> StressLevel {
    match stress {
        CmuStress::None => 0,
        CmuStress::Primary => 1,
        CmuStress::Secondary => 2,
    }
}

fn vowel_from_symbol(symbol: &Symbol) -> Option<(Vowel, StressLevel)> {
    use Symbol::*;
    Some(match symbol {
        AA(s) => (Vowel::Aa, stress_of(s)),
        AE(s) => (Vowel::Ae, stress_of(s)),
        AH(s) => (Vowel::Ah, stress_of(s)),
        AO(s) => (Vowel::Ao, stress_of(s)),
        AW(s) => (Vowel::Aw, stress_of(s)),
        AY(s) => (Vowel::Ay, stress_of(s)),
        EH(s) => (Vowel::Eh, stress_of(s)),
        ER(s) => (Vowel::Er, stress_of(s)),
        EY(s) => (Vowel::Ey, stress_of(s)),
        IH(s) => (Vowel::Ih, stress_of(s)),
        IY(s) => (Vowel::Iy, stress_of(s)),
        OW(s) => (Vowel::Ow, stress_of(s)),
        OY(s) => (Vowel::Oy, stress_of(s)),
        UH(s) => (Vowel::Uh, stress_of(s)),
        UW(s) => (Vowel::Uw, stress_of(s)),
        _ => return None,
    })
}

fn consonant_from_symbol(symbol: &Symbol) -> Option<Consonant> {
    use Symbol::*;
    Some(match symbol {
        B => Consonant::B,
        CH => Consonant::Ch,
        D => Consonant::D,
        DH => Consonant::Dh,
        F => Consonant::F,
        G => Consonant::G,
        JH => Consonant::Jh,
        K => Consonant::K,
        L => Consonant::L,
        M => Consonant::M,
        N => Consonant::N,
        NG => Consonant::Ng,
        P => Consonant::P,
        R => Consonant::R,
        S => Consonant::S,
        SH => Consonant::Sh,
        T => Consonant::T,
        TH => Consonant::Th,
        V => Consonant::V,
        Z => Consonant::Z,
        ZH => Consonant::Zh,
        // HH, W, Y never end an English syllable, so they never appear in
        // a coda — dropped rather than mapped.
        _ => return None,
    })
}

/// Split one word's CMU pronunciation into `Syllable`s, applying Hirjee &
/// Brown's coda rule: for an internal consonant run *within* the word
/// (between two of its own vowels), the coda takes the first half (rounded
/// up) and the rest becomes the next syllable's onset (dropped). A word's
/// *final* syllable has no next vowel to split against, so it keeps every
/// trailing consonant — e.g. "mold" (M OW1 L D) keeps the full /ld/ coda,
/// while "breaking" (B R EY1 K IH0 NG) splits its single internal /k/
/// entirely onto the first syllable (ceil(1/2) = 1) and nothing carries
/// over. This matches the paper's own worked example exactly (see the
/// `paper_worked_example` test below).
pub fn syllables_from_pronunciation(phonemes: &[Symbol]) -> Vec<Syllable> {
    let vowel_positions: Vec<usize> = phonemes
        .iter()
        .enumerate()
        .filter(|(_, s)| s.is_syllable())
        .map(|(i, _)| i)
        .collect();

    vowel_positions
        .iter()
        .enumerate()
        .filter_map(|(vi, &pos)| {
            let (vowel, stress) = vowel_from_symbol(&phonemes[pos])?;
            let is_last_in_word = vi + 1 == vowel_positions.len();
            let next_vowel = vowel_positions
                .get(vi + 1)
                .copied()
                .unwrap_or(phonemes.len());
            let run = &phonemes[pos + 1..next_vowel];
            let coda_len = if is_last_in_word {
                run.len()
            } else {
                run.len().div_ceil(2)
            };
            let coda = run[..coda_len]
                .iter()
                .filter_map(consonant_from_symbol)
                .collect();
            Some(Syllable {
                vowel,
                stress,
                coda,
            })
        })
        .collect()
}

/// A detected rhyme between a span of syllables in `a` and a span in `b`
/// (indices into whatever syllable sequences were passed to
/// [`find_rhymes`]), with its total log-odds score.
#[derive(Debug, Clone, PartialEq)]
pub struct RhymeSpan {
    pub a: Range<usize>,
    pub b: Range<usize>,
    pub score: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    /// Minimum score for a syllable pair to seed a rhyme ("anchor" in the
    /// paper's terms).
    pub anchor: f32,
    /// Minimum length-normalized running score to keep extending forward
    /// from an established anchor. Deliberately looser than `anchor`: once
    /// a real rhyme has been found, a single weak or unstressed trailing
    /// syllable (e.g. the "-y" tail of "legacy") shouldn't be enough to cut
    /// the match short — `extend`'s own best-score tracking already
    /// discards a detour that doesn't lead anywhere better, so being
    /// lenient here only helps it see past a temporary dip to a real
    /// continuation, it doesn't add noise on its own.
    pub extend: f32,
    /// Minimum score an anchor-quality pair must have, after skipping 1-2
    /// non-rhyming syllables, to justify the "jump" (handles mosaic rhymes
    /// with an occasional non-rhyming syllable in the middle).
    pub jump: f32,
}

impl Default for Thresholds {
    fn default() -> Self {
        // 1.5 is the threshold Hirjee & Brown validate for line-final
        // rhymes (matched syllables ~e^1.5, about 4.5x, more likely in a
        // rhyme than by chance). The jump threshold is set higher so a
        // skip still needs a strong anchor on the far side, not just an
        // ordinary one.
        Self {
            anchor: 1.5,
            extend: 0.0,
            jump: 2.5,
        }
    }
}

/// Find rhyming syllable spans between two syllable sequences (typically
/// the current and a preceding line, concatenated in reading order across
/// their non-stopword words). This is anchor-and-extend, not a full
/// Smith-Waterman alignment: seed at syllable pairs that score above
/// `thresholds.anchor` and involve a stressed or line-final syllable, then
/// extend forward while the length-normalized running score stays above
/// `thresholds.extend`, with an allowance for jumping over 1-2 non-rhyming
/// syllables when the syllable pair just past them is itself a strong
/// anchor.
///
/// Unlike the paper, this doesn't discard one-syllable matches on an
/// unstressed syllable — Hirjee & Brown drop those because, for corpus
/// analysis, an occasional accidental unstressed match is noise; rhymr is a
/// writing aid instead, where missing a real rhyme (e.g. many rap bars
/// intentionally closing on an unstressed "-y"/"-ly" syllable) costs more
/// than showing one the writer didn't intend. The caller is expected to
/// apply its own precision gate on top of this — see `MERGE_THRESHOLD` in
/// `rhyme/highlight.rs`.
pub fn find_rhymes(a: &[Syllable], b: &[Syllable], thresholds: &Thresholds) -> Vec<RhymeSpan> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }

    let scores: Vec<Vec<f32>> = a
        .iter()
        .map(|sa| b.iter().map(|sb| syllable_score(sa, sb)).collect())
        .collect();

    let is_anchor = |i: usize, j: usize| {
        scores[i][j] > thresholds.anchor
            && (a[i].stress == 1 || b[j].stress == 1 || i == a.len() - 1 || j == b.len() - 1)
    };

    let mut spans = Vec::new();
    for i in 0..a.len() {
        for j in 0..b.len() {
            if is_anchor(i, j) {
                spans.push(extend(&scores, a, b, i, j, thresholds));
            }
        }
    }

    dedupe_subsumed(spans)
}

fn extend(
    scores: &[Vec<f32>],
    a: &[Syllable],
    b: &[Syllable],
    i0: usize,
    j0: usize,
    thresholds: &Thresholds,
) -> RhymeSpan {
    let mut total = scores[i0][j0];
    let mut end_i = i0 + 1;
    let mut end_j = j0 + 1;
    let mut best = (total, end_i, end_j);

    loop {
        if end_i < a.len() && end_j < b.len() {
            let candidate_total = total + scores[end_i][end_j];
            let candidate_span = (end_i + 1 - i0) as f32;
            if candidate_total / candidate_span >= thresholds.extend {
                total = candidate_total;
                end_i += 1;
                end_j += 1;
                if total > best.0 {
                    best = (total, end_i, end_j);
                }
                continue;
            }
        }

        let mut jumped = false;
        for skip in 1..=2usize {
            let ni = end_i + skip;
            let nj = end_j + skip;
            if ni >= a.len() || nj >= b.len() {
                continue;
            }
            if scores[ni][nj] > thresholds.jump {
                total += scores[ni][nj];
                end_i = ni + 1;
                end_j = nj + 1;
                if total > best.0 {
                    best = (total, end_i, end_j);
                }
                jumped = true;
                break;
            }
        }
        if !jumped {
            break;
        }
    }

    RhymeSpan {
        a: i0..best.1,
        b: j0..best.2,
        score: best.0,
    }
}

/// Drop any span whose syllable range (on both sides) is fully contained in
/// another kept span — a cheap approximation of the paper's "consolidate
/// consecutive and overlapping rhymes together" step.
fn dedupe_subsumed(mut spans: Vec<RhymeSpan>) -> Vec<RhymeSpan> {
    spans.sort_by(|x, y| {
        x.a.start
            .cmp(&y.a.start)
            .then(x.b.start.cmp(&y.b.start))
            .then(y.score.partial_cmp(&x.score).unwrap())
    });

    let mut kept: Vec<RhymeSpan> = Vec::new();
    for span in spans {
        let subsumed = kept.iter().any(|k| {
            k.a.start <= span.a.start
                && span.a.end <= k.a.end
                && k.b.start <= span.b.start
                && span.b.end <= k.b.end
        });
        if !subsumed {
            kept.push(span);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmudict_fast::Stress::{None as NoStress, Primary};

    const EPS: f32 = 1e-4;

    fn approx(a: f32, b: f32) {
        assert!((a - b).abs() < EPS, "expected {b}, got {a}");
    }

    #[test]
    fn matrix_spot_values_match_the_paper() {
        // Every one of these is cited by name in the paper's text or its
        // worked example, independent of the worked example's own totals.
        approx(vowel_score(Vowel::Ao, Vowel::Aa), 1.6); // "the score of 1.6 for /ɔ/ and /ɑ/"
        approx(vowel_score(Vowel::Ey, Vowel::Eh), -1.5);
        approx(vowel_score(Vowel::Ih, Vowel::Ih), 2.0);
        approx(vowel_score(Vowel::Ah, Vowel::Ay), -1.4);
        approx(vowel_score(Vowel::Ow, Vowel::Ow), 2.8);
        approx(consonant_pair_score(Consonant::K, Consonant::K), 2.6);
        approx(consonant_pair_score(Consonant::N, Consonant::N), 2.2);
        approx(consonant_pair_score(Consonant::D, Consonant::D), 2.3);
        approx(unmatched_start_score(Consonant::L), 0.4);
    }

    #[test]
    fn align_coda_matches_the_mold_code_worked_example() {
        // "mold" (coda /ld/) vs "code" (coda /d/): the /l/ unmatched at the
        // coda's start, /d/:/d/ matched — paper states this as (0.4+2.3).
        let score = align_coda(&[Consonant::L, Consonant::D], &[Consonant::D]);
        approx(score, 0.4 + 2.3);
    }

    #[test]
    fn syllables_from_pronunciation_splits_internal_runs_but_not_word_final_ones() {
        // "breaking": B R EY1 K IH0 NG — one vowel-internal consonant (K)
        // between EY and IH; ceil(1/2) = 1 keeps it whole on syllable one,
        // nothing carries over to syllable two.
        let breaking = [
            Symbol::B,
            Symbol::R,
            Symbol::EY(Primary),
            Symbol::K,
            Symbol::IH(NoStress),
            Symbol::NG,
        ];
        let syls = syllables_from_pronunciation(&breaking);
        assert_eq!(syls.len(), 2);
        assert_eq!(syls[0].coda, vec![Consonant::K]);
        // Word-final syllable: the trailing NG isn't split against
        // anything (there's no next vowel left in the word), so it stays
        // whole rather than being halved down to nothing.
        assert_eq!(syls[1].coda, vec![Consonant::Ng]);

        // "mold": M OW1 L D — final syllable, so both trailing consonants
        // stay together as one coda instead of being split in half.
        let mold = [Symbol::M, Symbol::OW(Primary), Symbol::L, Symbol::D];
        let syls = syllables_from_pronunciation(&mold);
        assert_eq!(syls.len(), 1);
        assert_eq!(syls[0].coda, vec![Consonant::L, Consonant::D]);
    }

    /// Reproduces Hirjee & Brown's own worked example: "breakin' the mold"
    /// rhyming with "checkin' my code" (their Table 1/2 discussion). Every
    /// individual coefficient they cite is checked here exactly — the
    /// paper additionally claims the four syllables' totals are 2.1, 4.2,
    /// -1.4, 4.1, summing to 9.0. The first three match exactly, but the
    /// fourth doesn't follow from their own displayed formula: they show
    /// "2.8 + (0.4+2.3)/2 + 1.0" for it, which is 2.8 + 1.35 + 1.0 = 5.15,
    /// not 4.1 (and 2.1+4.2-1.4+5.15 = 10.05, not 9.0). Every coefficient
    /// in that sum is independently confirmed elsewhere in this test file,
    /// so this looks like an arithmetic slip in the source paper rather
    /// than a modeling difference — this test asserts the value that
    /// actually follows from the paper's own stated coefficients.
    #[test]
    fn paper_worked_example() {
        // "the" and "my" here are reduced to no stress, matching the
        // paper's note that ~30 common minor words ("a", "I", "and", ...)
        // have their stress reduced in their augmented dictionary; a plain
        // CMUdict lookup would give "my" primary stress.
        let breakin = syllables_from_pronunciation(&[
            Symbol::B,
            Symbol::R,
            Symbol::EY(Primary),
            Symbol::K,
            Symbol::IH(NoStress),
            Symbol::N,
        ]);
        let the = syllables_from_pronunciation(&[Symbol::DH, Symbol::AH(NoStress)]);
        let mold =
            syllables_from_pronunciation(&[Symbol::M, Symbol::OW(Primary), Symbol::L, Symbol::D]);

        let checkin = syllables_from_pronunciation(&[
            Symbol::CH,
            Symbol::EH(Primary),
            Symbol::K,
            Symbol::IH(NoStress),
            Symbol::N,
        ]);
        let my = syllables_from_pronunciation(&[Symbol::M, Symbol::AY(NoStress)]);
        let code = syllables_from_pronunciation(&[Symbol::K, Symbol::OW(Primary), Symbol::D]);

        let line_a: Vec<Syllable> = breakin.into_iter().chain(the).chain(mold).collect();
        let line_b: Vec<Syllable> = checkin.into_iter().chain(my).chain(code).collect();
        assert_eq!(line_a.len(), 4);
        assert_eq!(line_b.len(), 4);

        let per_syllable: Vec<f32> = line_a
            .iter()
            .zip(&line_b)
            .map(|(x, y)| syllable_score(x, y))
            .collect();
        approx(per_syllable[0], 2.1);
        approx(per_syllable[1], 4.2);
        approx(per_syllable[2], -1.4);
        approx(per_syllable[3], 2.8 + (0.4 + 2.3) / 2.0 + 1.0); // 5.15 — see doc comment above

        let total: f32 = per_syllable.iter().sum();
        approx(total, 2.1 + 4.2 - 1.4 + (2.8 + (0.4 + 2.3) / 2.0 + 1.0));

        // Regardless of the paper's own subtotal, this is comfortably a
        // detected rhyme: an anchor-and-extend pass over the two full
        // 4-syllable sequences should find the whole span.
        let spans = find_rhymes(&line_a, &line_b, &Thresholds::default());
        assert!(
            spans.iter().any(|s| s.a == (0..4) && s.b == (0..4)),
            "{spans:?}"
        );
    }

    #[test]
    fn find_rhymes_extends_a_mosaic_rhyme_across_a_non_rhyming_syllable() {
        // A synthetic case shaped like the paper's "them clips copped" /
        // "with ziplocks of" example: three strongly-rhyming syllable pairs
        // with one weak pair in the middle that a plain forward extension
        // alone wouldn't survive, but the jump extension should bridge.
        let strong = |v: Vowel, coda: Vec<Consonant>| Syllable {
            vowel: v,
            stress: 1,
            coda,
        };
        let weak_a = Syllable {
            vowel: Vowel::Eh,
            stress: 0,
            coda: vec![],
        };
        let weak_b = Syllable {
            vowel: Vowel::Uw,
            stress: 0,
            coda: vec![],
        };

        let line_a = vec![
            strong(Vowel::Ae, vec![Consonant::T]),
            weak_a,
            strong(Vowel::Aa, vec![Consonant::K]),
        ];
        let line_b = vec![
            strong(Vowel::Ae, vec![Consonant::T]),
            weak_b,
            strong(Vowel::Aa, vec![Consonant::K]),
        ];

        let spans = find_rhymes(&line_a, &line_b, &Thresholds::default());
        assert!(
            spans.iter().any(|s| s.a == (0..3) && s.b == (0..3)),
            "{spans:?}"
        );
    }

    #[test]
    fn find_rhymes_on_empty_input_returns_no_spans() {
        assert!(find_rhymes(&[], &[], &Thresholds::default()).is_empty());
    }
}
