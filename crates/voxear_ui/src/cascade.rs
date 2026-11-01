//! The cascade: each element's style, worked out as CSS does. It starts
//! from the style given in code, applies the matching rules of every sheet
//! in effect (those attached to it and its ancestors) from least to most
//! specific, then its inline declarations. Text styles and custom
//! properties are inherited from the parent.

use std::{collections::HashMap, rc::Rc};

use taffy::Display;

use crate::{
    Animated, KeyframeAnimation, Overflow, Style, TextStyle, Ui,
    animation::{self, Playing, Track},
    properties,
    selector::{Facts, States},
    sheet::{Declaration, StyleSheet},
};

/// What an element passes down to its children.
struct Inherited {
    text: TextStyle,
    vars: Rc<HashMap<String, String>>,
    /// Sheets in effect, each with how far down it was attached.
    sheets: Vec<(Rc<StyleSheet>, usize)>,
    /// The parent scrolls, so its children keep their size instead of
    /// shrinking to fit, as in Unity's ScrollView.
    in_scroller: bool,
}

/// An ancestor as selectors see it, owned so the tree can change meanwhile.
struct Seen {
    tag: &'static str,
    name: Option<String>,
    classes: Vec<String>,
    states: States,
}

impl Seen {
    fn facts(&self, depth: usize) -> Facts<'_> {
        Facts { tag: self.tag, name: self.name.as_deref(), classes: &self.classes, states: self.states, depth }
    }
}

impl Ui {
    pub(crate) fn cascade(&mut self) {
        self.scrollers.clear();
        let top = Inherited { text: TextStyle::default(), vars: Rc::default(), sheets: Vec::new(), in_scroller: false };
        let mut warnings = Vec::new();
        self.cascade_node(self.root, &top, &mut Vec::new(), &mut warnings);
        self.warn(warnings);
    }

    fn cascade_node(
        &mut self,
        node: taffy::NodeId,
        parent: &Inherited,
        path: &mut Vec<Seen>,
        warnings: &mut Vec<String>,
    ) {
        let element = self.tree.get_node_context(node).expect("every node has an element");
        let mut sheets = parent.sheets.clone();
        sheets.extend(element.sheets.iter().map(|sheet| (sheet.clone(), path.len())));
        path.push(Seen {
            tag: element.kind.tag(),
            name: element.name.clone(),
            classes: element.classes.clone(),
            states: element.states,
        });
        let (base, base_text, inline) = (element.base.clone(), element.base_text, element.inline.clone());

        let declarations = matching(&sheets, path, &inline);
        let mut vars = parent.vars.clone();
        for (declaration, _) in declarations.iter().filter(|(d, _)| d.name.starts_with("--")) {
            Rc::make_mut(&mut vars).insert(declaration.name.clone(), declaration.value.clone());
        }
        let mut style = base.clone();
        if parent.in_scroller {
            style.layout.flex_shrink = 0.0;
        }
        let mut text = base_text.unwrap_or(parent.text);
        for (declaration, source) in declarations.iter().filter(|(d, _)| !d.name.starts_with("--")) {
            if let Err(message) = properties::apply(&mut style, &mut text, &declaration.name, &declaration.value, &vars)
            {
                warnings.push(format!("{source}:{}: {message}", declaration.line));
            }
        }
        // Hiding from code wins over sheets.
        if base.layout.display == Display::None {
            style.layout.display = Display::None;
        }
        // Scrollbars float over the content, so they take no room in the layout.
        let clips =
            if style.overflow == Overflow::Visible { taffy::Overflow::Visible } else { taffy::Overflow::Hidden };
        style.layout.overflow = taffy::Point { x: clips, y: clips };
        if style.overflow == Overflow::Scroll {
            self.scrollers.push(node);
        }

        let (now, clock) = (self.input.time, self.input.clock);
        let tracks = style.animation.as_ref().map(|played| keyframes(played, &sheets, &style, &text, &vars, warnings));
        let element = self.tree.get_node_context_mut(node).expect("every node has an element");
        if clock {
            animation::start(element, &style, &text, now);
        }
        // Keyframes go on from when they began, unless the animation itself changed.
        element.keyframes = match (style.animation.clone(), tracks) {
            (Some(spec), Some(tracks)) => {
                let kept = element.keyframes.take().filter(|played| played.spec == spec);
                let start = kept.map_or(now, |played| played.start);
                Some(Playing { spec, tracks, start })
            }
            _ => None,
        };
        element.styled = true;
        let layout = (element.computed.layout != style.layout).then(|| style.layout.clone());
        element.computed = style;
        element.text_style = text;
        let mut reshape = false;
        if let Some(label) = &mut element.text
            && label.style != text
        {
            (label.style, label.shaped, reshape) = (text, None, true);
        }
        if let Some(layout) = layout {
            let _ = self.tree.set_style(node, layout);
        }
        if reshape {
            let _ = self.tree.mark_dirty(node);
        }

        let in_scroller = self.tree.get_node_context(node).is_some_and(|e| e.computed.overflow == Overflow::Scroll);
        let inherited = Inherited { text, vars, sheets, in_scroller };
        for child in self.tree.children(node).unwrap_or_default() {
            self.cascade_node(child, &inherited, path, warnings);
        }
        path.pop();
    }
}

