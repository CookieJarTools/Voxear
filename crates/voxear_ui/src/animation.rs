//! Transitions at work: when the cascade changes a property that has a
//! `transition`, the element eases from what is on screen to the new value.
//! Colours blend in linear, premultiplied space, so a fade never passes
//! through a muddy middle. Nothing animates the first time an element is
//! styled, so new elements appear as they are.

use crate::{Animated, Easing, Style, TextStyle, tree::Element};

const PROPERTIES: [Animated; 4] = [Animated::Background, Animated::BorderColor, Animated::Color, Animated::Opacity];

pub(crate) struct Animation {
    pub property: Animated,
    /// Linear premultiplied colours, or an opacity in the first place.
    from: [f32; 4],
    to: [f32; 4],
    /// In the UI's seconds.
    start: f32,
    duration: f32,
    easing: Easing,
}

impl Animation {
    fn at(&self, now: f32) -> [f32; 4] {
        let t = self.easing.at((now - self.start) / self.duration);
        std::array::from_fn(|i| self.from[i] + (self.to[i] - self.from[i]) * t)
    }

    fn finished(&self, now: f32) -> bool {
        now >= self.start + self.duration
    }
}

/// What a property shows when nothing animates it.
fn resting(style: &Style, text: &TextStyle, property: Animated) -> [f32; 4] {
    let transparent = [0.0; 4];
    match property {
        Animated::Background => style.background.map_or(transparent, |c| c.linear_premultiplied()),
        Animated::BorderColor => style.border.map_or(transparent, |(_, c)| c.linear_premultiplied()),
        Animated::Color => text.color.linear_premultiplied(),
        Animated::Opacity => [style.opacity, 0.0, 0.0, 0.0],
        Animated::All => transparent,
    }
}

/// What a property shows at `now`: its animation's value, or its resting one.
pub(crate) fn showing(element: &Element, property: Animated, now: f32) -> [f32; 4] {
    match element.animations.iter().find(|a| a.property == property) {
        Some(animation) => animation.at(now),
        None => resting(&element.computed, &element.text_style, property),
    }
}

/// Starts animations for whatever the new style changes that it transitions.
pub(crate) fn start(element: &mut Element, style: &Style, text: &TextStyle, now: f32) {
    if style.transitions.is_empty() || !element.styled {
        return;
    }
    for property in PROPERTIES {
        let Some(transition) =
            style.transitions.iter().rev().find(|t| t.property == property || t.property == Animated::All)
        else {
            continue;
        };
        let target = resting(style, text, property);
        // Already heading there, or already there.
        if resting(&element.computed, &element.text_style, property) == target {
            continue;
        }
        let from = showing(element, property, now);
        element.animations.retain(|a| a.property != property);
        element.animations.push(Animation {
            property,
            from,
            to: target,
            start: now + transition.delay,
            duration: transition.duration.max(1e-3),
            easing: transition.easing,
        });
    }
}

/// Drops animations that have finished.
pub(crate) fn settle(element: &mut Element, now: f32) {
    element.animations.retain(|a| !a.finished(now));
}
