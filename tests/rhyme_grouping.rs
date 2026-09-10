//! Rhyme-grouping accuracy harness.
//!
//! Runs the app's pure grouping pipeline
//! ([`rhymr_rs::rhyme::highlight::group_spans`]) over every fixture in
//! `tests/fixtures/rhyme/`, scores it against the fixture's hand annotations
//! as pairwise precision / recall / F1, and locks the raw grouping output
//! against `tests/fixtures/rhyme_baseline.json` so later accuracy stages
//! surface a mechanical diff.
//!
//! This is additive — it does not touch the inline `~19k`-word syllable
//! regression suite in `src/rhyme/`. See epic issue #25, stage 1.
//!
//! Regenerate the baseline after an intentional grouping change:
//! `UPDATE_RHYME_BASELINE=1 cargo test --test rhyme_grouping`.

use rhymr_rs::rhyme::highlight::{RhymeTuning, group_spans};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Minimum acceptable micro-averaged F1 across all fixtures. Measured at
/// 0.70 when the harness landed ("1/10" — poor recall, over-merging); the
/// floor sits just under that and is ratcheted up as each accuracy stage
/// lands. A drop below it fails local runs.
const MIN_AGGREGATE_F1: f64 = 0.68;

/// `Settings::rhyme_stop_at_blank_line` default.
const STOP_AT_BLANK_LINE: bool = true;

// ---------------------------------------------------------------------------
// Fixture model
// ---------------------------------------------------------------------------

struct Fixture {
    name: String,
    body: String,
    /// Every annotated word → its group label. First-occurrence match.
    gold: BTreeMap<String, String>,
    /// `# nogroup:` sets — no two words in a set may share a computed group.
    nogroups: Vec<Vec<String>>,
}

fn parse_fixture(path: &Path, text: &str) -> Fixture {
    let mut name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("fixture")
        .to_string();
    let mut gold: BTreeMap<String, String> = BTreeMap::new();
    let mut nogroups: Vec<Vec<String>> = Vec::new();

    let mut lines = text.lines().peekable();
    while let Some(line) = lines.peek() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('#') {
            break;
        }
        let directive = trimmed.trim_start_matches('#').trim();
        if let Some(rest) = directive.strip_prefix("name:") {
            name = rest.trim().to_string();
        } else if let Some(rest) = directive.strip_prefix("group ") {
            let (label, words) = rest.split_once(':').unwrap_or_else(|| {
                panic!("{name}: malformed `# group` line: {directive:?}");
            });
            let label = label.trim().to_string();
            for word in words.split_whitespace() {
                let word = word.to_lowercase();
                if let Some(prev) = gold.insert(word.clone(), label.clone()) {
                    assert_eq!(
                        prev, label,
                        "{name}: word {word:?} annotated in two groups ({prev} and {label})"
                    );
                }
            }
        } else if let Some(rest) = directive.strip_prefix("nogroup:") {
            let set: Vec<String> = rest.split_whitespace().map(|w| w.to_lowercase()).collect();
            assert!(
                set.len() >= 2,
                "{name}: `# nogroup` needs at least two words"
            );
            nogroups.push(set);
        } else {
            panic!("{name}: unknown annotation directive: {directive:?}");
        }
        lines.next();
    }

    // Skip any blank separator lines between the header and the verse.
    let body: String = lines.collect::<Vec<_>>().join("\n");
    let body = body.trim_start_matches('\n').to_string();

    Fixture {
        name,
        body,
        gold,
        nogroups,
    }
}

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

fn is_word_char(c: char) -> bool {
    c.is_alphabetic() || c == '\''
}

/// The lower-cased word the character at `offset` sits in, or `None` when
/// `offset` is not inside a word. Mirrors `highlight::word_at_str`.
fn word_at(chars: &[char], offset: usize) -> Option<String> {
    if offset >= chars.len() || !is_word_char(chars[offset]) {
        return None;
    }
    let mut start = offset;
    while start > 0 && is_word_char(chars[start - 1]) {
        start -= 1;
    }
    let mut end = offset + 1;
    while end < chars.len() && is_word_char(chars[end]) {
        end += 1;
    }
    let word: String = chars[start..end].iter().collect();
    let word = word.trim_matches('\'').to_lowercase();
    (!word.is_empty()).then_some(word)
}

