//! Layers, as Unity's: each Obj is on one of 32, by number, and the project
//! names them and says which layers' Objs collide. Physics keeps apart
//! Objs on layers that do not collide, and a ray or a sphere can ask after
//! some layers only.

use voxear_ecs::Component;

/// How many layers there are.
pub const LAYER_COUNT: usize = 32;

/// An Obj's layer, by its number from 0 to 31. An Obj without one is on
/// layer 0, Default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Layer(pub u8);

impl Component for Layer {}

/// The project's layers: each one's name, by number, with an empty name for
/// one not used, and which layers' Objs collide. Kept in the world as a
/// resource.
#[derive(Clone, Debug, PartialEq)]
pub struct Layers {
    names: Vec<String>,
    /// Bit `b` of `collide[a]` is set when Objs on layers `a` and `b` collide.
    collide: [u32; LAYER_COUNT],
}

impl Default for Layers {
    fn default() -> Self {
        let mut names = vec![String::new(); LAYER_COUNT];
        names[0] = "Default".to_string();
        Self { names, collide: [u32::MAX; LAYER_COUNT] }
    }
}

impl Layers {
    /// Layers named in order from layer 0, the first being Default when it is
    /// left empty, every pair colliding.
    pub fn named(names: &[String]) -> Self {
        let mut layers = Self::default();
        for (at, name) in names.iter().take(LAYER_COUNT).enumerate() {
            layers.names[at] = name.trim().to_string();
        }
        if layers.names[0].is_empty() {
            layers.names[0] = "Default".to_string();
        }
        layers
    }

    /// A layer's name, empty for one not used.
    pub fn name(&self, layer: u8) -> &str {
        self.names.get(usize::from(layer)).map_or("", String::as_str)
    }

    /// Every layer's name, by number, empty for one not used.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// The layer with a name.
    pub fn find(&self, name: &str) -> Option<u8> {
        let name = name.trim();
        (!name.is_empty()).then(|| self.names.iter().position(|n| n == name)).flatten().map(|at| at as u8)
    }

    /// The layers in use, by number and name.
    pub fn used(&self) -> impl Iterator<Item = (u8, &str)> {
        self.names.iter().enumerate().filter(|(_, n)| !n.is_empty()).map(|(at, n)| (at as u8, n.as_str()))
    }

    pub fn collide(&self, a: u8, b: u8) -> bool {
        let (a, b) = (usize::from(a) % LAYER_COUNT, usize::from(b) % LAYER_COUNT);
        self.collide[a] & (1 << b) != 0
    }

    /// Whether Objs on two layers collide, both ways round.
    pub fn set_collide(&mut self, a: u8, b: u8, on: bool) {
        let (a, b) = (usize::from(a) % LAYER_COUNT, usize::from(b) % LAYER_COUNT);
        for (row, bit) in [(a, b), (b, a)] {
            match on {
                true => self.collide[row] |= 1 << bit,
                false => self.collide[row] &= !(1 << bit),
            }
        }
    }

    /// Each layer's row of the layers it collides with, as bits.
    pub fn matrix(&self) -> [u32; LAYER_COUNT] {
        self.collide
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_are_named_by_number_and_collide_both_ways_round() {
        let layers = Layers::named(&[String::new(), "Player".into(), " Enemy ".into()]);
        assert_eq!(
            (layers.name(0), layers.name(1), layers.name(2), layers.name(3)),
            ("Default", "Player", "Enemy", "")
        );
        assert_eq!((layers.find("Enemy"), layers.find("Nobody"), layers.find("")), (Some(2), None, None));
        assert_eq!(layers.used().count(), 3);
        let mut layers = layers;
        assert!(layers.collide(1, 2));
        layers.set_collide(2, 1, false);
        assert!(!layers.collide(1, 2) && !layers.collide(2, 1) && layers.collide(1, 1));
        assert_eq!(layers.matrix()[1], !(1 << 2));
    }
}
