//! The properties a style sheet can set, with CSS's names and values.
//!
//! Layout: `display`, `position`, `left`/`top`/`right`/`bottom`, `width`,
//! `height` and their `min-`/`max-` forms, `margin`, `padding`, `gap`,
//! `flex`, `flex-direction`, `flex-grow`, `flex-shrink`, `flex-basis`,
//! `flex-wrap`, `align-items`, `align-self`, `align-content` and
//! `justify-content`. Look: `background-color`, `border`, `border-width`,
//! `border-color`, `border-radius`, `opacity`, `pointer-events`, `overflow`
//! (`visible`, `hidden`, or `scroll`/`auto`) and `scrollbar-color`. Text,
//! which children inherit: `color`, `font-size`, `font-weight` and
//! `font-family` (`sans` or `mono`).
//!
//! Lengths are `px` or `%`, colours are `#rgb`, `#rrggbb`, `#rrggbbaa`,
//! `rgb()`, `rgba()`, `oklch()` or a few names, and `var(--name)` or
//! `var(--name, fallback)` reads a custom property.

use std::collections::HashMap;

use cssparser::{ParseError, Parser, ParserInput, Token};
use taffy::{
    AlignContent, AlignItems, Dimension, Display, FlexDirection, FlexWrap, LengthPercentage, LengthPercentageAuto,
    Position, Rect, Size,
};

use crate::{Color, Family, Overflow, Style, TextStyle};

/// Applies one declaration, after replacing its variables. Errors say what
/// was wrong, for the caller to report with the declaration's place.
pub(crate) fn apply(
    style: &mut Style,
    text: &mut TextStyle,
    name: &str,
    value: &str,
    vars: &HashMap<String, String>,
) -> Result<(), String> {
    let value = substitute(value, vars, 0)?;
    let mut input = ParserInput::new(&value);
    let mut parser = Parser::new(&mut input);
    let parsed = set(style, text, name, &mut parser);
    let parsed = parsed.and_then(|known| {
        if known {
            parser.expect_exhausted().map_err(|_| format!("'{value}' has more than {name} takes"))?;
        }
        Ok(known)
    });
    match parsed {
        Ok(true) => Ok(()),
        Ok(false) => Err(format!("unknown property '{name}'")),
        Err(message) => Err(format!("{name}: {message}")),
    }
}

/// Replaces each `var(--name)` or `var(--name, fallback)` with its value.
fn substitute(value: &str, vars: &HashMap<String, String>, depth: u32) -> Result<String, String> {
    if depth > 8 {
        return Err("variables refer to each other in a loop".into());
    }
    let Some(start) = value.find("var(") else { return Ok(value.to_string()) };
    let inner_start = start + 4;
    let mut level = 1;
    let end = value[inner_start..]
        .char_indices()
        .find(|&(_, c)| {
            level += match c {
                '(' => 1,
                ')' => -1,
                _ => 0,
            };
            level == 0
        })
        .map(|(i, _)| inner_start + i)
        .ok_or("var( is not closed")?;
    let inner = &value[inner_start..end];
    let (name, fallback) = match inner.split_once(',') {
        Some((name, fallback)) => (name.trim(), Some(fallback.trim())),
        None => (inner.trim(), None),
    };
    let replacement = match (vars.get(name), fallback) {
        (Some(found), _) => found.as_str(),
        (None, Some(fallback)) => fallback,
        (None, None) => return Err(format!("{name} is not defined")),
    };
    let replaced = format!("{}{}{}", &value[..start], replacement, &value[end + 1..]);
    substitute(&replaced, vars, depth + 1)
}

type Parsed<T> = Result<T, String>;