/// Per-word membership: word → the set of computed group indices it appears
/// in (a word can land in more than one group via different syllables).
fn membership(body: &str, groups: &[Vec<(usize, usize)>]) -> BTreeMap<String, BTreeSet<usize>> {
    let chars: Vec<char> = body.chars().collect();
    let mut map: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    for (idx, spans) in groups.iter().enumerate() {
        for &(start, _) in spans {
            if let Some(word) = word_at(&chars, start) {
                map.entry(word).or_default().insert(idx);
            }
        }
    }
    map
}

fn share_group(membership: &BTreeMap<String, BTreeSet<usize>>, a: &str, b: &str) -> bool {
    match (membership.get(a), membership.get(b)) {
        (Some(ga), Some(gb)) => !ga.is_disjoint(gb),
        _ => false,
    }
}

#[derive(Default)]
struct Counts {
    tp: usize,
    fp: usize,
    fn_: usize,
}

impl Counts {
    fn precision(&self) -> f64 {
        if self.tp + self.fp == 0 {
            1.0
        } else {
            self.tp as f64 / (self.tp + self.fp) as f64
        }
    }
    fn recall(&self) -> f64 {
        if self.tp + self.fn_ == 0 {
            1.0
        } else {
            self.tp as f64 / (self.tp + self.fn_) as f64
        }
    }
    fn f1(&self) -> f64 {
        let (p, r) = (self.precision(), self.recall());
        if p + r == 0.0 {
            0.0
        } else {
            2.0 * p * r / (p + r)
        }
    }
}

struct FixtureResult {
    name: String,
    counts: Counts,
    nogroup_violations: Vec<(String, String)>,
    /// Raw grouping output for the baseline lock: each group as its sorted
    /// distinct words, groups sorted.
    word_groups: Vec<Vec<String>>,
}

fn score(fixture: &Fixture) -> FixtureResult {
    let groups = group_spans(&fixture.body, STOP_AT_BLANK_LINE, RhymeTuning::default());
    let membership = membership(&fixture.body, &groups);

    // Precision/recall universe = the annotated words, which must all appear.
    let universe: Vec<&String> = fixture.gold.keys().collect();
    for word in &universe {
        assert!(
            membership_word_seen(&fixture.body, word),
            "{}: annotated word {word:?} does not occur in the verse",
            fixture.name
        );
    }

    let mut counts = Counts::default();
    for i in 0..universe.len() {
        for j in (i + 1)..universe.len() {
            let (a, b) = (universe[i], universe[j]);
            let gold_pos = fixture.gold[a] == fixture.gold[b];
            let comp_pos = share_group(&membership, a, b);
            match (gold_pos, comp_pos) {
                (true, true) => counts.tp += 1,
                (false, true) => counts.fp += 1,
                (true, false) => counts.fn_ += 1,
                (false, false) => {}
            }
        }
    }

    let mut nogroup_violations = Vec::new();
    for set in &fixture.nogroups {
        for i in 0..set.len() {
            for j in (i + 1)..set.len() {
                if share_group(&membership, &set[i], &set[j]) {
                    nogroup_violations.push((set[i].clone(), set[j].clone()));
                }
            }
        }
    }

    let chars: Vec<char> = fixture.body.chars().collect();
    let mut word_groups: Vec<Vec<String>> = groups
        .iter()
        .map(|spans| {
            let mut words: Vec<String> = spans
                .iter()
                .filter_map(|&(start, _)| word_at(&chars, start))
                .collect();
            words.sort();
            words.dedup();
            words
        })
        .filter(|words| !words.is_empty())
        .collect();
    word_groups.sort();

    FixtureResult {
        name: fixture.name.clone(),
        counts,
        nogroup_violations,
        word_groups,
    }
}

