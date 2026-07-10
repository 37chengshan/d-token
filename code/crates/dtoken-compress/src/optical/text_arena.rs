use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct TextRefId(String);

impl TextRefId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for TextRefId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("TextRefId").field(&self.0).finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextSensitivity {
    NormalInternal,
    ExactInternal,
    SecretInternal,
    RenderCandidateInternal,
}

#[derive(Clone, PartialEq, Eq)]
pub struct TextRef {
    id: TextRefId,
    len_chars: usize,
    len_bytes: usize,
    digest: String,
    sensitivity: TextSensitivity,
}

impl TextRef {
    pub fn id(&self) -> &TextRefId {
        &self.id
    }

    pub fn len_chars(&self) -> usize {
        self.len_chars
    }

    pub fn len_bytes(&self) -> usize {
        self.len_bytes
    }

    pub fn digest_prefix(&self) -> &str {
        self.digest.get(..8).unwrap_or(&self.digest)
    }

    pub fn sensitivity(&self) -> &TextSensitivity {
        &self.sensitivity
    }
}

impl fmt::Debug for TextRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextRef")
            .field("id", &self.id)
            .field("len_chars", &self.len_chars)
            .field("len_bytes", &self.len_bytes)
            .field("digest_prefix", &self.digest_prefix())
            .field("sensitivity", &self.sensitivity)
            .finish()
    }
}

struct TextArenaEntry {
    text_ref: TextRef,
    text: String,
}

pub struct TextArena {
    request_id: String,
    entries: Vec<TextArenaEntry>,
}

impl fmt::Debug for TextArena {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextArena")
            .field("request_id", &self.request_id)
            .field("entries", &self.entries.len())
            .finish()
    }
}

impl TextArena {
    pub fn new(request_id: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            entries: Vec::new(),
        }
    }

    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub fn insert(&mut self, text: impl Into<String>, sensitivity: TextSensitivity) -> TextRef {
        let text = text.into();
        let entry_index = self.entries.len();
        let digest = digest_text(&text);
        let text_ref = TextRef {
            id: TextRefId(format!("txt-{}-{}", entry_index, &digest[..8])),
            len_chars: text.chars().count(),
            len_bytes: text.len(),
            digest,
            sensitivity,
        };
        self.entries.push(TextArenaEntry {
            text_ref: text_ref.clone(),
            text,
        });
        text_ref
    }

    pub fn get(&self, text_ref: &TextRef) -> Option<&str> {
        self.entries
            .iter()
            .find(|entry| entry.text_ref.id == text_ref.id)
            .map(|entry| entry.text.as_str())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn digest_text(text: &str) -> String {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_ref_debug_does_not_print_raw_text() {
        let mut arena = TextArena::new("request-1");
        let text_ref = arena.insert(
            "raw prompt content",
            TextSensitivity::RenderCandidateInternal,
        );
        let debug = format!("{:?}", text_ref);
        let arena_debug = format!("{:?}", arena);

        assert!(!debug.contains("raw prompt content"));
        assert!(!arena_debug.contains("raw prompt content"));
        assert!(debug.contains("digest_prefix"));
        assert_eq!(arena.get(&text_ref), Some("raw prompt content"));
    }

    #[test]
    fn text_arena_tracks_request_local_entries() {
        let mut arena = TextArena::new("request-2");
        let first = arena.insert("alpha", TextSensitivity::NormalInternal);
        let second = arena.insert("beta", TextSensitivity::ExactInternal);

        assert_eq!(arena.request_id(), "request-2");
        assert_eq!(arena.len(), 2);
        assert_eq!(arena.get(&first), Some("alpha"));
        assert_eq!(arena.get(&second), Some("beta"));
    }
}
