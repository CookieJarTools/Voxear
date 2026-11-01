//! Transitions at work: when the cascade changes a property that has a
//! `transition`, the element eases from what is on screen to the new value.
//! Colours blend in linear, premultiplied space, so a fade never passes
//! through a muddy middle. Nothing animates the first time an element is
//! styled, so new elements appear as they are.

use crate::{Animated, Easing, KeyframeAnimation, Style, TextStyle, tree::Element};

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

/// Keyframes being played: each animated property's values at its stops,
/// and when it began.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Playing {
    pub spec: KeyframeAnimation,
    pub tracks: Vec<Track>,
    /// In the UI's seconds.
    pub start: f32,
}

/// One property's keyframes: where each stop is, from 0 to 1, and its value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Track {
    pub property: Animated,
    pub stops: Vec<(f32, [f32; 4])>,
}

impl Playing {
    /// Played through as many times as asked; one played over and over
    /// never is.
    pub fn finished(&self, now: f32) -> bool {
        let spec = &self.spec;
        spec.iterations.is_some_and(|count| now >= self.start + spec.delay + spec.duration * count)
    }

    /// The property's value now, or `None` while it shows its own: before
    /// the delay, once finished, or for a property the keyframes leave be.
    fn at(&self, property: Animated, now: f32, resting: [f32; 4]) -> Option<[f32; 4]> {
        let track = self.tracks.iter().find(|t| t.property == property)?;
        let spec = &self.spec;
        let played = (now - self.start - spec.delay) / spec.duration.max(1e-3);
        if played < 0.0 || self.finished(now) {
            return None;
        }
        let lap = played.floor();
        let mut phase = played - lap;
        if spec.alternate && lap as u64 % 2 == 1 {
            phase = 1.0 - phase;
        }
        // Stops the keyframes leave out, at either end, show the property's own value.
        let mut stops = track.stops.clone();
        if stops.first().is_none_or(|&(at, _)| at > 0.0) {
            stops.insert(0, (0.0, resting));
        }
        if stops.last().is_none_or(|&(at, _)| at < 1.0) {
            stops.push((1.0, resting));
        }
        let next = stops.iter().position(|&(at, _)| at >= phase).unwrap_or(stops.len() - 1).max(1);
        let ((a, from), (b, to)) = (stops[next - 1], stops[next]);
        let t = spec.easing.at(if b > a { (phase - a) / (b - a) } else { 1.0 });
        Some(std::array::from_fn(|i| from[i] + (to[i] - from[i]) * t))
    }
}

/// What a property shows when nothing animates it.
pub(crate) fn resting(style: &Style, text: &TextStyle, property: Animated) -> [f32; 4] {
    let transparent = [0.0; 4];
    match property {
        Animated::Background => style.background.map_or(transparent, |c| c.linear_premultiplied()),
        Animated::BorderColor => style.border.map_or(transparent, |(_, c)| c.linear_premultiplied()),
        Animated::Color => text.color.linear_premultiplied(),
        Animated::Opacity => [style.opacity, 0.0, 0.0, 0.0],
        Animated::All => transparent,
    }
}

/// What a property shows at `now`: its keyframes' value while they play,
/// else its transition's, else its resting one.
pub(crate) fn showing(element: &Element, property: Animated, now: f32) -> [f32; 4] {
    let transitioned = match element.animations.iter().find(|a| a.property == property) {
        Some(animation) => animation.at(now),
        None => resting(&element.computed, &element.text_style, property),
    };
    let keyed = element.keyframes.as_ref().and_then(|played| played.at(property, now, transitioned));
    keyed.unwrap_or(transitioned)
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
