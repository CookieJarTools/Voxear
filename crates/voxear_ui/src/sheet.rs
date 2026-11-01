//! Style sheets (`.vss`): CSS syntax, with USS's selectors and the
//! properties Voxear UI knows (see [`crate::properties`]). Custom properties
//! (`--accent: #8b93ff`) are the theme tokens, read back with `var()`.
//!
//! `@keyframes name { from { ... } 50% { ... } to { ... } }` names keyframes
//! for the `animation` property to play.
//!
//! A rule or declaration that does not parse is skipped with a warning, as a
//! browser would, so one typo never loses a whole sheet.

use cssparser::{
    AtRuleParser, CowRcStr, DeclarationParser, ParseError, ParseErrorKind, Parser, ParserInput, ParserState,
    QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, StyleSheetParser, Token,
};

use crate::selector::{Combinator, Compound, Selector, States};

/// A parsed `.vss` file, attached to an element with
/// [`Ui::add_sheet`](crate::Ui::add_sheet) to style it and everything in it.
#[derive(Debug, Default)]
pub struct StyleSheet {
    pub(crate) source: String,
    pub(crate) rules: Vec<Rule>,
    pub(crate) keyframes: Vec<Keyframes>,
    warnings: Vec<String>,
}

/// `@keyframes name { ... }`: each stop's place from 0 to 1, and its
/// declarations, in the order written.
#[derive(Debug)]
pub(crate) struct Keyframes {
    pub name: String,
    pub stops: Vec<(f32, Vec<Declaration>)>,
}

/// What the top of a sheet holds.
enum Item {
    Rule(Rule),
    Keyframes(Keyframes),
}

#[derive(Debug)]
pub(crate) struct Rule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
}

/// `name: value`, with the value kept as written until the cascade knows
/// which variables it refers to.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Declaration {
    pub name: String,
    pub value: String,
    /// One-based, for messages.
    pub line: u32,
}

impl StyleSheet {
    /// Parses a sheet; `source` names it in warnings, such as
    /// `Stats.vss:12: unknown property 'colour'`.
    pub fn parse(source: &str, text: &str) -> Self {
        let mut input = ParserInput::new(text);
        let mut parser = Parser::new(&mut input);
        let mut rules = TopLevel { source, warnings: Vec::new() };
        let (mut parsed, mut keyframes) = (Vec::new(), Vec::new());
        let mut skipped = Vec::new();
        for result in StyleSheetParser::new(&mut parser, &mut rules) {
            match result {
                Ok(Item::Rule(rule)) => parsed.push(rule),
                Ok(Item::Keyframes(named)) => keyframes.push(named),
                Err((error, text)) => {
                    let message = describe(&error, text);
                    skipped.push(format!("{source}:{}: {message}", error.location.line + 1));
                }
            }
        }
        let mut warnings = rules.warnings;
        warnings.extend(skipped);
        Self { source: source.to_string(), rules: parsed, keyframes, warnings }
    }

    /// What was skipped while parsing.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
}

fn describe(error: &ParseError<String>, text: &str) -> String {
    match &error.kind {
        ParseErrorKind::Custom(message) => message.clone(),
        ParseErrorKind::Basic(_) => format!("could not read '{}'", text.trim()),
    }
}

/// Parses rules at the top of a sheet, and `@keyframes`; other at-rules
/// have no meaning yet.
struct TopLevel<'a> {
    source: &'a str,
    warnings: Vec<String>,
}

impl<'i> QualifiedRuleParser<'i> for TopLevel<'_> {
    type Prelude = Vec<Selector>;
    type QualifiedRule = Item;
    type Error = String;

    fn parse_prelude<'t>(&mut self, input: &mut Parser<'i, 't>) -> Result<Vec<Selector>, ParseError<'i, String>> {
        input.parse_comma_separated(parse_selector)
    }

    fn parse_block<'t>(
        &mut self,
        selectors: Vec<Selector>,
        _start: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<Item, ParseError<'i, String>> {
        let declarations = self.declarations(input);
        Ok(Item::Rule(Rule { selectors, declarations }))
    }
}

