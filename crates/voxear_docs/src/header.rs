//! The few lines between `---` marks at the top of a page file: its id,
//! title, tags, when it was last updated, a banner picture and where it
//! sorts among its neighbours.
//!
//! ```text
//! ---
//! id: 3f9a1c2b7d4e
//! tags: [combat, ai]
//! updated: 2026-09-19
//! ---
//! ```
//!
//! A page without a header is fine; it gets one the first time the
//! VoxDocs saves it. Lines VoxDocs does not know are kept as they were.

const FENCE: &str = "---";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Header {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub updated: String,
    pub banner: String,
    pub order: i32,
    /// Lines VoxDocs does not read, written back as they were.
    other: Vec<String>,
}

impl Header {
    /// Splits a page file into its header and the Markdown after it.
    pub fn split(text: &str) -> (Header, String) {
        let mut header = Header::default();
        let normalised = text.replace("\r\n", "\n");
        let Some(rest) = normalised.strip_prefix("---\n") else { return (header, normalised) };
        // The closing fence is a line of its own, straight away for an empty header.
        let (inside, after) = match rest.strip_prefix(FENCE) {
            Some(after) => ("", after),
            None => match rest.find("\n---") {
                Some(end) => (&rest[..end], &rest[end + 4..]),
                None => return (header, normalised),
            },
        };
        for line in inside.lines() {
            header.read(line);
        }
        let body = after.split_once('\n').map_or("", |(_, body)| body);
        (header, body.trim_start_matches('\n').to_string())
    }

    /// The header, then the Markdown: the whole page file. An empty header
    /// is left out.
    pub fn join(&self, body: &str) -> String {
        if self.is_empty() {
            return body.to_string();
        }
        let mut text = String::from("---\n");
        let mut line = |key: &str, value: &str| {
            if !value.is_empty() {
                text.push_str(&format!("{key}: {value}\n"));
            }
        };
        line("id", &self.id);
        line("title", &self.title);
        line("tags", &if self.tags.is_empty() { String::new() } else { format!("[{}]", self.tags.join(", ")) });
        line("updated", &self.updated);
        line("banner", &self.banner);
        line("order", &if self.order == 0 { String::new() } else { self.order.to_string() });
        for other in &self.other {
            text.push_str(other);
            text.push('\n');
        }
        text.push_str(FENCE);
        text.push_str("\n\n");
        text.push_str(body.trim_start_matches('\n'));
        text
    }

    pub fn is_empty(&self) -> bool {
        self.id.is_empty()
            && self.title.is_empty()
            && self.tags.is_empty()
            && self.updated.is_empty()
            && self.banner.is_empty()
            && self.order == 0
            && self.other.is_empty()
    }

    fn read(&mut self, line: &str) {
        let Some((key, value)) = line.split_once(':') else {
            if !line.trim().is_empty() {
                self.other.push(line.to_string());
            }
            return;
        };
        let value = value.trim().trim_matches('"').to_string();
        match key.trim() {
            "id" => self.id = value,
            "title" => self.title = value,
            "tags" => {
                let list = value.trim_start_matches('[').trim_end_matches(']');
                self.tags =
                    list.split(',').map(|t| t.trim().trim_matches('"').to_string()).filter(|t| !t.is_empty()).collect();
            }
            "updated" => self.updated = value,
            "banner" => self.banner = value,
            "order" => self.order = value.parse().unwrap_or_default(),
            _ => self.other.push(line.to_string()),
        }
    }
}

/// A new page id: twelve hex digits, different every time.
pub fn new_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher
        .write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
    format!("{:012x}", hasher.finish() & 0xffff_ffff_ffff)
}

/// Today's date as a page's `updated` line writes it: `2026-10-31`.
pub fn today() -> String {
    let seconds = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (year, month, day) = civil_date((seconds / 86_400) as i64);
    format!("{year:04}-{month:02}-{day:02}")
}

/// The calendar date `days` after 1970-01-01, by Howard Hinnant's method.
fn civil_date(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 } as u32;
    (year_of_era + era * 400 + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_read_and_write_back() {
        let file = "---\nid: a4f08c3e91b2\ntags: [docs, guide]\nupdated: 2026-09-19\nauthor: Jo\n---\n\nPages are plain Markdown.\n";
        let (header, body) = Header::split(file);
        assert_eq!(header.id, "a4f08c3e91b2");
        assert_eq!(header.tags, ["docs", "guide"]);
        assert_eq!(header.updated, "2026-09-19");
        assert_eq!(body, "Pages are plain Markdown.\n");
        assert_eq!(header.join(&body), file, "unknown lines are kept, and the file comes back as it was");

        let (header, body) = Header::split("Just words.\n");
        assert!(header.is_empty());
        assert_eq!(header.join(&body), "Just words.\n");
    }

    #[test]
    fn dates_and_ids() {
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(20_757), (2026, 10, 31));
        assert_eq!(civil_date(11_016), (2000, 2, 29));
        let (a, b) = (new_id(), new_id());
        assert!(a.len() == 12 && a.chars().all(|c| c.is_ascii_hexdigit()) && a != b);
        assert_eq!(today().len(), 10);
    }
}
