//! Smart strings: a text's values filled in as ICU MessageFormat says, so
//! that a sentence fits its numbers and people in every language.
//!
//! - `{player}` is the value named `player`, as it is.
//! - `{count, number}` is a number, written as the language writes them:
//!   `1,234.5` in English, `1 234,5` in French.
//! - `{count, plural, one {# coin} other {# coins}}` picks by the
//!   language's plural rules (one, few, many and the rest), `=0 {…}` by an
//!   exact number first; `#` inside is the number, written as above.
//! - `{gender, select, female {her} male {his} other {their}}` picks by
//!   the value itself.
//!
//! The branches nest, and `other` is taken when nothing else fits. An
//! apostrophe quotes a brace or `#` (`'{'`), and two make one (`''`). A
//! name with no value, or a part that cannot be read, is left as written.

use icu_decimal::{DecimalFormatter, input::Decimal};
use icu_locale_core::Locale;
use icu_plurals::{PluralCategory, PluralRules};

/// The plural rules and number formatting of a language, made once for a
/// text.
struct Rules {
    plurals: Option<PluralRules>,
    numbers: Option<DecimalFormatter>,
}

impl Rules {
    fn of(language: &str) -> Self {
        let locale = language.parse::<Locale>().unwrap_or(Locale::UNKNOWN);
        Self {
            plurals: PluralRules::try_new_cardinal((&locale).into()).ok(),
            numbers: DecimalFormatter::try_new((&locale).into(), Default::default()).ok(),
        }
    }

    fn number(&self, value: &str) -> String {
        match (value.trim().parse::<Decimal>(), &self.numbers) {
            (Ok(number), Some(numbers)) => numbers.format(&number).to_string(),
            _ => value.to_string(),
        }
    }

    fn category(&self, number: &Decimal) -> &'static str {
        let category = self.plurals.as_ref().map_or(PluralCategory::Other, |rules| rules.category_for(number));
        match category {
            PluralCategory::Zero => "zero",
            PluralCategory::One => "one",
            PluralCategory::Two => "two",
            PluralCategory::Few => "few",
            PluralCategory::Many => "many",
            PluralCategory::Other => "other",
        }
    }
}

/// `text` with the values it names filled in, in `language`'s ways.
pub fn format(text: &str, values: &[(&str, &str)], language: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    Formatter { chars: &chars, values, rules: Rules::of(language) }.message(0, chars.len(), None, &mut out);
    out
}

struct Formatter<'a> {
    chars: &'a [char],
    values: &'a [(&'a str, &'a str)],
    rules: Rules,
}

