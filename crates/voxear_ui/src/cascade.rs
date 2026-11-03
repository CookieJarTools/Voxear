//! The cascade: each element's style, worked out as CSS does. It starts
//! from the style given in code, applies the matching rules of every sheet
//! in effect (those attached to it and its ancestors) from least to most
//! specific, then its inline declarations. Text styles and custom
//! properties are inherited from the parent.
//!
//! Only what changed is styled again. A change marks the element it was
//! made to, and marks it deep when what is inside it may now match other
//! rules: a new element, a sheet, a name, or a class or state some selector
//! asks of an ancestor (`.dark .label`). An element styled again passes the
//! cascade on to its children when marked deep, or when what they inherit
//! from it changed.

use std::{collections::HashMap, rc::Rc};

use taffy::Display;

use crate::{
    Animated, KeyframeAnimation, Overflow, Style, TextStyle, Ui,
    animation::{self, Playing, Track},
    properties,
    selector::{Facts, States},
    sheet::{Declaration, StyleSheet},
    tree::Element,
};

/// What an element passes down to its children.
struct Inherited {
    text: TextStyle,
    vars: Rc<HashMap<String, String>>,
    /// Sheets in effect, each with how far down it was attached.
    sheets: Rc<Vec<(Rc<StyleSheet>, usize)>>,
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
    fn of(element: &Element) -> Self {
        Self {
            tag: element.kind.tag(),
            name: element.name.clone(),
            classes: element.classes.clone(),
            states: element.states,
        }
    }

    fn facts(&self, depth: usize) -> Facts<'_> {
        Facts { tag: self.tag, name: self.name.as_deref(), classes: &self.classes, states: self.states, depth }
    }
}

impl Ui {
    /// Styles again each element marked since the last cascade, and what is
    /// in it as far as the change reaches.
    pub(crate) fn cascade(&mut self) {
        voxear_core::profile_scope!("Styles");
        let mut warnings = Vec::new();
        for (node, deep) in self.marked_for_restyle() {
            let (inherited, mut path) = self.inherited(node);
            self.cascade_node(node, &inherited, &mut path, deep, &mut warnings);
        }
        self.warn(warnings);
    }

    /// The elements marked since the last cascade that are still in the
    /// tree, each once and deep if it was ever marked so, parents before
    /// what is in them, leaving out those inside one marked deep, as styling
    /// that one styles them.
    fn marked_for_restyle(&mut self) -> Vec<(taffy::NodeId, bool)> {
        let mut marked: Vec<(taffy::NodeId, bool)> = Vec::new();
        let mut at: HashMap<taffy::NodeId, usize> = HashMap::new();
        for (node, deep) in std::mem::take(&mut self.restyles) {
            match at.get(&node) {
                Some(&index) => marked[index].1 |= deep,
                None => {
                    at.insert(node, marked.len());
                    marked.push((node, deep));
                }
            }
        }
        let deep_at = |node: taffy::NodeId| at.get(&node).is_some_and(|&index| marked[index].1);
        let mut kept = Vec::new();
        for &(node, deep) in &marked {
            // Gone since it was marked.
            if self.tree.get_node_context(node).is_none() {
                continue;
            }
            let (mut top, mut depth, mut covered) = (node, 0, false);
            while let Some(parent) = self.tree.parent(top) {
                covered |= deep_at(parent);
                (top, depth) = (parent, depth + 1);
            }
            if !covered && top == self.root {
                kept.push((depth, node, deep));
            }
        }
        kept.sort_by_key(|&(depth, ..)| depth);
        kept.into_iter().map(|(_, node, deep)| (node, deep)).collect()
    }

    /// What `node`'s parent passes down, as the last cascade left it, and
    /// the elements above `node` as selectors see them, from the root.
    fn inherited(&self, node: taffy::NodeId) -> (Inherited, Vec<Seen>) {
        let mut above = Vec::new();
        let mut at = node;
        while let Some(parent) = self.tree.parent(at) {
            above.push(parent);
            at = parent;
        }
        above.reverse();
        let (mut sheets, mut path) = (Vec::new(), Vec::with_capacity(above.len() + 1));
        for (depth, &ancestor) in above.iter().enumerate() {
            let element = self.tree.get_node_context(ancestor).expect("every node has an element");
            sheets.extend(element.sheets.iter().map(|sheet| (sheet.clone(), depth)));
            path.push(Seen::of(element));
        }
        let parent = above.last().and_then(|&parent| self.tree.get_node_context(parent));
        let inherited = Inherited {
            text: parent.map_or_else(|| TextStyle { rtl: self.rtl, ..TextStyle::default() }, |parent| parent.text_style),
            vars: parent.map_or_else(Rc::default, |parent| parent.vars.clone()),
            sheets: Rc::new(sheets),
            in_scroller: parent.is_some_and(|parent| parent.computed.overflow == Overflow::Scroll),
        };
        (inherited, path)
    }

