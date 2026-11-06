//! Values that change along the way: a `Curve` of numbers and a `Gradient`
//! of colours, each a few keys from 0 to 1 joined by straight lines, as a
//! particle's size over its life or a tyre's grip against its slip. They
//! hold their keys in place, up to `MAX_KEYS`, so they copy as plain values
//! and go to the GPU as they are; scene files keep them as lists.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The most keys a curve or a gradient holds.
pub const MAX_KEYS: usize = 8;

/// Numbers along the way from 0 to 1: keys of where and what, joined by
/// straight lines, flat before the first and after the last. With no keys
/// it has no value, and what uses it does as it would without one.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Curve {
    keys: [[f32; 2]; MAX_KEYS],
    count: u8,
}

impl Curve {
    /// A curve through `keys`, each `[where, what]`: put in order of where,
    /// `where` kept from 0 to 1, as many as it holds.
    pub fn new(keys: &[[f32; 2]]) -> Self {
        let mut curve = Self::default();
        for key in keys.iter().take(MAX_KEYS) {
            curve.keys[curve.count as usize] = [key[0].clamp(0.0, 1.0), key[1]];
            curve.count += 1;
        }
        curve.keys[..curve.count as usize].sort_by(|a, b| a[0].total_cmp(&b[0]));
        curve
    }

    /// The same all along.
    pub fn flat(value: f32) -> Self {
        Self::new(&[[0.0, value], [1.0, value]])
    }

    pub fn keys(&self) -> &[[f32; 2]] {
        &self.keys[..self.count as usize]
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Its value at `t`, from 0 to 1; `None` with no keys.
    pub fn at(&self, t: f32) -> Option<f32> {
        along(self.keys(), t, |key: &[f32; 2]| key[0], |a, b, f| [a[0], a[1] + (b[1] - a[1]) * f]).map(|key| key[1])
    }

    /// With a key moved or changed: put back in order, so its index may change; gives its index after.
    pub fn with_key(&self, index: usize, key: [f32; 2]) -> (Self, usize) {
        let mut keys = self.keys().to_vec();
        if index >= keys.len() {
            return (*self, index);
        }
        keys[index] = [key[0].clamp(0.0, 1.0), key[1]];
        let curve = Self::new(&keys);
        let at = curve.keys().iter().position(|k| *k == keys[index]).unwrap_or(index);
        (curve, at)
    }

    /// With a key added, when it holds fewer than `MAX_KEYS`; and its index.
    pub fn with_added(&self, key: [f32; 2]) -> Option<(Self, usize)> {
        if self.count as usize >= MAX_KEYS {
            return None;
        }
        let mut keys = self.keys().to_vec();
        keys.push(key);
        let curve = Self::new(&keys);
        let at = curve.keys().iter().position(|k| k[0] == key[0].clamp(0.0, 1.0) && k[1] == key[1]).unwrap_or(0);
        Some((curve, at))
    }

    /// With a key taken out.
    pub fn without(&self, index: usize) -> Self {
        let keys: Vec<[f32; 2]> = self.keys().iter().enumerate().filter(|(i, _)| *i != index).map(|(_, k)| *k).collect();
        Self::new(&keys)
    }

    /// How much it adds up to from 0 to `t`: the area under it.
    pub fn sum_to(&self, t: f32) -> f32 {
        let keys = self.keys();
        let Some(first) = keys.first() else { return t };
        let t = t.clamp(0.0, 1.0);
        let mut total = first[1] * first[0].min(t);
        for pair in keys.windows(2) {
            let [a, b] = [pair[0], pair[1]];
            if t <= a[0] {
                break;
            }
            let end = t.min(b[0]);
            let span = b[0] - a[0];
            let value_at_end = if span > 0.0 { a[1] + (b[1] - a[1]) * (end - a[0]) / span } else { b[1] };
            total += (a[1] + value_at_end) * 0.5 * (end - a[0]);
        }
        let last = keys[keys.len() - 1];
        total + last[1] * (t - last[0]).max(0.0)
    }
}

/// Colours along the way from 0 to 1: keys of where and which colour,
/// linear, blended in straight lines, as a Curve's numbers are.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Gradient {
    keys: [[f32; 4]; MAX_KEYS],
    count: u8,
}

impl Gradient {
    /// A gradient through `keys`, each `[where, red, green, blue]`.
    pub fn new(keys: &[[f32; 4]]) -> Self {
        let mut gradient = Self::default();
        for key in keys.iter().take(MAX_KEYS) {
            gradient.keys[gradient.count as usize] = [key[0].clamp(0.0, 1.0), key[1], key[2], key[3]];
            gradient.count += 1;
        }
        gradient.keys[..gradient.count as usize].sort_by(|a, b| a[0].total_cmp(&b[0]));
        gradient
    }

