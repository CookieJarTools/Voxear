//! Selectors, the part of a style rule that says which elements it styles.
//! The set is USS's: types (`Button`), classes (`.primary`), names
//! (`#title`), `*`, the states `:hover`, `:active`, `:focus`,
//! `:focus-visible` (focused, and to be shown so: reached with the keyboard,
//! or a text field, as in browsers), `:disabled`, `:checked` and
//! `:selected`, `:root` (the element the sheet is attached to), and the
//! descendant and child (`>`) combinators.

/// States an element can be in, which `:hover` and the like select on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct States(u8);

impl States {
    pub const HOVER: States = States(1);
    pub const ACTIVE: States = States(2);
    pub const FOCUS: States = States(4);
    pub const DISABLED: States = States(8);
    pub const CHECKED: States = States(16);
    /// A selected row of a list or tree.
    pub const SELECTED: States = States(32);
    /// Focused, and to be shown so: reached with Tab, or a text field. A
    /// button pressed with the pointer is focused without it, so its ring
    /// does not stay on as if marking what was pressed last.
    pub const FOCUS_VISIBLE: States = States(64);

    pub fn contains(self, other: States) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn set(&mut self, state: States, on: bool) {
        if on { self.0 |= state.0 } else { self.0 &= !state.0 }
    }

    pub(crate) fn named(name: &str) -> Option<States> {
        Some(match name {
            "hover" => Self::HOVER,
            "active" => Self::ACTIVE,
            "focus" => Self::FOCUS,
            "focus-visible" => Self::FOCUS_VISIBLE,
            "disabled" => Self::DISABLED,
            "checked" => Self::CHECKED,
            "selected" => Self::SELECTED,
            _ => return None,
        })
    }
}

/// What a selector can see of an element.
pub(crate) struct Facts<'a> {
    pub tag: &'static str,
    pub name: Option<&'a str>,
    pub classes: &'a [String],
    pub states: States,
    /// How far down the tree it is, for `:root`.
    pub depth: usize,
}

/// Simple selectors with nothing between them, such as `Button.primary:hover`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Compound {
    pub tag: Option<String>,
    pub name: Option<String>,
    pub classes: Vec<String>,
    pub states: States,
    pub root: bool,
}

impl Compound {
    /// Whether `element` matches, for a sheet attached `scope` levels down.
    fn matches(&self, element: &Facts, scope: usize) -> bool {
        self.tag.as_deref().is_none_or(|tag| tag == element.tag)
            && self.name.as_deref().is_none_or(|name| Some(name) == element.name)
            && self.classes.iter().all(|class| element.classes.contains(class))
            && element.states.contains(self.states)
            && (!self.root || element.depth == scope)
    }

    fn is_empty(&self) -> bool {
        *self == Compound::default()
    }

    /// Names count most, then classes and states, then types, as in CSS.
    fn specificity(&self) -> u32 {
        let states = self.states.0.count_ones() + u32::from(self.root);
        u32::from(self.name.is_some()) * 10_000
            + (self.classes.len() as u32 + states) * 100
            + u32::from(self.tag.is_some())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Combinator {
    /// Anywhere inside: `A B`.
    Descendant,
    /// Directly inside: `A > B`.
    Child,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Selector {
    /// The compound the element itself must match.
    pub subject: Compound,
    /// What its ancestors must match, nearest first, each with how it joins
    /// the compound after it.
    pub ancestors: Vec<(Combinator, Compound)>,
    pub specificity: u32,
}

impl Selector {
    pub fn new(mut compounds: Vec<Compound>, combinators: Vec<Combinator>) -> Option<Self> {
        let subject = compounds.pop()?;
        if subject.is_empty() && compounds.is_empty() {
            return None;
        }
        let specificity = compounds.iter().map(Compound::specificity).sum::<u32>() + subject.specificity();
        let ancestors = combinators.into_iter().rev().zip(compounds.into_iter().rev()).collect();
        Some(Self { subject, ancestors, specificity })
    }

    /// Whether the last element of `path`, which runs from the root down,
    /// matches, for a sheet attached to `path[scope]`.
    pub fn matches(&self, path: &[Facts], scope: usize) -> bool {
        let Some((element, above)) = path.split_last() else { return false };
        self.subject.matches(element, scope) && matches_up(&self.ancestors, above, scope)
    }
}

fn matches_up(parts: &[(Combinator, Compound)], above: &[Facts], scope: usize) -> bool {
    let Some(((combinator, compound), rest)) = parts.split_first() else { return true };
    match combinator {
        Combinator::Child => above
            .split_last()
            .is_some_and(|(parent, higher)| compound.matches(parent, scope) && matches_up(rest, higher, scope)),
        Combinator::Descendant => {
            (0..above.len()).rev().any(|i| compound.matches(&above[i], scope) && matches_up(rest, &above[..i], scope))
        }
    }
}
