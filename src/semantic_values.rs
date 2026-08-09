//! Small semantic text values used between permissive tool adapters and
//! application/domain behavior.

use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticTextError {
    Blank,
    TooLong { max: usize, actual: usize },
}

impl Display for SemanticTextError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Blank => f.write_str("value must not be blank"),
            Self::TooLong { max, actual } => {
                write!(f, "value exceeds the {max}-character limit ({actual})")
            }
        }
    }
}

impl std::error::Error for SemanticTextError {}

/// An identifier-like value whose surrounding whitespace is not meaningful.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TrimmedText(String);

impl TrimmedText {
    pub fn new(value: impl Into<String>) -> Result<Self, SemanticTextError> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(SemanticTextError::Blank);
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl TryFrom<String> for TrimmedText {
    type Error = SemanticTextError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for TrimmedText {
    type Error = SemanticTextError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl AsRef<str> for TrimmedText {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Display for TrimmedText {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Content that must contain a non-whitespace character but whose original
/// bytes must remain unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiredContent(String);

impl RequiredContent {
    pub fn new(value: impl Into<String>) -> Result<Self, SemanticTextError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(SemanticTextError::Blank);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl TryFrom<String> for RequiredContent {
    type Error = SemanticTextError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for RequiredContent {
    type Error = SemanticTextError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl AsRef<str> for RequiredContent {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Content with a Unicode scalar-value limit. The original content is kept;
/// this type never trims or otherwise rewrites it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedText<const MAX: usize>(String);

impl<const MAX: usize> BoundedText<MAX> {
    pub fn new(value: impl Into<String>) -> Result<Self, SemanticTextError> {
        let value = value.into();
        let actual = value.chars().count();
        if actual > MAX {
            return Err(SemanticTextError::TooLong { max: MAX, actual });
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    pub fn len_chars(&self) -> usize {
        self.0.chars().count()
    }

    pub fn len_bytes(&self) -> usize {
        self.0.len()
    }
}

impl<const MAX: usize> TryFrom<String> for BoundedText<MAX> {
    type Error = SemanticTextError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<const MAX: usize> TryFrom<&str> for BoundedText<MAX> {
    type Error = SemanticTextError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<const MAX: usize> AsRef<str> for BoundedText<MAX> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::{BoundedText, RequiredContent, SemanticTextError, TrimmedText};

    #[test]
    fn trimmed_text_normalizes_only_identifier_like_values() {
        let value = TrimmedText::try_from("  queue-a  ").expect("identifier");
        assert_eq!(value.as_str(), "queue-a");
        assert_eq!(
            TrimmedText::try_from(" \n\t ").unwrap_err(),
            SemanticTextError::Blank
        );
    }

    #[test]
    fn required_content_rejects_blank_without_rewriting_content() {
        let value = RequiredContent::try_from("  <p>hello</p>  ").expect("content");
        assert_eq!(value.as_str(), "  <p>hello</p>  ");
        assert_eq!(
            RequiredContent::try_from("\n\t").unwrap_err(),
            SemanticTextError::Blank
        );
    }

    #[test]
    fn bounded_text_counts_unicode_characters_and_preserves_bytes() {
        let value = BoundedText::<3>::try_from("é🙂a").expect("three characters");
        assert_eq!(value.len_chars(), 3);
        assert_eq!(value.len_bytes(), "é🙂a".len());
        assert_eq!(value.as_str(), "é🙂a");
        assert!(matches!(
            BoundedText::<2>::try_from("é🙂a"),
            Err(SemanticTextError::TooLong { max: 2, actual: 3 })
        ));
    }
}