impl TopLevel<'_> {
    /// A block's declarations, each that does not parse warned of.
    fn declarations(&mut self, input: &mut Parser) -> Vec<Declaration> {
        let mut declarations = Vec::new();
        for result in RuleBodyParser::new(input, &mut Declarations) {
            match result {
                Ok(declaration) => declarations.push(declaration),
                Err((error, text)) => {
                    let message = describe(&error, text);
                    self.warnings.push(format!("{}:{}: {message}", self.source, error.location.line + 1));
                }
            }
        }
        declarations
    }
}

impl<'i> AtRuleParser<'i> for TopLevel<'_> {
    /// The keyframes' name.
    type Prelude = String;
    type AtRule = Item;
    type Error = String;

    fn parse_prelude<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
    ) -> Result<String, ParseError<'i, String>> {
        if !name.eq_ignore_ascii_case("keyframes") {
            return Err(input.new_custom_error(format!("@{name} is not something a sheet can hold")));
        }
        let named = input.expect_ident_cloned().map_err(|_| input.new_custom_error("@keyframes needs a name"))?;
        Ok(named.to_string())
    }

    fn parse_block<'t>(
        &mut self,
        name: String,
        _start: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<Item, ParseError<'i, String>> {
        let (mut stops, mut skipped) = (Vec::new(), Vec::new());
        let mut parser = Stops { top: self };
        for result in RuleBodyParser::new(input, &mut parser) {
            match result {
                Ok(stop) => stops.extend(stop),
                Err((error, text)) => skipped.push((error.location.line + 1, describe(&error, text))),
            }
        }
        for (line, message) in skipped {
            self.warnings.push(format!("{}:{line}: {message}", self.source));
        }
        stops.sort_by(|a: &(f32, Vec<Declaration>), b| a.0.total_cmp(&b.0));
        Ok(Item::Keyframes(Keyframes { name, stops }))
    }
}

/// Parses `from`, `to` and `50%` stops in a `@keyframes`, each with its
/// declarations. A stop listing several places (`0%, 100%`) is each of them.
struct Stops<'a, 'b> {
    top: &'a mut TopLevel<'b>,
}

impl<'i> QualifiedRuleParser<'i> for Stops<'_, '_> {
    type Prelude = Vec<f32>;
    type QualifiedRule = Vec<(f32, Vec<Declaration>)>;
    type Error = String;

    fn parse_prelude<'t>(&mut self, input: &mut Parser<'i, 't>) -> Result<Vec<f32>, ParseError<'i, String>> {
        input.parse_comma_separated(|input| {
            let location = input.current_source_location();
            match input.next()?.clone() {
                Token::Ident(word) if word.eq_ignore_ascii_case("from") => Ok(0.0),
                Token::Ident(word) if word.eq_ignore_ascii_case("to") => Ok(1.0),
                Token::Percentage { unit_value, .. } if (0.0..=1.0).contains(&unit_value) => Ok(unit_value),
                other => Err(location.new_custom_error(format!("{other:?} is not a keyframe; use from, to or a %"))),
            }
        })
    }

    fn parse_block<'t>(
        &mut self,
        places: Vec<f32>,
        _start: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<Vec<(f32, Vec<Declaration>)>, ParseError<'i, String>> {
        let declarations = self.top.declarations(input);
        Ok(places.into_iter().map(|place| (place, declarations.clone())).collect())
    }
}

impl<'i> AtRuleParser<'i> for Stops<'_, '_> {
    type Prelude = ();
    type AtRule = Vec<(f32, Vec<Declaration>)>;
    type Error = String;
}

impl<'i> DeclarationParser<'i> for Stops<'_, '_> {
    type Declaration = Vec<(f32, Vec<Declaration>)>;
    type Error = String;
}

impl<'i> RuleBodyItemParser<'i, Vec<(f32, Vec<Declaration>)>, String> for Stops<'_, '_> {
    fn parse_declarations(&self) -> bool {
        false
    }

    fn parse_qualified(&self) -> bool {
        true
    }
}

/// Parses a rule's declarations, keeping each value's text.
struct Declarations;

