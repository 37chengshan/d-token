//! Strict unified-diff codec (收口计划 B2 item 3a).
//!
//! Strict unified-diff parser plus a reversible, content-preserving
//! context-repeat fold. The fold collapses adjacent identical context lines
//! into a counted marker and expands back to the exact original. The codec is
//! **default-off** on the live route path: `compress_diff` only produces a
//! fold when `verify_diff_reversible` confirms exact reconstruction;
//! otherwise the input is passed through unchanged.

use anyhow::{anyhow, Result};

pub const DIFF_PARSER_REVISION: &str = "unified-diff-strict-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffCodecResult {
    pub original_bytes: usize,
    pub compressed_bytes: usize,
    pub parser_revision: &'static str,
    pub transforms: Vec<&'static str>,
    pub reversible: bool,
    pub body: String,
}

/// Parse a strict unified diff: requires a single `diff --git` header, an
/// `--- `/`+++ ` pair, and at least one `@@ ... @@` hunk whose declared line
/// counts match the body. Returns `Ok(())` only for a well-formed diff.
pub fn parse_strict_unified_diff(text: &str) -> Result<()> {
    if text.ends_with('\n') {
        return Err(anyhow!("diff must not end with a trailing newline"));
    }
    if text.matches("diff --git ").count() != 1 {
        return Err(anyhow!("expected exactly one diff --git header"));
    }
    let lines: Vec<&str> = text.lines().collect();
    let old_header = lines
        .iter()
        .position(|line| line.starts_with("--- "))
        .ok_or_else(|| anyhow!("missing --- header"))?;
    let new_header = lines
        .iter()
        .position(|line| line.starts_with("+++ "))
        .ok_or_else(|| anyhow!("missing +++ header"))?;
    if new_header != old_header + 1 || old_header == 0 {
        return Err(anyhow!(
            "--- /+++ headers must be adjacent and after the git header"
        ));
    }
    if !lines.iter().any(|line| line.starts_with("@@ ")) {
        return Err(anyhow!("missing hunk header"));
    }

    let mut index = new_header + 1;
    while index < lines.len() {
        let (old_count, new_count) = parse_hunk_counts(lines[index])
            .ok_or_else(|| anyhow!("malformed hunk header at line {}", index + 1))?;
        index += 1;
        let mut old_seen = 0usize;
        let mut new_seen = 0usize;
        while index < lines.len() && !lines[index].starts_with("@@ ") {
            let line = lines[index];
            if line == "\\ No newline at end of file" {
                index += 1;
                continue;
            }
            match line.as_bytes().first() {
                Some(b' ') => {
                    old_seen += 1;
                    new_seen += 1;
                }
                Some(b'-') => old_seen += 1,
                Some(b'+') => new_seen += 1,
                _ => return Err(anyhow!("unexpected diff line at {}", index + 1)),
            }
            index += 1;
        }
        if old_seen != old_count || new_seen != new_count {
            return Err(anyhow!("hunk line count mismatch"));
        }
    }
    Ok(())
}

/// Compress a unified diff by folding adjacent identical context lines into a
/// counted marker. Returns a passthrough result (unchanged, reversible) when
/// the input is not a strict valid diff, already contains a fold marker, or
/// the fold would not reduce size / would not be exactly reversible.
pub fn compress_diff(input: &str) -> Result<DiffCodecResult> {
    let original_bytes = input.len();
    let valid = parse_strict_unified_diff(input).is_ok();
    if !valid || input.lines().any(has_repeat_marker) || !has_foldable_context_repeat(input) {
        return Ok(DiffCodecResult {
            original_bytes,
            compressed_bytes: original_bytes,
            parser_revision: DIFF_PARSER_REVISION,
            transforms: Vec::new(),
            reversible: true,
            body: input.to_string(),
        });
    }
    let folded = fold_context_repeats(input);
    if folded.len() >= original_bytes || !verify_diff_reversible(input, &folded)? {
        return Ok(DiffCodecResult {
            original_bytes,
            compressed_bytes: original_bytes,
            parser_revision: DIFF_PARSER_REVISION,
            transforms: Vec::new(),
            reversible: true,
            body: input.to_string(),
        });
    }
    Ok(DiffCodecResult {
        original_bytes,
        compressed_bytes: folded.len(),
        parser_revision: DIFF_PARSER_REVISION,
        transforms: vec!["diff_context_repeat_fold_v1"],
        reversible: true,
        body: folded,
    })
}

