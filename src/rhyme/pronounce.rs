//! The single pronunciation-resolution path for rhyme highlighting.
//!
//! Every consumer — rhyme-unit keying, the per-line syllable streams the
//! Hirjee & Brown scorer reads, and the in-/out-of-dictionary routing in
//! `highlight::compute_groups` — asks here instead of reaching for
//! [`Cmudict`] directly, so lookup order and the out-of-dictionary fallback
//! live in exactly one place. Grapheme-to-phoneme for unknown words (issue
//! #21) registers as the final layer here, not as another `if let None`
//! branch at a call site.

use cmudict_fast::{Cmudict, Symbol};
use std::str::FromStr;
use std::sync::OnceLock;

/// The bundled CMU Pronouncing Dictionary, in `.dict` text form. Also the
/// word source for the syllable regression suite.
pub(crate) const CMUDICT_TXT: &str = include_str!("../../assets/dictionary/cmudict.dict");

/// The parsed dictionary, built once on first use. Parsing embedded text
/// that shipped with the binary is a genuine invariant — it cannot vary at
/// runtime — so a failure here is a build defect, not an input error.
fn cmudict() -> &'static Cmudict {
    static DICT: OnceLock<Cmudict> = OnceLock::new();
    DICT.get_or_init(|| Cmudict::from_str(CMUDICT_TXT).expect("bundled cmudict.dict should parse"))
}

/// Where a resolved pronunciation came from. Only [`Source::Cmudict`] exists
/// today; grapheme-to-phoneme (#21) will add a variant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// A CMU Pronouncing Dictionary entry.
    Cmudict,
}

/// One pronunciation of a word: its ARPABET phoneme sequence (with CMUdict
/// stress digits intact) and where it came from.
#[derive(Clone, Debug)]
pub struct Pronunciation {
    /// The phoneme sequence, e.g. `[K, AE1, T]` for "cat".
    pub phonemes: Vec<Symbol>,
    /// Which layer produced this pronunciation.
    pub source: Source,
}

/// Every known pronunciation of `word`, most canonical first — more than one
/// for heteronyms and recorded variants ("read", "the", "aluminium"). Empty
/// when the word is out of dictionary; the caller then falls back to the
/// orthographic exact-key path in `highlight::rhyme_units` (until #21 adds a
/// G2P layer here). `word` is lower-cased internally.
pub fn resolve(word: &str) -> Vec<Pronunciation> {
    match cmudict().get(&word.to_lowercase()) {
        Some(rules) => rules
            .iter()
            .map(|rule| Pronunciation {
                phonemes: rule.pronunciation().to_vec(),
                source: Source::Cmudict,
            })
            .collect(),
        None => Vec::new(),
    }
}

/// The first (most canonical) pronunciation of `word`, or `None` when it is
/// out of dictionary. The behaviour-preserving replacement for the old
/// `cmudict().get(word).and_then(|rules| rules.first())` at each call site;
/// stage 4 of the epic switches the scored path to [`resolve`] instead, to
/// stop mis-resolving heteronyms.
pub fn resolve_first(word: &str) -> Option<Pronunciation> {
    resolve(word).into_iter().next()
}

/// Whether `word` has a CMU dictionary entry. `highlight::compute_groups`
/// uses this to route unknown words down the orthographic exact-key
/// fallback instead of the Hirjee & Brown scored path.
pub fn is_in_dictionary(word: &str) -> bool {
    cmudict().get(&word.to_lowercase()).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_first_matches_the_legacy_cmudict_first_rule() {
        let pron = resolve_first("cat").expect("cat is in cmudict");
        assert_eq!(pron.source, Source::Cmudict);
        // Same sequence the old `cmudict().get("cat").unwrap().first()` gave.
        let legacy = cmudict()
            .get("cat")
            .and_then(|rules| rules.first())
            .map(|rule| rule.pronunciation().to_vec())
            .expect("cat is in cmudict");
        assert_eq!(pron.phonemes, legacy);
    }

    #[test]
    fn resolve_is_empty_for_out_of_dictionary_words() {
        assert!(resolve("xyzzyplonk").is_empty());
        assert!(resolve_first("xyzzyplonk").is_none());
        assert!(!is_in_dictionary("xyzzyplonk"));
    }

    #[test]
    fn heteronyms_resolve_to_more_than_one_pronunciation() {
        // "read" has /riːd/ and /rɛd/; the scored path (stage 4) will pick
        // between them, but the resolver must surface both.
        assert!(resolve("read").len() >= 2);
    }
}
