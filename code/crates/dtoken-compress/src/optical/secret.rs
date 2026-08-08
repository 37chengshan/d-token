//! Shared, conservative secret classification for every Optical/Hybrid boundary.
//!
//! This module deliberately prefers preserving a whole block as native text over
//! risking duplication into a rendered image or exact-value sidecar.

pub const SECRET_CLASSIFIER_REVISION: &str = "dtoken-secret-classifier-v5";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    Authorization,
    CookieOrSession,
    PrivateKey,
    ProviderCredential,
    Jwt,
    LabeledCredential,
    HighEntropyCredential,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretFinding {
    pub kind: SecretKind,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SecretClassifier;

impl SecretClassifier {
    pub fn classify(self, text: &str) -> Option<SecretFinding> {
        let canonical = canonicalize(text);
        let lower = canonical.to_ascii_lowercase();

        if [
            "authorization: bearer ",
            "authorization=bearer ",
            "authorization: basic ",
            "authorization=basic ",
            "bearer ",
            "basic ",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
        {
            return finding(SecretKind::Authorization);
        }
        if [
            "cookie:",
            "cookie=",
            "set-cookie:",
            "session=",
            "session_id=",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
        {
            return finding(SecretKind::CookieOrSession);
        }
        if lower.contains("-----begin ")
            && (lower.contains(" private key-----") || lower.contains("openssh private key"))
        {
            return finding(SecretKind::PrivateKey);
        }
        if contains_spaced_labeled_secret(&canonical) {
            return finding(SecretKind::LabeledCredential);
        }

        // Tokenize on whitespace AND structured-text delimiters so compact
        // JSON (`{"api_key":"sk-..."}`, `{"base_url":"...","api_key_ref":
        // "sk-..."}`) and handwritten compact config arrive as separate
        // key/value chunks instead of one whitespace blob whose first key
        // hides every later credential.
        let tokens = canonical
            .split(|character: char| {
                character.is_whitespace() || matches!(character, '"' | '\'' | ',' | ';')
            })
            .map(clean_token)
            .collect::<Vec<_>>();
        let mut index = 0usize;
        while index < tokens.len() {
            let token = tokens[index];
            if token.is_empty() {
                index += 1;
                continue;
            }
            let token_lower = token.to_ascii_lowercase();
            if is_provider_credential(token, &token_lower) {
                return finding(SecretKind::ProviderCredential);
            }
            if is_jwt(token) {
                return finding(SecretKind::Jwt);
            }
            // Credential-store references (`os-credential://`, `wincred://`,
            // `keychain://`) are identifiers, not secret material — but only
            // while opaque. A reference with userinfo, query, fragment, or
            // whitespace carries credential material and is classified.
            if is_credential_reference_like(token) {
                if !is_opaque_credential_reference(token) {
                    return finding(SecretKind::LabeledCredential);
                }
                index += 1;
                continue;
            }
            if let Some((label, value)) = split_labeled_value(token) {
                if let Some(finding) = classified_labeled_value(label, value) {
                    return Some(finding);
                }
                // A label chunk whose separator has no inline value
                // (`api_key:` in handwritten `api_key: value`) still pairs
                // with the next chunk below.
                if !value.is_empty() || (!token.ends_with('=') && !token.ends_with(':')) {
                    index += 1;
                    continue;
                }
            }
            // Structured text splits a key and its value across chunks:
            // `"api_key":"sk-..."` yields `api_key`, `:`, `sk-...` and
            // handwritten `api_key: value` yields `api_key:`, `value`.
            // Pair the label chunk with the following value chunk so compact
            // configs get the same labeled checks as spaced configs.
            let trailing_label = token.ends_with('=') || token.ends_with(':');
            if trailing_label && index + 1 < tokens.len() {
                let label = token.trim_end_matches(['=', ':']);
                if let Some(finding) = classified_labeled_value(label, tokens[index + 1]) {
                    return Some(finding);
                }
            } else if index + 2 < tokens.len() && matches!(tokens[index + 1], "=" | ":") {
                let label = token;
                if let Some(finding) = classified_labeled_value(label, tokens[index + 2]) {
                    return Some(finding);
                }
            }
            index += 1;
        }

        None
    }

    pub fn contains_secret(self, text: &str) -> bool {
        self.classify(text).is_some()
    }
}

pub fn contains_secret_like(text: &str) -> bool {
    SecretClassifier.contains_secret(text)
}

/// Labeled-value checks shared by whitespace tokens, compact structured
/// chunks, and the spaced-label scan: provider prefixes, JWTs, sensitive
/// labels, and high-entropy credential labels — with an exemption for
/// opaque OS credential-store references.
fn classified_labeled_value(label: &str, value: &str) -> Option<SecretFinding> {
    let value = clean_token(value);
    // In compact structured text the remainder after a key's separator may
    // run straight into the next key (`"api_key_ref":null,"base_url":
    // "https://..."`). Only the first delimited chunk is this label's
    // actual value; trailing keys must not contribute entropy to its check.
    let value = value
        .split(|character: char| {
            character.is_whitespace() || matches!(character, '"' | '\'' | ',' | ';')
        })
        .next()
        .unwrap_or_default();
    let value_lower = value.to_ascii_lowercase();
    if is_provider_credential(value, &value_lower) {
        return finding(SecretKind::ProviderCredential);
    }
    if is_jwt(value) {
        return finding(SecretKind::Jwt);
    }
    if is_sensitive_label(label) && !value.is_empty() {
        return finding(SecretKind::LabeledCredential);
    }
    if is_credential_label(label) {
        if is_credential_reference_like(value) {
            if is_opaque_credential_reference(value) {
                return None;
            }
            return finding(SecretKind::LabeledCredential);
        }
        if looks_high_entropy(value) {
            return finding(SecretKind::HighEntropyCredential);
        }
    }
    None
}

/// Opaque OS credential-store reference (`os-credential://`, `wincred://`,
/// `keychain://`) with a non-empty service and account and no userinfo,
/// query, fragment, or whitespace. Such references are identifiers, not
/// credential material, so numeric slugs (`gpt-4o`, `claude-3-5-sonnet`,
/// `glm-5`) must not trip the high-entropy heuristic.
fn is_opaque_credential_reference(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let Some(rest) = ["os-credential://", "wincred://", "keychain://"]
        .iter()
        .find_map(|scheme| lower.strip_prefix(scheme))
    else {
        return false;
    };
    let Some((authority, path)) = rest.split_once('/') else {
        return false;
    };
    !authority.is_empty()
        && !path.is_empty()
        && !authority.contains(['@', '?', '#', ':', ' ', '\t', '\n', '\r'])
        && !path.contains(['@', '?', '#', ' ', '\t', '\n', '\r', '"', '\'', ',', ';'])
}

/// True when the value starts with an allowed credential-store scheme,
/// whether or not the reference is opaque. Reference-like strings with
/// userinfo, query, fragment, or whitespace carry secret material and must
/// be classified rather than exempted.
fn is_credential_reference_like(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("os-credential://")
        || lower.starts_with("wincred://")
        || lower.starts_with("keychain://")
}

fn finding(kind: SecretKind) -> Option<SecretFinding> {
    Some(SecretFinding { kind })
}

fn canonicalize(text: &str) -> String {
    let width_normalized = text
        .chars()
        .filter_map(|character| match character {
            '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}' => None,
            '\u{3000}' => Some(' '),
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(u32::from(character) - 0xFEE0),
            _ => Some(character),
        })
        .collect::<String>();
    decode_literal_ascii_escapes(&width_normalized)
}

fn decode_literal_ascii_escapes(text: &str) -> String {
    let characters = text.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(text.len());
    let mut index = 0usize;
    while index < characters.len() {
        if characters[index] == '\\' && index + 1 < characters.len() {
            let (digits, width) = match characters[index + 1] {
                'u' | 'U' => (4usize, 2usize),
                'x' | 'X' => (2usize, 2usize),
                _ => (0usize, 0usize),
            };
            if digits > 0 && index + width + digits <= characters.len() {
                let hex = characters[index + width..index + width + digits]
                    .iter()
                    .collect::<String>();
                if let Ok(value) = u32::from_str_radix(&hex, 16) {
                    if value <= 0x7f {
                        if let Some(decoded) = char::from_u32(value) {
                            output.push(decoded);
                            index += width + digits;
                            continue;
                        }
                    }
                }
            }
        }
        output.push(characters[index]);
        index += 1;
    }
    output
}

fn contains_spaced_labeled_secret(text: &str) -> bool {
    text.lines().any(|line| {
        line.char_indices()
            .filter(|(_, character)| matches!(character, '=' | ':'))
            .any(|(separator, _)| {
                let left = &line[..separator];
                let right = &line[separator + 1..];
                let value = right
                    .trim_start_matches(|character: char| {
                        character.is_whitespace()
                            || matches!(character, '"' | '\'' | '{' | '[' | '(')
                    })
                    .split(|character: char| {
                        character.is_whitespace()
                            || matches!(character, '"' | '\'' | ',' | ';' | '}' | ']' | ')')
                    })
                    .next()
                    .unwrap_or_default();
                if value.is_empty() {
                    return false;
                }

                // The label is the key name immediately before the
                // separator. In structured text (JSON/TOML) that key is one
                // quoted segment — spaces inside the quotes are part of the
                // same key ("api key"). Restricting the window to that
                // segment prevents an earlier key on the same line (e.g.
                // `"api_key_ref"` two keys before `"base_url"`) from being
                // misread as part of this label, which would otherwise
                // reject benign values such as https base URLs.
                let words = label_words_for(left);
                // The scheme word immediately before the separator, taken
                // from the raw left side (`api_key_ref=os-credential`,
                // `"api_key_ref":"os-credential`, `api_key_ref:
                // os-credential` all end in the same scheme word).
                let scheme_word = left
                    .split(|character: char| {
                        character.is_whitespace()
                            || matches!(character, '=' | ':' | '"' | '\'' | ',' | ';')
                    })
                    .filter(|word| !word.is_empty())
                    .next_back()
                    .unwrap_or("");
                (1..=words.len().min(3)).any(|width| {
                    let label = words[words.len() - width..].join(" ");
                    if is_sensitive_label(&label) {
                        return true;
                    }
                    if !is_credential_label(&label) {
                        return false;
                    }
                    // Opaque OS credential-store references are identifiers:
                    // numeric slugs such as `gpt-4o` must not trip the
                    // entropy heuristic. The separator inside
                    // `os-credential://...` reconstructs the full reference
                    // from the scheme word plus the `//` remainder.
                    if is_opaque_credential_reference(value)
                        || is_opaque_credential_reference(&format!("{scheme_word}:{value}"))
                    {
                        return false;
                    }
                    // ...but reference-like values with userinfo, query, or
                    // fragment are exactly the shapes that hide secrets.
                    if is_credential_reference_like(value)
                        || is_credential_reference_like(&format!("{scheme_word}:{value}"))
                    {
                        return true;
                    }
                    looks_high_entropy(value)
                })
            })
    })
}

fn label_words_for(left: &str) -> Vec<&str> {
    match left.rfind('"') {
        Some(last_quote) => {
            let after = &left[last_quote + 1..];
            if after.trim().is_empty() {
                left[..last_quote]
                    .rsplit_once('"')
                    .map(|(_, quoted_key_segment)| quoted_key_segment)
                    .unwrap_or(after)
            } else {
                after
            }
            .split_whitespace()
            .collect()
        }
        None => left.split_whitespace().collect(),
    }
}

fn clean_token(token: &str) -> &str {
    token.trim_matches(|character: char| {
        matches!(
            character,
            ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '"' | '\''
        )
    })
}

fn split_labeled_value(token: &str) -> Option<(&str, &str)> {
    token
        .split_once('=')
        .or_else(|| token.split_once(':'))
        .map(|(label, value)| (label.trim(), value.trim()))
}

fn normalized_label(label: &str) -> String {
    label
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_sensitive_label(label: &str) -> bool {
    matches!(
        normalized_label(label).as_str(),
        "apikey"
            | "xapikey"
            | "clientsecret"
            | "password"
            | "passwd"
            | "secret"
            | "token"
            | "accesstoken"
            | "refreshtoken"
            | "credential"
            | "credentials"
            | "auth"
            | "authentication"
            | "authorization"
            | "proxyauthorization"
            | "cookie"
            | "setcookie"
            | "session"
            | "sessionid"
            | "privatekey"
            | "accesskey"
            | "secretaccesskey"
            | "githubtoken"
            | "slacktoken"
            | "openaitoken"
            | "anthropictoken"
            | "awssessiontoken"
            | "awssecuritytoken"
    )
}

fn is_credential_label(label: &str) -> bool {
    let label = normalized_label(label);
    label.contains("auth") || label.contains("key") || label.contains("credential")
}

fn is_provider_credential(token: &str, lower: &str) -> bool {
    (lower.starts_with("sk-") && token.len() >= 12)
        || lower.starts_with("sk-ant-")
        || lower.starts_with("ghp_")
        || lower.starts_with("github_pat_")
        || lower.starts_with("xoxb-")
        || lower.starts_with("xoxp-")
        || (lower.starts_with("aiza") && token.len() >= 35)
        || lower.starts_with("gocspx-")
        || (lower.starts_with("ya29.") && token.len() >= 30)
        || ((token.starts_with("AKIA") || token.starts_with("ASIA"))
            && token.len() == 20
            && token
                .chars()
                .all(|character| character.is_ascii_alphanumeric()))
}

fn is_jwt(token: &str) -> bool {
    let mut parts = token.split('.');
    let Some(header) = parts.next() else {
        return false;
    };
    let Some(payload) = parts.next() else {
        return false;
    };
    let Some(signature) = parts.next() else {
        return false;
    };
    parts.next().is_none()
        && header.starts_with("eyJ")
        && payload.len() >= 8
        && signature.len() >= 8
        && [header, payload, signature]
            .iter()
            .all(|part| part.chars().all(is_base64_url_character))
}

fn is_base64_url_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '=')
}

fn looks_high_entropy(value: &str) -> bool {
    let value = clean_token(value);
    if !(24..=512).contains(&value.len()) {
        return false;
    }
    let has_lower = value
        .chars()
        .any(|character| character.is_ascii_lowercase());
    let has_upper = value
        .chars()
        .any(|character| character.is_ascii_uppercase());
    let has_digit = value.chars().any(|character| character.is_ascii_digit());
    let has_symbol = value
        .chars()
        .any(|character| matches!(character, '-' | '_' | '/' | '+' | '=' | '.'));
    usize::from(has_lower)
        + usize::from(has_upper)
        + usize::from(has_digit)
        + usize::from(has_symbol)
        >= 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_provider_and_structured_credentials() {
        for value in [
            "Authorization: Basic Zm9vOmJhcg==",
            "credential=AbCDefghijklmnop12345678-_",
            "session_id=opaque-session",
            "ghp_abcdefghijklmnopqrstuvwxyz012345",
            "xoxb-123456789-abcdefghijklmnopqrstuv",
            "AKIAIOSFODNN7EXAMPLE",
            "-----BEGIN PRIVATE KEY-----",
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.signature123",
            "github_token=ghp_abcdefghijklmnopqrstuvwxyz012345",
            "slack_token=xoxb-123456789-abcdefghijklmnopqrstuv",
            "anthropic_token=sk-ant-abcdefghijklmnop",
            "aws_session_token=opaque-session-value",
            r#"{"openai_token":"sk-abcdefghijklmnop"}"#,
        ] {
            assert!(contains_secret_like(value), "expected secret: {value}");
        }
    }

    #[test]
    fn detects_google_provider_credentials() {
        for value in [
            "AIzaSyA1234567890abcdefghijklmnopqrstuv",
            "GOCSPX-abcdefghijklmnopqrstuvwxyz123456",
            "ya29.abcdefghijklmnopqrstuvwxyz1234567890",
            "google_api_key=AIzaSyA1234567890abcdefghijklmnopqrstuv",
            "client_secret=GOCSPX-abcdefghijklmnopqrstuvwxyz123456",
            r#"{"api_key":"ya29.abcdefghijklmnopqrstuvwxyz1234567890"}"#,
        ] {
            assert!(contains_secret_like(value), "expected secret: {value}");
        }
        for value in [
            "AIza",
            "aiza",
            "AIzaSyA", // below the reasonable-length threshold
            "GOCSPX",  // prefix without the "-" separator is not a credential
            "ya29.0",
            "note=prefix aiza-notes are documentation, not credentials",
        ] {
            assert!(!contains_secret_like(value), "expected public: {value}");
        }
    }

    #[test]
    fn compact_structured_config_never_hides_provider_credentials() {
        for value in [
            r#"{"api_key":"sk-abcdefghijklmnop"}"#,
            // The credential key is not first: the leading base_url must not
            // hide the api_key_ref value behind its own token.
            r#"{"base_url":"https://one.example.test/v1","api_key_ref":"sk-abcdefghijklmnop"}"#,
            r#"{"base_url":"https://one.example.test/v1","api_key_ref":"ghp_abcdefghijklmnopqrstuvwxyz012345","routes":[]}"#,
            r#"{"providers":[{"id":"p1","base_url":"https://one.example.test/v1","api_key_ref":"sk-ant-abcdefghijklmnop"}]}"#,
            r#"{"api_key":"eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.signature123"}"#,
            r#"{"openai_token":"sk-abcdefghijklmnop","base_url":"https://one.example.test/v1"}"#,
            // Handwritten compact config without quotes.
            "api_key_ref:sk-abcdefghijklmnop,base_url:https://one.example.test/v1",
            // Pretty JSON with the credential key after a URL-bearing key.
            "{\n  \"base_url\": \"https://one.example.test/v1\",\n  \"api_key_ref\": \"sk-abcdefghijklmnop\"\n}",
        ] {
            assert!(contains_secret_like(value), "expected secret: {value}");
        }
    }

    #[test]
    fn detects_fullwidth_and_zero_width_obfuscation() {
        assert!(contains_secret_like("ｔｏｋｅｎ＝abc123secret"));
        assert!(contains_secret_like("to\u{200b}ken=abc123secret"));
    }

    #[test]
    fn detects_spaced_json_and_literal_escape_labels() {
        for value in [
            "INFO password = huntertwo",
            r#"{"password": "huntertwo"}"#,
            r#"{"api key" : "AbCDefghijklmnop12345678-_"}"#,
            "INFO cookie : short-session-value",
            r#"{"authorization" : "Token short-value"}"#,
            r#"{"co\u006fkie" : "session\x2dvalue"}"#,
            r"a\u0075th = short-value",
            r"to\u006ben = credentialmaterial",
            r"pass\x77ord = credentialmaterial",
        ] {
            assert!(contains_secret_like(value), "expected secret: {value}");
        }
    }

    #[test]
    fn ordinary_public_log_is_not_a_secret() {
        assert!(!contains_secret_like(
            "INFO worker completed retry scheduled public history"
        ));
        assert!(!contains_secret_like(
            "INFO author completed authentication documentation"
        ));
    }

    #[test]
    fn benign_route_config_with_reference_only_credentials_is_not_a_secret() {
        for value in [
            r#"{"api_key_ref":null,"base_url":"https://one.example.test/v1"}"#,
            r#"{"providers":[{"api_key_ref":null,"base_url":"https://one.example.test/v1","models":["model/test"]}]}"#,
            "log_path: ./logs/reviewed-route.jsonl",
            "INFO provider base_url https://one.example.test/v1",
            r#"{"api_key_ref":"os-credential://d-token/providers/opencode-zen"}"#,
        ] {
            assert!(!contains_secret_like(value), "expected public: {value}");
        }
    }

    #[test]
    fn opaque_credential_references_with_numeric_slugs_are_not_secrets() {
        for value in [
            "os-credential://d-token/providers/gpt-4o",
            "os-credential://d-token/providers/claude-3-5-sonnet",
            "os-credential://d-token/providers/glm-5",
            "wincred://d-token/providers/gpt-4o",
            "keychain://d-token/providers/gpt-4o",
            r#"{"api_key_ref":"os-credential://d-token/providers/gpt-4o"}"#,
            r#"{"api_key_ref":"os-credential://d-token/providers/claude-3-5-sonnet"}"#,
            r#"{"api_key_ref":"os-credential://d-token/providers/glm-5"}"#,
            r#"{"api_key_ref":"wincred://d-token/providers/gpt-4o"}"#,
            r#"{"providers":[{"api_key_ref":"wincred://d-token/providers/gpt-4o"}]}"#,
            "api_key_ref=os-credential://d-token/providers/gpt-4o",
            "api_key_ref: os-credential://d-token/providers/claude-3-5-sonnet",
        ] {
            assert!(!contains_secret_like(value), "expected public: {value}");
        }
    }

    #[test]
    fn credential_references_with_userinfo_or_query_are_still_secrets() {
        for value in [
            r#"{"api_key_ref":"os-credential://user:pass@d-token/item"}"#,
            r#"{"api_key_ref":"os-credential://d-token/item?secret=value"}"#,
            r#"{"api_key_ref":"os-credential://d-token/item#fragment"}"#,
            "os-credential://user:pass@d-token/item",
            "api_key_ref=os-credential://d-token/item?secret=value",
        ] {
            assert!(contains_secret_like(value), "expected secret: {value}");
        }
    }
}
