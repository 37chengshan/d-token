//! Shared, conservative secret classification for every Optical/Hybrid boundary.
//!
//! This module deliberately prefers preserving a whole block as native text over
//! risking duplication into a rendered image or exact-value sidecar.

pub const SECRET_CLASSIFIER_REVISION: &str = "dtoken-secret-classifier-v4";

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

        for token in canonical.split_whitespace().map(clean_token) {
            if token.is_empty() {
                continue;
            }
            let token_lower = token.to_ascii_lowercase();
            if is_provider_credential(token, &token_lower) {
                return finding(SecretKind::ProviderCredential);
            }
            if is_jwt(token) {
                return finding(SecretKind::Jwt);
            }
            if let Some((label, value)) = split_labeled_value(token) {
                let value = clean_token(value);
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
                if is_credential_label(label) && looks_high_entropy(value) {
                    return finding(SecretKind::HighEntropyCredential);
                }
            }
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

                let words = left
                    .split(|character: char| {
                        character.is_whitespace()
                            || matches!(character, '"' | '\'' | ',' | ';' | '{' | '[' | '(')
                    })
                    .filter(|word| !word.is_empty())
                    .collect::<Vec<_>>();
                (1..=words.len().min(3)).any(|width| {
                    let label = words[words.len() - width..].join(" ");
                    is_sensitive_label(&label)
                        || (is_credential_label(&label) && looks_high_entropy(value))
                })
            })
    })
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
}