impl Formatter<'_> {
    /// Writes the message between `from` and `to`, `#` standing for `number`
    /// inside a plural's branch.
    fn message(&self, from: usize, to: usize, number: Option<&str>, out: &mut String) {
        let mut at = from;
        while at < to {
            match self.chars[at] {
                '\'' => at = self.quoted(at, to, number.is_some(), out),
                '#' if number.is_some() => {
                    out.push_str(&self.rules.number(number.unwrap_or_default()));
                    at += 1;
                }
                '{' => {
                    let close = self.closing(at, to);
                    match close.and_then(|close| self.argument(at + 1, close, out).then_some(close)) {
                        Some(close) => at = close + 1,
                        None => {
                            // Not one it can read: written as it is.
                            let end = close.map_or(to, |close| close + 1);
                            out.extend(&self.chars[at..end]);
                            at = end;
                        }
                    }
                }
                c => {
                    out.push(c);
                    at += 1;
                }
            }
        }
    }

    /// An apostrophe at `at`: two make one, and one before a brace (or `#`
    /// in a plural) quotes up to the next lone one. Returns where to go on.
    fn quoted(&self, at: usize, to: usize, in_plural: bool, out: &mut String) -> usize {
        let next = self.chars.get(at + 1).copied().filter(|_| at + 1 < to);
        match next {
            Some('\'') => {
                out.push('\'');
                at + 2
            }
            Some('{' | '}') | Some('#') if next != Some('#') || in_plural => {
                let mut i = at + 1;
                while i < to {
                    match (self.chars[i], self.chars.get(i + 1)) {
                        ('\'', Some('\'')) if i + 1 < to => {
                            out.push('\'');
                            i += 2;
                        }
                        ('\'', _) => return i + 1,
                        (c, _) => {
                            out.push(c);
                            i += 1;
                        }
                    }
                }
                to
            }
            _ => {
                out.push('\'');
                at + 1
            }
        }
    }

    /// The `}` closing the `{` at `open`, braces nesting.
    fn closing(&self, open: usize, to: usize) -> Option<usize> {
        let mut depth = 0;
        for i in open..to {
            match self.chars[i] {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Writes the argument between the braces; false when it cannot be read
    /// or names no value.
    fn argument(&self, from: usize, to: usize, out: &mut String) -> bool {
        let inside: String = self.chars[from..to].iter().collect();
        let mut parts = inside.splitn(3, ',');
        let name = parts.next().unwrap_or_default().trim();
        let Some(&(_, value)) = self.values.iter().find(|(n, _)| *n == name) else { return false };
        let kind = parts.next().map(str::trim);
        let Some(kind) = kind else {
            out.push_str(value);
            return true;
        };
        // Where the branches start: past the second comma.
        let mut commas = (from..to).filter(|&i| self.chars[i] == ',');
        let branches = commas.nth(1).map(|comma| comma + 1);
        match (kind, branches) {
            ("number", _) => out.push_str(&self.rules.number(value)),
            ("plural", Some(start)) => return self.plural(start, to, value, out),
            ("select", Some(start)) => return self.select(start, to, value, out),
            _ => return false,
        }
        true
    }

    /// A plural's branch for `value`: `=N` for that number, else its
    /// category, else `other`, each written with `#` as the number.
    fn plural(&self, from: usize, to: usize, value: &str, out: &mut String) -> bool {
        let Ok(number) = value.trim().parse::<Decimal>() else { return false };
        let branches = self.branches(from, to);
        let exact = branches.iter().find(|(key, ..)| {
            key.strip_prefix('=').and_then(|n| n.parse::<Decimal>().ok()).is_some_and(|n| n == number)
        });
        let category = self.rules.category(&number);
        let chosen = exact
            .or_else(|| branches.iter().find(|(key, ..)| key == category))
            .or_else(|| branches.iter().find(|(key, ..)| key == "other"));
        let Some((_, start, end)) = chosen else { return false };
        self.message(*start, *end, Some(value), out);
        true
    }

    /// A select's branch for `value`, else `other`.
    fn select(&self, from: usize, to: usize, value: &str, out: &mut String) -> bool {
        let branches = self.branches(from, to);
        let chosen = branches.iter().find(|(key, ..)| key == value.trim());
        let Some((_, start, end)) = chosen.or_else(|| branches.iter().find(|(key, ..)| key == "other")) else {
            return false;
        };
        self.message(*start, *end, None, out);
        true
    }

    /// Each `key {message}` between `from` and `to`: its key, and where its
    /// message starts and ends.
    fn branches(&self, from: usize, to: usize) -> Vec<(String, usize, usize)> {
        let mut found = Vec::new();
        let mut at = from;
        while at < to {
            let Some(open) = (at..to).find(|&i| self.chars[i] == '{') else { break };
            let key: String = self.chars[at..open].iter().collect();
            let Some(close) = self.closing(open, to) else { break };
            let key = key.trim();
            // An offset is not supported, and is passed over.
            let key = key.rsplit(char::is_whitespace).next().unwrap_or(key).to_string();
            found.push((key, open + 1, close));
            at = close + 1;
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COINS: &str = "{player} found {count, plural, =0 {no coins} one {# coin} other {# coins}}";

    #[test]
    fn plurals_follow_each_languages_rules() {
        let coins = |count: &str, language: &str| format(COINS, &[("player", "Ana"), ("count", count)], language);
        assert_eq!(coins("0", "en"), "Ana found no coins", "an exact number first");
        assert_eq!(coins("1", "en"), "Ana found 1 coin");
        assert_eq!(coins("1234", "en"), "Ana found 1,234 coins", "# written as the language writes numbers");
        assert_eq!(coins("1.5", "en"), "Ana found 1.5 coins");

        // Polish has a few for 2 to 4, and many from 5.
        let polish = "{n, plural, one {# moneta} few {# monety} many {# monet} other {# monety}}";
        let pl = |n: &str| format(polish, &[("n", n)], "pl");
        assert_eq!(
            (pl("1"), pl("3"), pl("5"), pl("22")),
            ("1 moneta".into(), "3 monety".into(), "5 monet".into(), "22 monety".into())
        );
        // French counts 0 and 1 as one.
        let french = "{n, plural, one {# pièce} other {# pièces}}";
        assert_eq!(format(french, &[("n", "0")], "fr"), "0 pièce");
    }

    #[test]
    fn numbers_selects_and_quotes() {
        assert_eq!(format("{n, number}", &[("n", "1234.5")], "fr"), "1\u{202f}234,5");
        assert_eq!(format("{n, number}", &[("n", "1234.5")], "de"), "1.234,5");
        let whose = "{g, select, female {her} male {his} other {their}} sword";
        assert_eq!(format(whose, &[("g", "female")], "en"), "her sword");
        assert_eq!(format(whose, &[("g", "robot")], "en"), "their sword");
        assert_eq!(format("'{'braces'}' and it''s {x}", &[("x", "y")], "en"), "{braces} and it's y");
        assert_eq!(
            format("{missing} stays, {n, plural, one {#}}", &[("n", "2")], "en"),
            "{missing} stays, {n, plural, one {#}}"
        );
        let nested = "{g, select, female {{n, plural, one {She has # cat} other {She has # cats}}} other {?}}";
        assert_eq!(format(nested, &[("g", "female"), ("n", "2")], "en"), "She has 2 cats");
    }
}
