//! The Console's search, as Sifter's: one field, whose parts, between
//! spaces, all have to hold. A word is found anywhere, ignoring case;
//! `tag:name` keeps only lines in a category and `-tag:name` leaves one
//! out; and a regular expression goes between slashes, spaces and all. A bad
//! expression says why rather than quietly matching nothing.

use regex::Regex;

#[derive(Debug, Default)]
pub struct Query {
    /// Words to find, lowercase.
    words: Vec<String>,
    tags: Vec<String>,
    not_tags: Vec<String>,
    pattern: Option<Regex>,
    /// Why the expression could not be read, when it could not.
    pub error: Option<String>,
}

impl Query {
    pub fn parse(text: &str) -> Self {
        let mut query = Query::default();
        let mut rest = text.trim();
        while !rest.is_empty() {
            let taken = match rest.strip_prefix('/') {
                Some(body) => query.read_pattern(body),
                None => query.read_word(rest),
            };
            rest = rest[taken..].trim_start();
        }
        query
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.tags.is_empty() && self.not_tags.is_empty() && self.pattern.is_none()
    }

    /// Whether a line matches: `text` is what it says, `lower` the same in
    /// lowercase, made once, and `categories` the categories it is in.
    pub fn matches(&self, text: &str, lower: &str, categories: &[String]) -> bool {
        let in_category = |tag: &String| categories.iter().any(|c| c.to_lowercase() == *tag);
        self.words.iter().all(|word| lower.contains(word.as_str()))
            && self.tags.iter().all(in_category)
            && !self.not_tags.iter().any(in_category)
            && self.pattern.as_ref().is_none_or(|pattern| pattern.is_match(text))
    }

    /// A word, or a tag to keep or leave out. Returns how much it took.
    fn read_word(&mut self, rest: &str) -> usize {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let word = &rest[..end];
        if let Some(tag) = word.strip_prefix("-tag:").filter(|t| !t.is_empty()) {
            self.not_tags.push(tag.to_lowercase());
        } else if let Some(tag) = word.strip_prefix("tag:").filter(|t| !t.is_empty()) {
            self.tags.push(tag.to_lowercase());
        } else {
            self.words.push(word.to_lowercase());
        }
        end
    }

    /// A regular expression after its opening slash, up to the next slash
    /// not escaped with a backslash. Returns how much it took, slashes too.
    fn read_pattern(&mut self, body: &str) -> usize {
        let mut escaped = false;
        let close = body.char_indices().find(|&(_, c)| {
            let ends = c == '/' && !escaped;
            escaped = c == '\\' && !escaped;
            ends
        });
        let (source, taken) = match close {
            Some((at, _)) => (&body[..at], at + 2),
            None => (body, body.len() + 1),
        };
        match Regex::new(source) {
            Ok(pattern) => self.pattern = Some(pattern),
            Err(error) => {
                let reason = error.to_string();
                let reason = reason.lines().last().unwrap_or(&reason).trim().trim_start_matches("error: ");
                self.error = Some(format!("/{source}/ is not a regular expression: {reason}"));
            }
        }
        taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(query: &str, text: &str, categories: &[&str]) -> bool {
        let categories: Vec<String> = categories.iter().map(|c| c.to_string()).collect();
        Query::parse(query).matches(text, &text.to_lowercase(), &categories)
    }

    #[test]
    fn words_tags_and_expressions_all_have_to_hold() {
        assert!(found("nullref", "NullReferenceException in Spinner", &[]), "a word, ignoring case");
        assert!(!found("nullref missing", "NullReferenceException", &[]), "every word");
        assert!(found("tag:combat", "hit", &["Combat"]));
        assert!(!found("-tag:combat", "hit", &["Combat"]));
        assert!(
            found(r"/^Failed to load .*\.asset$/", "Failed to load Rock.asset", &[]),
            "spaces inside the expression"
        );
        assert!(!found("/^Failed/", "It Failed", &[]));
        assert!(found(r"/a\/b/ tag:io", "a/b", &["IO"]), "an escaped slash, then more");
        let bad = Query::parse("/[unclosed/");
        assert!(bad.error.as_deref().is_some_and(|e| e.starts_with("/[unclosed/ is not")), "{:?}", bad.error);
        assert!(Query::parse("  ").is_empty());
    }
}