    fn cascade_node(
        &mut self,
        node: taffy::NodeId,
        parent: &Inherited,
        path: &mut Vec<Seen>,
        deep: bool,
        warnings: &mut Vec<String>,
    ) {
        let element = self.tree.get_node_context(node).expect("every node has an element");
        let sheets = match element.sheets.is_empty() {
            true => parent.sheets.clone(),
            false => {
                let mut sheets = (*parent.sheets).clone();
                sheets.extend(element.sheets.iter().map(|sheet| (sheet.clone(), path.len())));
                Rc::new(sheets)
            }
        };
        path.push(Seen::of(element));
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
        // Laid out the other way round in right-to-left text, as a page of Arabic is.
        if text.rtl {
            mirror(&mut style.layout);
        }
        // Hiding from code wins over sheets.
        if base.layout.display == Display::None {
            style.layout.display = Display::None;
        }
        // Scrollbars float over the content, so they take no room in the layout.
        let clips =
            if style.overflow == Overflow::Visible { taffy::Overflow::Visible } else { taffy::Overflow::Hidden };
        style.layout.overflow = taffy::Point { x: clips, y: clips };
        let scrolls = style.overflow == Overflow::Scroll;
        if scrolls != self.scrollers.contains(&node) {
            match scrolls {
                true => self.scrollers.push(node),
                false => self.scrollers.retain(|&scroller| scroller != node),
            }
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
        // Whether what the children inherit changed, so they need styling again too.
        let passed_on = element.text_style != text
            || (!Rc::ptr_eq(&element.vars, &vars) && element.vars != vars)
            || (element.computed.overflow == Overflow::Scroll) != scrolls;
        let layout = (element.computed.layout != style.layout).then(|| style.layout.clone());
        element.computed = style;
        element.text_style = text;
        element.vars = vars.clone();
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

        if deep || passed_on {
            let inherited = Inherited { text, vars, sheets, in_scroller: scrolls };
            for child in self.tree.children(node).unwrap_or_default() {
                self.cascade_node(child, &inherited, path, deep, warnings);
            }
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
    let element = facts.last().expect("the path ends at the element");
    let mut matched = Vec::new();
    for (sheet_index, (sheet, scope)) in sheets.iter().enumerate() {
        for (rule_index, selector_index) in sheet.index.candidates(element) {
            let selector = &sheet.rules[rule_index].selectors[selector_index];
            if selector.matches(&facts, *scope) {
                matched.push((selector.specificity, sheet_index, rule_index));
            }
        }
    }
    // A rule applies once, as specific as the most specific of its selectors that match.
    matched.sort_unstable_by_key(|&(specificity, sheet, rule)| (sheet, rule, std::cmp::Reverse(specificity)));
    matched.dedup_by_key(|&mut (_, sheet, rule)| (sheet, rule));
    matched.sort_unstable();
    let from_sheets = matched.into_iter().flat_map(|(_, sheet, rule)| {
        let sheet = &sheets[sheet].0;
        sheet.rules[rule].declarations.iter().map(move |d| (d, sheet.source.as_str()))
    });
    from_sheets.chain(inline.iter().map(|d| (d, "inline style"))).collect()
}

/// A layout the other way round: a row from the right, and left and right
/// swapped in its margins, padding, borders and place.
fn mirror(layout: &mut taffy::Style) {
    use taffy::FlexDirection;
    layout.flex_direction = match layout.flex_direction {
        FlexDirection::Row => FlexDirection::RowReverse,
        FlexDirection::RowReverse => FlexDirection::Row,
        column => column,
    };
    std::mem::swap(&mut layout.margin.left, &mut layout.margin.right);
    std::mem::swap(&mut layout.padding.left, &mut layout.padding.right);
    std::mem::swap(&mut layout.border.left, &mut layout.border.right);
    std::mem::swap(&mut layout.inset.left, &mut layout.inset.right);
}
