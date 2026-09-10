# Rhyme-grouping eval fixtures

Each `*.txt` file is one test case: a header block of `#` annotation lines,
then a blank line, then the verse the grouper runs on.

## Annotation lines

- `# name: <slug>` — optional; defaults to the file stem.
- `# group <LABEL>: <word> <word> ...` — every listed word (matched
  case-insensitively, first occurrence in the verse) must end up in **one**
  computed rhyme group. Two `group` lines with different labels must land in
  **different** groups.
- `# nogroup: <word> <word> ...` — no two of the listed words may share a
  computed group (guards against over-merging bare assonance).

Only words named on a `group` line form the precision/recall universe; every
other word in the verse is ignored by the metric (but still affects
grouping, so keep the verses realistic).

## Sources

Synthetic files are hand-built minimal cases. Verse files are public domain:
`blake_tyger` (William Blake, 1794), `taylor_twinkle` (Jane Taylor, 1806).

## Baseline

`../rhyme_baseline.json` is the frozen grouping output at the time the
harness landed (epic issue #25, "1/10"). Later accuracy stages regenerate it
with `UPDATE_RHYME_BASELINE=1 cargo test --test rhyme_grouping` and show the
diff in the commit body.