/// The keyframes an animation names, from the nearest sheet that has them,
/// as each animated property's value at each stop. A stop's values are read
/// as the element's own would be, `var()` and all.
fn keyframes(
    played: &KeyframeAnimation,
    sheets: &[(Rc<StyleSheet>, usize)],
    style: &Style,
    text: &TextStyle,
    vars: &HashMap<String, String>,
    warnings: &mut Vec<String>,
) -> Vec<Track> {
    let found = sheets.iter().rev().find_map(|(sheet, _)| {
        sheet.keyframes.iter().rev().find(|k| k.name == played.name).map(|keyframes| (sheet, keyframes))
    });
    let Some((sheet, keyframes)) = found else {
        warnings.push(format!("no @keyframes named '{}' for an animation to play", played.name));
        return Vec::new();
    };
    let mut tracks: Vec<Track> = Vec::new();
    for (at, declarations) in &keyframes.stops {
        for declaration in declarations {
            let property = match declaration.name.as_str() {
                "background-color" => Animated::Background,
                "border-color" => Animated::BorderColor,
                "color" => Animated::Color,
                "opacity" => Animated::Opacity,
                other => {
                    warnings.push(format!(
                        "{}:{}: {other} does not animate; colours and opacity do",
                        sheet.source, declaration.line
                    ));
                    continue;
                }
            };
            let (mut keyed, mut keyed_text) = (style.clone(), *text);
            if let Err(message) =
                properties::apply(&mut keyed, &mut keyed_text, &declaration.name, &declaration.value, vars)
            {
                warnings.push(format!("{}:{}: {message}", sheet.source, declaration.line));
                continue;
            }
            let value = animation::resting(&keyed, &keyed_text, property);
            match tracks.iter_mut().find(|t| t.property == property) {
                Some(track) => track.stops.push((*at, value)),
                None => tracks.push(Track { property, stops: vec![(*at, value)] }),
            }
        }
    }
    tracks
}

/// Every declaration that applies to the last element of `path`, in the
/// order they apply, each with where it came from: sheet rules from least to
/// most specific (later sheets, then later rules, winning ties), then the
/// inline ones.
fn matching<'a>(
    sheets: &'a [(Rc<StyleSheet>, usize)],
    path: &[Seen],
    inline: &'a [Declaration],
) -> Vec<(&'a Declaration, &'a str)> {
    let facts: Vec<Facts> = path.iter().enumerate().map(|(depth, seen)| seen.facts(depth)).collect();
    let mut matched = Vec::new();
    for (sheet_index, (sheet, scope)) in sheets.iter().enumerate() {
        for (rule_index, rule) in sheet.rules.iter().enumerate() {
            let best = rule.selectors.iter().filter(|s| s.matches(&facts, *scope)).map(|s| s.specificity).max();
            if let Some(specificity) = best {
                matched.push((specificity, sheet_index, rule_index));
            }
        }
    }
    matched.sort_unstable();
    let from_sheets = matched.into_iter().flat_map(|(_, sheet, rule)| {
        let sheet = &sheets[sheet].0;
        sheet.rules[rule].declarations.iter().map(move |d| (d, sheet.source.as_str()))
    });
    from_sheets.chain(inline.iter().map(|d| (d, "inline style"))).collect()
}