fn membership_word_seen(body: &str, word: &str) -> bool {
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if is_word_char(chars[i]) {
            let start = i;
            while i < chars.len() && is_word_char(chars[i]) {
                i += 1;
            }
            let token: String = chars[start..i].iter().collect();
            if token.trim_matches('\'').to_lowercase() == word {
                return true;
            }
        } else {
            i += 1;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Baseline lock
// ---------------------------------------------------------------------------

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rhyme")
}

fn baseline_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rhyme_baseline.json")
}

fn load_fixtures() -> Vec<Fixture> {
    let dir = fixtures_dir();
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("txt"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no .txt fixtures in {}", dir.display());
    paths
        .iter()
        .map(|p| {
            let text =
                fs::read_to_string(p).unwrap_or_else(|e| panic!("reading {}: {e}", p.display()));
            parse_fixture(p, &text)
        })
        .collect()
}

#[test]
fn rhyme_grouping_accuracy() {
    let fixtures = load_fixtures();
    let results: Vec<FixtureResult> = fixtures.iter().map(score).collect();

    println!(
        "\n{:<32} {:>6} {:>6} {:>6}   nogroup",
        "fixture", "prec", "rec", "f1"
    );
    println!("{}", "-".repeat(68));
    let mut agg = Counts::default();
    let mut violations = 0usize;
    for r in &results {
        println!(
            "{:<32} {:>6.2} {:>6.2} {:>6.2}   {}",
            r.name,
            r.counts.precision(),
            r.counts.recall(),
            r.counts.f1(),
            if r.nogroup_violations.is_empty() {
                "ok".to_string()
            } else {
                format!("{:?}", r.nogroup_violations)
            }
        );
        agg.tp += r.counts.tp;
        agg.fp += r.counts.fp;
        agg.fn_ += r.counts.fn_;
        violations += r.nogroup_violations.len();
    }
    println!("{}", "-".repeat(68));
    println!(
        "{:<32} {:>6.2} {:>6.2} {:>6.2}   {} violation(s)\n",
        "AGGREGATE (micro)",
        agg.precision(),
        agg.recall(),
        agg.f1(),
        violations
    );

    // Baseline lock -------------------------------------------------------
    let current: BTreeMap<String, Vec<Vec<String>>> = results
        .iter()
        .map(|r| (r.name.clone(), r.word_groups.clone()))
        .collect();
    let current_json = serde_json::to_string_pretty(&current).expect("serialize baseline");
    let path = baseline_path();
    let update = std::env::var_os("UPDATE_RHYME_BASELINE").is_some();
    if update || !path.exists() {
        fs::write(&path, format!("{current_json}\n")).expect("write baseline");
        println!("baseline written to {}", path.display());
    } else {
        let expected = fs::read_to_string(&path).expect("read baseline");
        let expected: BTreeMap<String, Vec<Vec<String>>> =
            serde_json::from_str(&expected).expect("parse baseline");
        if expected != current {
            for name in current
                .keys()
                .chain(expected.keys())
                .collect::<BTreeSet<_>>()
            {
                let (e, c) = (expected.get(name), current.get(name));
                if e != c {
                    println!("baseline drift in {name}:\n  was: {e:?}\n  now: {c:?}");
                }
            }
            panic!(
                "grouping output changed vs {}. If intentional, rerun with \
                 UPDATE_RHYME_BASELINE=1 and show the diff in the commit body.",
                path.display()
            );
        }
    }

    // Thresholds --------------------------------------------------------
    assert!(
        agg.f1() >= MIN_AGGREGATE_F1,
        "aggregate F1 {:.3} fell below the {MIN_AGGREGATE_F1:.3} floor",
        agg.f1(),
    );
    assert_eq!(
        violations, 0,
        "{violations} `# nogroup:` violation(s) — pairs the fixtures forbid \
         merging that the grouper merged"
    );
}
