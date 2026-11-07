//! Modules: the engine systems a project switches on or leaves out, in
//! `voxear.toml`'s `[modules]` and the editor's Project Settings. A module a
//! project leaves out runs no systems, gives its scripts none of its API,
//! shows none of its components or tools in the editor, and puts nothing
//! into the project's builds. Every engine install has every module, so
//! switching one on later needs no download.
//!
//! ```toml
//! [modules]
//! networking = true
//! guns = true
//! physics_2d = false
//! vegetation = true
//! water = false
//! steam_audio = true
//! ```
//!
//! A project with no `[modules]` was made before modules existed, and has
//! every module it could have used: all but Guns.

use serde::{Deserialize, Serialize};

/// One of the engine's optional systems.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Module {
    Networking,
    Guns,
    Physics2D,
    Vegetation,
    Water,
    SteamAudio,
}

impl Module {
    /// Every module, in the order Project Settings lists them.
    pub const ALL: [Module; 6] =
        [Module::Networking, Module::Guns, Module::Physics2D, Module::Vegetation, Module::Water, Module::SteamAudio];

    /// Its key in `[modules]`.
    pub fn key(self) -> &'static str {
        match self {
            Module::Networking => "networking",
            Module::Guns => "guns",
            Module::Physics2D => "physics_2d",
            Module::Vegetation => "vegetation",
            Module::Water => "water",
            Module::SteamAudio => "steam_audio",
        }
    }

    /// Its name, as the editor shows it.
    pub fn title(self) -> &'static str {
        match self {
            Module::Networking => "Networking",
            Module::Guns => "Guns",
            Module::Physics2D => "2D Physics",
            Module::Vegetation => "Vegetation",
            Module::Water => "Water",
            Module::SteamAudio => "Steam Audio",
        }
    }

    /// What it adds, in a line.
    pub fn summary(self) -> &'static str {
        match self {
            Module::Networking => "Hosting, joining and dedicated servers",
            Module::Guns => "Procedural holding, aiming, recoil and reloads",
            Module::Physics2D => "2D colliders and rigidbodies",
            Module::Vegetation => "Foliage, seasons and impostors",
            Module::Water => "Oceans, pools, streams and buoyancy",
            Module::SteamAudio => "Occlusion, sound paths and room reverb",
        }
    }

    /// The module whose key this is.
    pub fn from_key(key: &str) -> Option<Module> {
        Module::ALL.into_iter().find(|module| module.key() == key)
    }
}

/// Which modules a project has, as `[modules]` lists them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Modules {
    pub networking: bool,
    pub guns: bool,
    pub physics_2d: bool,
    pub vegetation: bool,
    pub water: bool,
    pub steam_audio: bool,
}

impl Modules {
    /// What a project made before modules existed has: every module it
    /// could have used, so nothing it uses goes missing.
    pub fn older_project() -> Self {
        let mut modules = Self::all();
        modules.guns = false;
        modules
    }

    /// Every module.
    pub fn all() -> Self {
        let mut modules = Self::default();
        for module in Module::ALL {
            modules.set(module, true);
        }
        modules
    }

    pub fn has(&self, module: Module) -> bool {
        *self.slot(module)
    }

    pub fn set(&mut self, module: Module, on: bool) {
        *self.slot_mut(module) = on;
    }

    /// The modules switched on, in Project Settings' order.
    pub fn on(&self) -> impl Iterator<Item = Module> + '_ {
        Module::ALL.into_iter().filter(|module| self.has(*module))
    }

    fn slot(&self, module: Module) -> &bool {
        match module {
            Module::Networking => &self.networking,
            Module::Guns => &self.guns,
            Module::Physics2D => &self.physics_2d,
            Module::Vegetation => &self.vegetation,
            Module::Water => &self.water,
            Module::SteamAudio => &self.steam_audio,
        }
    }

    fn slot_mut(&mut self, module: Module) -> &mut bool {
        match module {
            Module::Networking => &mut self.networking,
            Module::Guns => &mut self.guns,
            Module::Physics2D => &mut self.physics_2d,
            Module::Vegetation => &mut self.vegetation,
            Module::Water => &mut self.water,
            Module::SteamAudio => &mut self.steam_audio,
        }
    }
}

/// Whether a world's game has a module: the project's choice, which the
/// game keeps as a resource, or every module where there is no project, as
/// in a test's world.
pub fn has(world: &voxear_ecs::World, module: Module) -> bool {
    world.resource::<Modules>().is_none_or(|modules| modules.has(module))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_lists_what_is_on_and_an_older_project_has_all_but_guns() {
        let modules: Modules = toml::from_str("networking = true\nwater = true\n").unwrap();
        assert_eq!(modules.on().collect::<Vec<_>>(), [Module::Networking, Module::Water]);
        let older = Modules::older_project();
        assert!(Module::ALL.into_iter().all(|m| older.has(m) == (m != Module::Guns)));
        for module in Module::ALL {
            assert_eq!(Module::from_key(module.key()), Some(module));
        }
        assert!(toml::from_str::<Modules>("lasers = true\n").is_err(), "a module there is not is an error");
    }
}
