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
//! - `{when, date}` is a date, `2026-11-03` or with its time, written as
//!   the language writes dates: `3 Nov 2026` in British English,
//!   `Nov 3, 2026` in American. `{when, date, short}`, `long` and `full`
//!   say how much of it: `03/11/2026`, `3 November 2026`, or with the day
//!   of the week.
//! - `{when, time}` is its time, `14:30` or `2:30 PM` as the language has
//!   it; `{when, time, medium}` with its seconds.
//! - `{price, currency, EUR}` is an amount of that currency, with its
//!   symbol where the language puts it: `€12.50`, `12,50 €`. A value with
//!   its own code, `12.5 EUR`, needs none: `{price, currency}`.
//!
//! The branches nest, and `other` is taken when nothing else fits. An
//! apostrophe quotes a brace or `#` (`'{'`), and two make one (`''`). A
//! name with no value, or a part that cannot be read, is left as written.

use icu_datetime::{
    DateTimeFormatter, fieldsets,
    input::{Date, Time},
};
use icu_decimal::{DecimalFormatter, input::Decimal};
use icu_experimental::dimension::currency::{CurrencyType, formatter::CurrencyFormatter};
use icu_locale_core::Locale;
use icu_plurals::{PluralCategory, PluralRules};

/// The plural rules and number formatting of a language, made once for a
/// text; dates and money are made as they are asked for.
struct Rules {
    locale: Locale,
    plurals: Option<PluralRules>,
    numbers: Option<DecimalFormatter>,
}

impl Rules {
    fn of(language: &str) -> Self {
        let locale = language.parse::<Locale>().unwrap_or(Locale::UNKNOWN);
        Self {
            plurals: PluralRules::try_new_cardinal((&locale).into()).ok(),
            numbers: DecimalFormatter::try_new((&locale).into(), Default::default()).ok(),
            locale,
        }
    }

    /// A date in `style` (short, medium, long or full); none when the value
    /// is not a date.
    fn date(&self, value: &str, style: &str) -> Option<String> {
        let (date, _) = when(value)?;
        let prefs = (&self.locale).into();
        let text = match style {
            "short" => DateTimeFormatter::try_new(prefs, fieldsets::YMD::short()).ok()?.format(&date).to_string(),
            "long" => DateTimeFormatter::try_new(prefs, fieldsets::YMD::long()).ok()?.format(&date).to_string(),
            "full" => DateTimeFormatter::try_new(prefs, fieldsets::YMDE::long()).ok()?.format(&date).to_string(),
            _ => DateTimeFormatter::try_new(prefs, fieldsets::YMD::medium()).ok()?.format(&date).to_string(),
        };
        Some(text)
    }

    /// A time, with its seconds in the medium style; none when the value
    /// has no time.
    fn time(&self, value: &str, style: &str) -> Option<String> {
        let time = when(value)?.1?;
        let prefs = (&self.locale).into();
        let text = match style {
            "medium" | "long" | "full" => {
                DateTimeFormatter::try_new(prefs, fieldsets::T::hms()).ok()?.format(&time).to_string()
            }
            _ => DateTimeFormatter::try_new(prefs, fieldsets::T::hm()).ok()?.format(&time).to_string(),
        };
        Some(text)
    }

    /// An amount of a currency, by its ISO code, given or after the amount;
    /// none when either cannot be read.
    fn currency(&self, value: &str, code: Option<&str>) -> Option<String> {
        let mut words = value.split_whitespace();
        let amount = words.next()?.parse::<Decimal>().ok()?;
        let code = code.or(words.next())?.trim().to_ascii_lowercase();
        let currency = CurrencyType::try_from_str(&code).ok()?;
        let formatter = CurrencyFormatter::try_new_symbol((&self.locale).into(), currency, Default::default()).ok()?;
        Some(formatter.format_fixed_decimal(&amount).to_string())
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

/// A date, `2026-11-03`, and its time when it has one, `2026-11-03T14:30`
/// or with seconds, as ISO 8601 writes them; a time alone, `14:30`, is on
/// no date in particular.
fn when(value: &str) -> Option<(Date<icu_calendar::Iso>, Option<Time>)> {
    let value = value.trim();
    let (day, clock) = match value.split_once(['T', ' ']) {
        Some((day, clock)) => (day, Some(clock)),
        None if value.contains(':') => ("2000-01-01", Some(value)),
        None => (value, None),
    };
    let mut parts = day.splitn(3, '-').map(str::parse::<i32>);
    let (year, month, date) = (parts.next()?.ok()?, parts.next()?.ok()?, parts.next()?.ok()?);
    let date = Date::try_new_iso(year, u8::try_from(month).ok()?, u8::try_from(date).ok()?).ok()?;
    let time = match clock {
        Some(clock) => {
            // Seconds, and any part of one or zone after them, are read as whole seconds.
            let clock = clock.trim_end_matches('Z');
            let mut parts = clock.split(':').map(|part| part.split(['.', '+', '-']).next().unwrap_or("").parse::<u8>());
            let (hour, minute) = (parts.next()?.ok()?, parts.next()?.ok()?);
            let second = parts.next().and_then(Result::ok).unwrap_or(0);
            Some(Time::try_new(hour, minute, second, 0).ok()?)
        }
        None => None,
    };
    Some((date, time))
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
        // A style after the kind, as `short` in `{when, date, short}`.
        let style = parts.next().map(str::trim).unwrap_or_default();
        let written = match kind {
            "date" => Some(self.rules.date(value, style)),
            "time" => Some(self.rules.time(value, style)),
            "currency" => Some(self.rules.currency(value, Some(style).filter(|code| !code.is_empty()))),
            _ => None,
        };
        if let Some(written) = written {
            let Some(text) = written else { return false };
            out.push_str(&text);
            return true;
        }
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

    #[test]
    fn dates_times_and_money_are_written_as_each_language_writes_them() {
        let when = [("when", "2026-11-03T14:30:05")];
        assert_eq!(format("{when, date}", &when, "en-GB"), "3 Nov 2026");
        assert_eq!(format("{when, date}", &when, "en-US"), "Nov 3, 2026");
        assert_eq!(format("{when, date, short}", &when, "en-GB"), "03/11/2026");
        assert_eq!(format("{when, date, long}", &when, "de"), "3. November 2026");
        assert!(format("{when, date, full}", &when, "en-GB").starts_with("Tuesday"), "with the day of the week");
        assert_eq!(format("{when, time}", &when, "en-GB"), "14:30");
        assert!(format("{when, time}", &when, "en-US").starts_with("2:30"), "on a twelve-hour clock");
        assert_eq!(format("{when, time, medium}", &when, "fr"), "14:30:05");
        assert_eq!(format("{t, time}", &[("t", "09:05")], "en-GB"), "09:05", "a time alone");

        assert_eq!(format("{p, currency, EUR}", &[("p", "12.5")], "en"), "€12.50");
        assert_eq!(format("{p, currency}", &[("p", "1234.5 EUR")], "de"), "1.234,50\u{a0}€");
        assert_eq!(format("{p, currency, JPY}", &[("p", "1200")], "ja"), "￥1,200");
        assert_eq!(format("{p, currency}", &[("p", "12")], "en"), "{p, currency}", "no code, left as written");
        assert_eq!(format("{d, date}", &[("d", "yesterday")], "en"), "{d, date}", "not a date");
    }
}
