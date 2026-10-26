//! Content-level secret detection shared by the local context engine and the
//! legacy Optical compatibility surface.
//!
//! The implementation still lives in `optical::secret` while Optical is being
//! retired.  Keeping this small forwarding module gives new callers a stable,
//! image-independent import path and lets the old callers migrate gradually.

pub use crate::optical::secret::{SecretClassifier, SecretFinding, SecretKind};

pub use crate::optical::secret::SECRET_CLASSIFIER_REVISION;

/// Detect secret-like material from either text or opaque bytes.
///
/// The public boundary is byte-oriented so callers handling provider payloads
/// do not need to decode arbitrary input first. Invalid UTF-8 is treated as
/// opaque and therefore cannot accidentally enter a text codec.
pub fn contains_secret_like<T: ?Sized + AsRef<[u8]>>(input: &T) -> bool {
    std::str::from_utf8(input.as_ref())
        .map(crate::optical::secret::contains_secret_like)
        .unwrap_or(false)
}