/// Sets `name` from the input. `Ok(false)` means no such property.
fn set(style: &mut Style, text: &mut TextStyle, name: &str, input: &mut Parser) -> Parsed<bool> {
    let layout = &mut style.layout;
    match name {
        "display" => layout.display = keyword(input, &[("flex", Display::Flex), ("none", Display::None)])?,
        "position" => {
            let positions =
                [("relative", Position::Relative), ("absolute", Position::Absolute), ("static", Position::Static)];
            layout.position = keyword(input, &positions)?
        }
        "left" => layout.inset.left = length_auto(input)?,
        "top" => layout.inset.top = length_auto(input)?,
        "right" => layout.inset.right = length_auto(input)?,
        "bottom" => layout.inset.bottom = length_auto(input)?,
        "width" => layout.size.width = dimension(input)?,
        "height" => layout.size.height = dimension(input)?,
        "min-width" => layout.min_size.width = length_auto(input)?,
        "min-height" => layout.min_size.height = length_auto(input)?,
        "max-width" => layout.max_size.width = length_auto(input)?,
        "max-height" => layout.max_size.height = length_auto(input)?,
        "margin" => layout.margin = sides(input, length_auto)?,
        "margin-left" => layout.margin.left = length_auto(input)?,
        "margin-top" => layout.margin.top = length_auto(input)?,
        "margin-right" => layout.margin.right = length_auto(input)?,
        "margin-bottom" => layout.margin.bottom = length_auto(input)?,
        "padding" => layout.padding = sides(input, length)?,
        "padding-left" => layout.padding.left = length(input)?,
        "padding-top" => layout.padding.top = length(input)?,
        "padding-right" => layout.padding.right = length(input)?,
        "padding-bottom" => layout.padding.bottom = length(input)?,
        "gap" => {
            let row = length(input)?;
            let column = input.try_parse(|i| length(i).map_err(|_| i.new_error_for_next_token::<()>())).unwrap_or(row);
            layout.gap = Size { width: column, height: row };
        }
        "row-gap" => layout.gap.height = length(input)?,
        "column-gap" => layout.gap.width = length(input)?,
        "flex" => flex(style, input)?,
        "flex-direction" => {
            let directions = [
                ("row", FlexDirection::Row),
                ("column", FlexDirection::Column),
                ("row-reverse", FlexDirection::RowReverse),
                ("column-reverse", FlexDirection::ColumnReverse),
            ];
            layout.flex_direction = keyword(input, &directions)?;
        }
        "flex-grow" => layout.flex_grow = number(input)?,
        "flex-shrink" => layout.flex_shrink = number(input)?,
        "flex-basis" => layout.flex_basis = dimension(input)?,
        "flex-wrap" => layout.flex_wrap = keyword(input, &[("nowrap", FlexWrap::NoWrap), ("wrap", FlexWrap::Wrap)])?,
        "align-items" => layout.align_items = keyword(input, ALIGN_ITEMS)?,
        "align-self" => layout.align_self = Some(keyword(input, ALIGN_ITEMS)?),
        "align-content" => layout.align_content = keyword(input, ALIGN_CONTENT)?,
        "justify-content" => layout.justify_content = keyword(input, ALIGN_CONTENT)?,
        "background-color" => style.background = Some(color(input)?),
        "border" => {
            let width = pixels(input)?;
            // `solid` and the like are accepted and mean nothing: borders are solid.
            let _ = input.try_parse(|i| i.expect_ident_matching("solid"));
            border(style, width, Some(color(input)?));
        }
        "border-width" => border(style, pixels(input)?, None),
        "border-color" => {
            let width = style.border.map_or(1.0, |(w, _)| w);
            border(style, width, Some(color(input)?));
        }
        "border-radius" => style.radius = pixels(input)?,
        "opacity" => style.opacity = number(input)?.clamp(0.0, 1.0),
        "pointer-events" => style.pickable = keyword(input, &[("auto", true), ("none", false)])?,
        "overflow" => {
            let modes = [
                ("visible", Overflow::Visible),
                ("hidden", Overflow::Hidden),
                ("scroll", Overflow::Scroll),
                ("auto", Overflow::Scroll),
            ];
            style.overflow = keyword(input, &modes)?;
        }
        "scrollbar-color" => style.scrollbar = color(input)?,
        "color" => text.color = color(input)?,
        "font-size" => text.size = pixels(input)?,
        "font-weight" => {
            let named = [("normal", 400.0), ("medium", 500.0), ("semibold", 600.0), ("bold", 700.0)];
            text.weight = input
                .try_parse(|i| keyword(i, &named).map_err(|_| i.new_error_for_next_token::<()>()))
                .or_else(|_| number(input))?;
        }
        "font-family" => {
            let families = [
                ("sans", Family::Sans),
                ("sans-serif", Family::Sans),
                ("mono", Family::Mono),
                ("monospace", Family::Mono),
            ];
            text.family = keyword(input, &families)?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

const ALIGN_ITEMS: &[(&str, AlignItems)] = &[
    ("flex-start", AlignItems::FLEX_START),
    ("start", AlignItems::START),
    ("flex-end", AlignItems::FLEX_END),
    ("end", AlignItems::END),
    ("center", AlignItems::CENTER),
    ("stretch", AlignItems::STRETCH),
    ("baseline", AlignItems::BASELINE),
];

const ALIGN_CONTENT: &[(&str, AlignContent)] = &[
    ("flex-start", AlignContent::FLEX_START),
    ("start", AlignContent::START),
    ("flex-end", AlignContent::FLEX_END),
    ("end", AlignContent::END),
    ("center", AlignContent::CENTER),
    ("stretch", AlignContent::STRETCH),
    ("space-between", AlignContent::SPACE_BETWEEN),
    ("space-around", AlignContent::SPACE_AROUND),
    ("space-evenly", AlignContent::SPACE_EVENLY),
];

/// A border is drawn and also insets the content, as in CSS.
fn border(style: &mut Style, width: f32, color: Option<Color>) {
    let color = color.or(style.border.map(|(_, c)| c)).unwrap_or(Color::TRANSPARENT);
    style.border = Some((width, color));
    style.layout.border = Rect::length(width);
}

fn keyword<T: Copy>(input: &mut Parser, choices: &[(&str, T)]) -> Parsed<T> {
    let ident = input.expect_ident().map_err(|_| "expected a keyword".to_string())?.to_ascii_lowercase();
    choices.iter().find(|(name, _)| *name == ident).map(|(_, value)| *value).ok_or_else(|| {
        let names: Vec<&str> = choices.iter().map(|(name, _)| *name).collect();
        format!("'{ident}' is not one of {}", names.join(", "))
    })
}

fn number(input: &mut Parser) -> Parsed<f32> {
    input.expect_number().map_err(|_| "expected a number".to_string())
}

fn pixels(input: &mut Parser) -> Parsed<f32> {
    match input.next().map_err(|_| "expected a length".to_string())? {
        Token::Dimension { value, unit, .. } if unit.eq_ignore_ascii_case("px") => Ok(*value),
        Token::Number { value, .. } if *value == 0.0 => Ok(0.0),
        other => Err(format!("expected a length in px, not {other:?}")),
    }
}

/// A length or percentage, which is what most sizes are.
enum Amount {
    Px(f32),
    Percent(f32),
    Auto,
}

fn amount(input: &mut Parser, auto: bool) -> Parsed<Amount> {
    match input.next().map_err(|_| "expected a length".to_string())?.clone() {
        Token::Dimension { value, unit, .. } if unit.eq_ignore_ascii_case("px") => Ok(Amount::Px(value)),
        Token::Percentage { unit_value, .. } => Ok(Amount::Percent(unit_value)),
        Token::Number { value: 0.0, .. } => Ok(Amount::Px(0.0)),
        Token::Ident(ident) if auto && ident.eq_ignore_ascii_case("auto") => Ok(Amount::Auto),
        other => Err(format!("expected a length in px or %, not {other:?}")),
    }
}

fn dimension(input: &mut Parser) -> Parsed<Dimension> {
    Ok(match amount(input, true)? {
        Amount::Px(px) => Dimension::length(px),
        Amount::Percent(fraction) => Dimension::percent(fraction),
        Amount::Auto => Dimension::auto(),
    })
}

fn length_auto(input: &mut Parser) -> Parsed<LengthPercentageAuto> {
    Ok(match amount(input, true)? {
        Amount::Px(px) => LengthPercentageAuto::length(px),
        Amount::Percent(fraction) => LengthPercentageAuto::percent(fraction),
        Amount::Auto => LengthPercentageAuto::auto(),
    })
}

fn length(input: &mut Parser) -> Parsed<LengthPercentage> {
    Ok(match amount(input, false)? {
        Amount::Px(px) => LengthPercentage::length(px),
        Amount::Percent(fraction) => LengthPercentage::percent(fraction),
        Amount::Auto => unreachable!("auto is not asked for"),
    })
}

/// One to four values, as `margin` and `padding` take them: all sides; top
/// and bottom then left and right; top, sides, bottom; or each clockwise
/// from the top.
fn sides<T: Copy>(input: &mut Parser, one: fn(&mut Parser) -> Parsed<T>) -> Parsed<Rect<T>> {
    let mut values = vec![one(input)?];
    while values.len() < 4 && !input.is_exhausted() {
        values.push(one(input)?);
    }
    let (top, right, bottom, left) = match values[..] {
        [all] => (all, all, all, all),
        [vertical, horizontal] => (vertical, horizontal, vertical, horizontal),
        [top, horizontal, bottom] => (top, horizontal, bottom, horizontal),
        [top, right, bottom, left] => (top, right, bottom, left),
        _ => unreachable!("one to four values"),
    };
    Ok(Rect { left, right, top, bottom })
}

/// `flex: none`, `flex: auto`, or a grow factor with an optional shrink
/// factor and basis. `flex: 1` grows from nothing, as in CSS.
fn flex(style: &mut Style, input: &mut Parser) -> Parsed<()> {
    let layout = &mut style.layout;
    if input.try_parse(|i| i.expect_ident_matching("none")).is_ok() {
        (layout.flex_grow, layout.flex_shrink, layout.flex_basis) = (0.0, 0.0, Dimension::auto());
        return Ok(());
    }
    if input.try_parse(|i| i.expect_ident_matching("auto")).is_ok() {
        (layout.flex_grow, layout.flex_shrink, layout.flex_basis) = (1.0, 1.0, Dimension::auto());
        return Ok(());
    }
    layout.flex_grow = number(input)?;
    layout.flex_shrink = input.try_parse(|i| i.expect_number()).unwrap_or(1.0);
    layout.flex_basis = if input.is_exhausted() { Dimension::percent(0.0) } else { dimension(input)? };
    Ok(())
}

pub(crate) fn color(input: &mut Parser) -> Parsed<Color> {
    let location = input.current_source_location();
    let token = input.next().map_err(|_| "expected a colour".to_string())?.clone();
    match token {
        Token::Hash(hex) | Token::IDHash(hex) => hex_color(&hex),
        Token::Ident(name) => named_color(&name),
        Token::Function(name) => {
            let name = name.to_ascii_lowercase();
            let values = input
                .parse_nested_block(|i| color_arguments(i).map_err(|e| location.new_custom_error::<String, String>(e)));
            let values = values.map_err(|e: ParseError<String>| match e.kind {
                cssparser::ParseErrorKind::Custom(message) => message,
                _ => "could not read the colour".to_string(),
            })?;
            function_color(&name, &values)
        }
        other => Err(format!("expected a colour, not {other:?}")),
    }
}

fn hex_color(hex: &str) -> Parsed<Color> {
    let digits: Vec<u8> =
        hex.chars().map(|c| c.to_digit(16).map(|d| d as u8)).collect::<Option<_>>().ok_or("not hex")?;
    let pair = |i: usize| digits[i] * 16 + digits[i + 1];
    let single = |i: usize| digits[i] * 17;
    Ok(match digits.len() {
        3 => Color::rgb(single(0), single(1), single(2)),
        4 => Color::rgba(single(0), single(1), single(2), single(3)),
        6 => Color::rgb(pair(0), pair(2), pair(4)),
        8 => Color::rgba(pair(0), pair(2), pair(4), pair(6)),
        _ => return Err(format!("#{hex} needs 3, 4, 6 or 8 hex digits")),
    })
}

fn named_color(name: &str) -> Parsed<Color> {
    Ok(match name.to_ascii_lowercase().as_str() {
        "transparent" => Color::TRANSPARENT,
        "white" => Color::WHITE,
        "black" => Color::BLACK,
        other => return Err(format!("unknown colour '{other}'")),
    })
}

/// A colour function's numbers: commas or spaces between them, with alpha
/// after `/` in the modern form. Percentages become fractions.
fn color_arguments(input: &mut Parser) -> Result<Vec<(f32, bool)>, String> {
    let mut values = Vec::new();
    while !input.is_exhausted() {
        match input.next().map_err(|_| "could not read the colour".to_string())? {
            Token::Number { value, .. } => values.push((*value, false)),
            Token::Percentage { unit_value, .. } => values.push((*unit_value, true)),
            Token::Dimension { value, unit, .. } if unit.eq_ignore_ascii_case("deg") => values.push((*value, false)),
            Token::Comma | Token::Delim('/') => {}
            other => return Err(format!("unexpected {other:?} in a colour")),
        }
    }
    Ok(values)
}

fn function_color(name: &str, values: &[(f32, bool)]) -> Parsed<Color> {
    let alpha = |index: usize| values.get(index).map_or(1.0, |&(v, _)| v.clamp(0.0, 1.0));
    match (name, values.len()) {
        ("rgb" | "rgba", 3 | 4) => {
            let channel =
                |(v, percent): (f32, bool)| (if percent { v * 255.0 } else { v }).round().clamp(0.0, 255.0) as u8;
            let [r, g, b] = [values[0], values[1], values[2]].map(channel);
            Ok(Color::rgba(r, g, b, (alpha(3) * 255.0).round() as u8))
        }
        ("oklch", 3 | 4) => Ok(Color::oklch(values[0].0, values[1].0, values[2].0, alpha(3))),
        (name, count) => Err(format!("{name}() with {count} values is not a colour Voxear knows")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn applied(name: &str, value: &str) -> Result<(Style, TextStyle), String> {
        let (mut style, mut text) = (Style::default(), TextStyle::default());
        let vars =
            HashMap::from([("--accent".to_string(), "#8b93ff".to_string()), ("--gap".to_string(), "6px".to_string())]);
        apply(&mut style, &mut text, name, value, &vars)?;
        Ok((style, text))
    }

    #[test]
    fn colours_read_in_every_form() {
        let color = |value: &str| applied("background-color", value).unwrap().0.background.unwrap();
        assert_eq!(color("#111216"), Color::hex(0x111216));
        assert_eq!(color("#fff"), Color::WHITE);
        assert_eq!(color("#11121680"), Color::hex(0x111216).with_alpha(0x80));
        assert_eq!(color("rgb(17, 18, 22)"), Color::hex(0x111216));
        assert_eq!(color("rgba(255 0 0 / 50%)"), Color::rgba(255, 0, 0, 128));
        assert_eq!(color("var(--accent)"), Color::hex(0x8b93ff));
        assert_eq!(color("oklch(100% 0 0)"), Color::WHITE);
        assert_eq!(color("oklch(0% 0 0)"), Color::BLACK);
        let accent = color("oklch(70% 0.14 275)");
        assert!(accent.b > accent.g && accent.b > accent.r, "a blue-violet, got {accent:?}");
        assert!(applied("color", "#12").is_err() && applied("color", "teal-ish").is_err());
    }

    #[test]
    fn lengths_sides_and_keywords() {
        let (style, _) = applied("padding", "4px 8px").unwrap();
        assert_eq!(
            style.layout.padding,
            Rect {
                top: LengthPercentage::length(4.0),
                bottom: LengthPercentage::length(4.0),
                left: LengthPercentage::length(8.0),
                right: LengthPercentage::length(8.0)
            }
        );
        let (style, _) = applied("width", "50%").unwrap();
        assert_eq!(style.layout.size.width, Dimension::percent(0.5));
        let (style, _) = applied("gap", "var(--gap)").unwrap();
        assert_eq!(style.layout.gap.width, LengthPercentage::length(6.0));
        let (style, _) = applied("border", "1px solid #2a2b30").unwrap();
        assert_eq!(style.border, Some((1.0, Color::hex(0x2a2b30))));
        assert_eq!(style.layout.border.left, LengthPercentage::length(1.0), "borders inset the content");
        let (style, _) = applied("flex", "1").unwrap();
        assert_eq!((style.layout.flex_grow, style.layout.flex_basis), (1.0, Dimension::percent(0.0)));
        let (_, text) = applied("font-weight", "semibold").unwrap();
        assert_eq!(text.weight, 600.0);
        let (_, text) = applied("font-family", "mono").unwrap();
        assert_eq!(text.family, Family::Mono);
    }

    #[test]
    fn mistakes_say_what_was_wrong() {
        assert_eq!(applied("colour", "red").unwrap_err(), "unknown property 'colour'");
        assert!(applied("display", "grid").unwrap_err().contains("not one of flex, none"));
        assert!(applied("width", "12em").unwrap_err().contains("px or %"));
        assert!(applied("color", "var(--missing)").unwrap_err().contains("--missing is not defined"));
        assert_eq!(applied("color", "var(--missing, #fff)").unwrap().1.color, Color::WHITE);
        assert!(applied("width", "10px 20px").unwrap_err().contains("more than width takes"));
    }
}
