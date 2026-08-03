//! General text / HTML quality codec (收口计划 B2 item 3c).
//!
//! Conservative, content-preserving quality transforms for plain text and
//! HTML. The text codec collapses runs of blank lines and trims trailing
//! whitespace; the HTML codec additionally collapses whitespace inside text
//! nodes between tags. Neither codec removes visible content: they are
//! whitespace-only normalizers, safe to re-enable on a measured route when
//! `enable_text_kompress` is true.

use dtoken_types::CompressionPolicy;

pub const TEXT_CODEC_REVISION: &str = "text-quality-v1";
pub const HTML_CODEC_REVISION: &str = "html-quality-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextCodecResult {
    pub original_bytes: usize,
    pub compressed_bytes: usize,
    pub codec_revision: &'static str,
    pub transforms: Vec<&'static str>,
    pub body: String,
    pub negative_benefit: bool,
}

/// Compress plain text by collapsing runs of two or more blank lines to one
/// and trimming trailing whitespace from each line. `Off`/`Safe` preserve the
/// body verbatim. The transform never removes non-blank content.
pub fn compress_text(input: &str, policy: CompressionPolicy) -> TextCodecResult {
    let original_bytes = input.len();
    if matches!(policy, CompressionPolicy::Off | CompressionPolicy::Safe) {
        return passthrough(input, original_bytes, TEXT_CODEC_REVISION);
    }
    let mut transforms = Vec::new();
    let collapsed = collapse_blank_runs(input);
    if collapsed != input {
        transforms.push("text_blank_collapse");
    }
    let trimmed = trim_trailing_whitespace(&collapsed);
    if trimmed != collapsed {
        transforms.push("text_trailing_ws_trim");
    }
    let compressed_bytes = trimmed.len();
    let negative_benefit = compressed_bytes >= original_bytes;
    TextCodecResult {
        original_bytes,
        compressed_bytes,
        codec_revision: TEXT_CODEC_REVISION,
        transforms,
        body: trimmed,
        negative_benefit,
    }
}

/// Compress HTML by collapsing whitespace inside text nodes between tags and
/// trimming trailing whitespace. Tag structure and attribute values are
/// preserved verbatim; only inter-tag whitespace runs of two or more spaces
/// or blank lines are collapsed. Visible text content is never removed.
pub fn compress_html(input: &str) -> TextCodecResult {
    let original_bytes = input.len();
    let mut transforms = Vec::new();
    let collapsed = collapse_inter_tag_whitespace(input);
    if collapsed != input {
        transforms.push("html_inter_tag_ws_collapse");
    }
    let trimmed = trim_trailing_whitespace(&collapsed);
    if trimmed != collapsed {
        transforms.push("html_trailing_ws_trim");
    }
    let compressed_bytes = trimmed.len();
    let negative_benefit = compressed_bytes >= original_bytes;
    TextCodecResult {
        original_bytes,
        compressed_bytes,
        codec_revision: HTML_CODEC_REVISION,
        transforms,
        body: trimmed,
        negative_benefit,
    }
}

fn passthrough(input: &str, original_bytes: usize, revision: &'static str) -> TextCodecResult {
    TextCodecResult {
        original_bytes,
        compressed_bytes: original_bytes,
        codec_revision: revision,
        transforms: Vec::new(),
        body: input.to_string(),
        negative_benefit: false,
    }
}

fn collapse_blank_runs(text: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut blank_run = 0usize;
    for line in text.lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run == 1 {
                out.push(line);
            }
            continue;
        }
        blank_run = 0;
        out.push(line);
    }
    out.join("\n")
}

fn trim_trailing_whitespace(text: &str) -> String {
    let mut lines: Vec<String> = text
        .lines()
        .map(|line| line.trim_end().to_string())
        .collect();
    while matches!(lines.last(), Some(last) if last.trim().is_empty()) {
        lines.pop();
    }
    let joined = lines.join("\n");
    if text.ends_with('\n') && !joined.is_empty() {
        format!("{joined}\n")
    } else {
        joined
    }
}

/// Collapse runs of two or more whitespace characters that appear strictly
/// between `>` and `<` (inside text nodes). Tag content and attribute values
/// are left intact; only pure-whitespace text-node segments are normalized.
fn collapse_inter_tag_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    let mut ws_run = 0usize;
    for ch in text.chars() {
        match ch {
            '<' => {
                if ws_run > 0 {
                    out.push(' ');
                    ws_run = 0;
                }
                in_tag = true;
                out.push(ch);
            }
            '>' => {
                in_tag = false;
                out.push(ch);
            }
            c if c.is_whitespace() && !in_tag => {
                ws_run += 1;
            }
            c if !in_tag => {
                if ws_run > 0 {
                    out.push(' ');
                    ws_run = 0;
                }
                out.push(c);
            }
            c => out.push(c),
        }
    }
    if ws_run > 0 {
        out.push(' ');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_preserves_text_verbatim() {
        let body = "keep\n\n\n\nme";
        let r = compress_text(body, CompressionPolicy::Off);
        assert_eq!(r.body, body);
        assert!(!r.negative_benefit);
        assert!(r.transforms.is_empty());
    }

    #[test]
    fn balanced_collapses_blank_runs_without_removing_content() {
        let body = "a\n\n\n\nb";
        let r = compress_text(body, CompressionPolicy::Balanced);
        assert!(r.compressed_bytes < r.original_bytes);
        assert_eq!(r.body, "a\n\nb");
        assert!(r.transforms.contains(&"text_blank_collapse"));
    }

    #[test]
    fn text_codec_preserves_visible_content() {
        let body = "line one   \n\n\nline two   \n";
        let r = compress_text(body, CompressionPolicy::Balanced);
        assert!(r.body.contains("line one"));
        assert!(r.body.contains("line two"));
        assert!(!r.body.contains("   "));
    }

    #[test]
    fn html_collapses_inter_tag_whitespace() {
        let html = "<div>\n\n\n   <span>hi</span>    </div>";
        let r = compress_html(html);
        assert!(r.compressed_bytes < r.original_bytes);
        assert!(r.body.contains("<div>"));
        assert!(r.body.contains("<span>hi</span>"));
        assert!(r.body.contains("</div>"));
        assert!(!r.body.contains("\n\n\n"));
        assert!(!r.body.contains("    "));
    }

    #[test]
    fn html_preserves_tag_structure_and_attributes() {
        let html = "<a  href=\"x\">link</a>";
        let r = compress_html(html);
        assert_eq!(r.body, "<a  href=\"x\">link</a>");
    }

    #[test]
    fn negative_benefit_when_no_savings() {
        let body = "a";
        let r = compress_text(body, CompressionPolicy::Balanced);
        assert!(r.negative_benefit);
        assert_eq!(r.body, "a");
    }
}