    pub fn keys(&self) -> &[[f32; 4]] {
        &self.keys[..self.count as usize]
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Its colour at `t`; `None` with no keys.
    pub fn at(&self, t: f32) -> Option<[f32; 3]> {
        let blend = |a: &[f32; 4], b: &[f32; 4], f: f32| [0, 1, 2, 3].map(|c| a[c] + (b[c] - a[c]) * f);
        along(self.keys(), t, |key: &[f32; 4]| key[0], blend).map(|key| [key[1], key[2], key[3]])
    }

    /// With a key changed, put back in order; its index after.
    pub fn with_key(&self, index: usize, key: [f32; 4]) -> (Self, usize) {
        let mut keys = self.keys().to_vec();
        if index >= keys.len() {
            return (*self, index);
        }
        keys[index] = [key[0].clamp(0.0, 1.0), key[1], key[2], key[3]];
        let gradient = Self::new(&keys);
        let at = gradient.keys().iter().position(|k| *k == keys[index]).unwrap_or(index);
        (gradient, at)
    }

    /// With a key added, when it holds fewer than `MAX_KEYS`.
    pub fn with_added(&self, key: [f32; 4]) -> Option<(Self, usize)> {
        if self.count as usize >= MAX_KEYS {
            return None;
        }
        let mut keys = self.keys().to_vec();
        keys.push(key);
        let gradient = Self::new(&keys);
        let at = gradient.keys().iter().position(|k| k[0] == key[0].clamp(0.0, 1.0) && k[1..] == key[1..]).unwrap_or(0);
        Some((gradient, at))
    }

    pub fn without(&self, index: usize) -> Self {
        let keys: Vec<[f32; 4]> = self.keys().iter().enumerate().filter(|(i, _)| *i != index).map(|(_, k)| *k).collect();
        Self::new(&keys)
    }
}

/// The value at `t` along keys in order of `place`, blended by `blend`
/// between the two either side, flat past the ends.
fn along<K: Copy>(keys: &[K], t: f32, place: impl Fn(&K) -> f32, blend: impl Fn(&K, &K, f32) -> K) -> Option<K> {
    let first = keys.first()?;
    if t <= place(first) {
        return Some(*first);
    }
    for pair in keys.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if t <= place(b) {
            let span = place(b) - place(a);
            return Some(blend(a, b, if span > 0.0 { (t - place(a)) / span } else { 1.0 }));
        }
    }
    keys.last().copied()
}

impl Serialize for Curve {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.keys().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Curve {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::new(&Vec::<[f32; 2]>::deserialize(deserializer)?))
    }
}

impl Serialize for Gradient {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.keys().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Gradient {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::new(&Vec::<[f32; 4]>::deserialize(deserializer)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_curve_runs_straight_between_its_keys_and_flat_past_them() {
        let curve = Curve::new(&[[1.0, 0.0], [0.5, 2.0], [0.0, 1.0]]);
        assert_eq!(curve.keys(), &[[0.0, 1.0], [0.5, 2.0], [1.0, 0.0]], "put in order");
        assert_eq!(curve.at(0.25), Some(1.5));
        assert_eq!(curve.at(0.75), Some(1.0));
        assert_eq!(Curve::new(&[[0.5, 3.0]]).at(0.9), Some(3.0), "one key is flat");
        assert_eq!(Curve::default().at(0.5), None, "none, no value");
    }

    #[test]
    fn a_curve_adds_up_to_the_area_under_it() {
        let rising = Curve::new(&[[0.0, 0.0], [1.0, 2.0]]);
        assert!((rising.sum_to(1.0) - 1.0).abs() < 1e-6);
        assert!((rising.sum_to(0.5) - 0.25).abs() < 1e-6);
        assert!((Curve::flat(3.0).sum_to(0.5) - 1.5).abs() < 1e-6);
        assert!((Curve::new(&[[0.5, 2.0]]).sum_to(1.0) - 2.0).abs() < 1e-6, "flat before its only key too");
        assert_eq!(Curve::default().sum_to(0.4), 0.4, "none, as if 1 all along");
    }

    #[test]
    fn keys_move_add_and_go_keeping_their_order() {
        let curve = Curve::new(&[[0.0, 1.0], [1.0, 0.0]]);
        let (moved, at) = curve.with_key(0, [0.8, 0.5]);
        assert_eq!((moved.keys(), at), (&[[0.8, 0.5], [1.0, 0.0]][..], 0));
        let (moved, at) = moved.with_key(0, [1.0, 0.5]);
        assert_eq!(at, 0, "two at the same place keep their order");
        let (added, at) = curve.with_added([0.5, 4.0]).unwrap();
        assert_eq!((added.keys().len(), at), (3, 1));
        assert_eq!(added.without(1), curve);
        let full = Curve::new(&[[0.5, 0.0]; MAX_KEYS]);
        assert!(full.with_added([0.1, 0.0]).is_none(), "no more than it holds");
        assert_eq!(moved.keys().len(), 2);
    }

    #[test]
    fn a_gradient_blends_its_colours_and_both_save_as_lists() {
        let gradient = Gradient::new(&[[0.0, 1.0, 0.0, 0.0], [1.0, 0.0, 0.0, 1.0]]);
        assert_eq!(gradient.at(0.5), Some([0.5, 0.0, 0.5]));
        let text = serde_json::to_string(&gradient).unwrap();
        assert_eq!(text, "[[0.0,1.0,0.0,0.0],[1.0,0.0,0.0,1.0]]");
        assert_eq!(serde_json::from_str::<Gradient>(&text).unwrap(), gradient);
        let curve: Curve = serde_json::from_str("[[0.0,1.0],[1.0,0.0]]").unwrap();
        assert_eq!(curve.at(0.5), Some(0.5));
    }
}