/// Verify that `compressed` expands back to exactly `original`.
pub fn verify_diff_reversible(original: &str, compressed: &str) -> Result<bool> {
    Ok(expand_folded(compressed).as_deref() == Some(original))
}

fn parse_hunk_counts(line: &str) -> Option<(usize, usize)> {
    let body = line.strip_prefix("@@ ")?.strip_suffix(" @@")?;
    let (old, new) = body.split_once(' ')?;
    Some((parse_range(old, '-')?, parse_range(new, '+')?))
}

fn parse_range(range: &str, prefix: char) -> Option<usize> {
    let range = range.strip_prefix(prefix)?;
    let (start, count) = range.split_once(',').unwrap_or((range, "1"));
    let start = start.parse::<usize>().ok()?;
    let count = count.parse::<usize>().ok()?;
    (start > 0 || count == 0).then_some(count)
}

fn has_repeat_marker(line: &str) -> bool {
    line.rsplit_once(" [repeated x")
        .and_then(|(_, suffix)| suffix.strip_suffix(']'))
        .is_some_and(|count| count.parse::<usize>().is_ok())
}

fn has_foldable_context_repeat(text: &str) -> bool {
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if !is_context_line(line) {
            continue;
        }
        if lines
            .peek()
            .is_some_and(|next| *next == line && is_context_line(next))
        {
            return true;
        }
    }
    false
}

fn is_context_line(line: &str) -> bool {
    line.starts_with(' ') && !has_repeat_marker(line)
}

fn fold_context_repeats(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if !is_context_line(line) {
            out.push(line.to_string());
            continue;
        }
        let mut count = 1usize;
        while lines
            .peek()
            .is_some_and(|next| *next == line && is_context_line(next))
        {
            let _ = lines.next();
            count += 1;
        }
        if count > 1 {
            out.push(format!("{line} [repeated x{count}]"));
        } else {
            out.push(line.to_string());
        }
    }
    out.join("\n")
}

fn expand_folded(candidate: &str) -> Option<String> {
    let mut expanded: Vec<String> = Vec::new();
    for line in candidate.lines() {
        let Some((base, suffix)) = line.rsplit_once(" [repeated x") else {
            expanded.push(line.to_string());
            continue;
        };
        let count = suffix.strip_suffix(']')?.parse::<usize>().ok()?;
        if !(2..=1_000_000).contains(&count) || !base.starts_with(' ') {
            return None;
        }
        expanded.extend(std::iter::repeat_n(base.to_string(), count));
    }
    Some(expanded.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn well_formed_diff() -> String {
        // old count = 4 (3 ctx + 1 removed), new count = 4 (3 ctx + 1 added)
        "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,4 +1,4 @@\n ctx\n ctx\n ctx\n-old\n+new".into()
    }

    #[test]
    fn parses_well_formed_diff() {
        assert!(parse_strict_unified_diff(&well_formed_diff()).is_ok());
    }

    #[test]
    fn rejects_trailing_newline() {
        let mut d = well_formed_diff();
        d.push('\n');
        assert!(parse_strict_unified_diff(&d).is_err());
    }

    #[test]
    fn rejects_missing_git_header() {
        let d = "--- a/f\n+++ b/f\n@@ -1,1 +1,1 @@\n-old\n+new";
        assert!(parse_strict_unified_diff(d).is_err());
    }

    #[test]
    fn compress_then_verify_reversible() {
        // Six identical context lines: folding ` c` x6 into a counted marker
        // reduces bytes (6 * 3 = 18 -> 2 + 14 = 16) and is exactly reversible.
        let d = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,7 +1,7 @@\n c\n c\n c\n c\n c\n c\n-old\n+new";
        let result = compress_diff(d).expect("compress");
        assert!(result.reversible);
        assert!(verify_diff_reversible(d, &result.body).expect("verify"));
        assert!(result.compressed_bytes < result.original_bytes);
        assert!(result.transforms.contains(&"diff_context_repeat_fold_v1"));
    }

    #[test]
    fn compress_passes_through_non_diff() {
        let not_a_diff = "just some text\nwith lines";
        let result = compress_diff(not_a_diff).expect("compress");
        assert!(result.reversible);
        assert_eq!(result.body, not_a_diff);
        assert!(result.transforms.is_empty());
    }

    #[test]
    fn verify_detects_non_reversible_edit() {
        assert!(!verify_diff_reversible("a\nb", "a\nc").expect("verify"));
    }
}