impl<'i> DeclarationParser<'i> for Declarations {
    type Declaration = Declaration;
    type Error = String;

    fn parse_value<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
        start: &ParserState,
    ) -> Result<Declaration, ParseError<'i, String>> {
        let begin = input.position();
        while input.next().is_ok() {}
        let value = input.slice_from(begin).trim();
        let value = value.strip_suffix("!important").unwrap_or(value).trim();
        Ok(Declaration {
            name: name.to_ascii_lowercase(),
            value: value.to_string(),
            line: start.source_location().line + 1,
        })
    }
}

impl<'i> AtRuleParser<'i> for Declarations {
    type Prelude = ();
    type AtRule = Declaration;
    type Error = String;
}

impl<'i> QualifiedRuleParser<'i> for Declarations {
    type Prelude = ();
    type QualifiedRule = Declaration;
    type Error = String;
}

impl<'i> RuleBodyItemParser<'i, Declaration, String> for Declarations {
    fn parse_declarations(&self) -> bool {
        true
    }

    fn parse_qualified(&self) -> bool {
        false
    }
}

/// Parses declarations written in a `style` attribute, which have no
/// selector or braces.
pub(crate) fn parse_inline(source: &str, text: &str, warnings: &mut Vec<String>) -> Vec<Declaration> {
    let mut input = ParserInput::new(text);
    let mut parser = Parser::new(&mut input);
    let mut declarations = Vec::new();
    for result in RuleBodyParser::new(&mut parser, &mut Declarations) {
        match result {
            Ok(declaration) => declarations.push(declaration),
            Err((error, text)) => warnings.push(format!("{source}: {}", describe(&error, text))),
        }
    }
    declarations
}

/// One selector of a comma-separated list, such as `.panel > Label:hover`.
fn parse_selector<'i>(input: &mut Parser<'i, '_>) -> Result<Selector, ParseError<'i, String>> {
    let mut compounds = vec![Compound::default()];
    let mut combinators = Vec::new();
    // A combinator seen since the last compound, which the next one starts with.
    let mut pending: Option<Combinator> = None;
    loop {
        let location = input.current_source_location();
        let Ok(token) = input.next_including_whitespace().cloned() else { break };
        let current_empty = compounds.last().is_none_or(|c| *c == Compound::default());
        match token {
            Token::WhiteSpace(_) => {
                if !current_empty {
                    pending.get_or_insert(Combinator::Descendant);
                }
                continue;
            }
            Token::Delim('>') => {
                pending = Some(Combinator::Child);
                continue;
            }
            _ => {}
        }
        if let Some(combinator) = pending.take() {
            combinators.push(combinator);
            compounds.push(Compound::default());
        }
        let compound = compounds.last_mut().expect("starts with one");
        match token {
            Token::Ident(tag) => compound.tag = Some(tag.to_string()),
            Token::Delim('*') => {}
            Token::IDHash(name) | Token::Hash(name) => compound.name = Some(name.to_string()),
            Token::Delim('.') => match input.next_including_whitespace() {
                Ok(Token::Ident(class)) => compound.classes.push(class.to_string()),
                _ => return Err(location.new_custom_error("a class name must follow '.'")),
            },
            Token::Colon => {
                let state = match input.next_including_whitespace() {
                    Ok(Token::Ident(state)) => state.to_ascii_lowercase(),
                    _ => return Err(location.new_custom_error("a state must follow ':'")),
                };
                if state == "root" {
                    compound.root = true;
                } else {
                    let Some(state) = States::named(&state) else {
                        let message = format!(
                            "unknown state ':{state}'; expected hover, active, focus, disabled, checked, selected or root"
                        );
                        return Err(location.new_custom_error(message));
                    };
                    compound.states.set(state, true);
                }
            }
            other => return Err(location.new_custom_error(format!("unexpected {other:?} in a selector"))),
        }
    }
    if pending.is_some() && !combinators.is_empty() && compounds.last().is_some_and(|c| *c == Compound::default()) {
        return Err(input.new_custom_error("a selector cannot end with a combinator"));
    }
    Selector::new(compounds, combinators).ok_or_else(|| input.new_custom_error("an empty selector"))
}
