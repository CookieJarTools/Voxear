//! What making worlds of blocks may spend, so a game runs on slow machines
//! as well as fast ones: how many threads make terrain at once, how many
//! chunk columns are made at once, how many megabytes of chunks go to the
//! GPU a frame, and how far a world without end is drawn: its own blocks,
//! its coarser rings, and its plants and the structures in its rings, each
//! as many times as far as the world itself says, and no further than a
//! limit. A preset ([`WorldQuality`]) sets all of them; changing one makes
//! it Custom. Kept in the project's `voxear.toml`, changed by a game's
//! options as it runs, and kept for one machine in `Graphics.toml` beside
//! the game's saves ([`MachineBudget`]), which wins over the project's:
//!
//! ```toml
//! [world]
//! quality = "ultra"
//! ```

use serde::{Deserialize, Serialize};
use voxear_ecs::World;

/// A preset of [`WorldBudget`]s, from Low, for slow machines, to Ultra; and
/// Custom, once any number is changed by hand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorldQuality {
    Low,
    Medium,
    #[default]
    High,
    Ultra,
    Custom,
}

impl WorldQuality {
    pub const ALL: [WorldQuality; 5] =
        [WorldQuality::Low, WorldQuality::Medium, WorldQuality::High, WorldQuality::Ultra, WorldQuality::Custom];

    pub fn label(self) -> &'static str {
        match self {
            WorldQuality::Low => "Low",
            WorldQuality::Medium => "Medium",
            WorldQuality::High => "High",
            WorldQuality::Ultra => "Ultra",
            WorldQuality::Custom => "Custom",
        }
    }

    /// The preset's numbers on this machine; none for Custom, which keeps
    /// whatever they are. High draws a world as far as it says, out to 192 m
    /// of its own blocks and 8 km of rings; Ultra half as far again near,
    /// twice as far off, and its plants half as far again.
    pub fn budget(self) -> Option<WorldBudget> {
        self.budget_for(machine_cores())
    }

    /// The preset's numbers on a machine of `cores` cores: how far it draws
    /// is the preset's alone, and how many threads make terrain is a share
    /// of the cores, so a machine of many makes a world as quickly as it
    /// can, however far the world is drawn. Low a quarter of them and
    /// Medium half, High all but two and Ultra all but one, never fewer
    /// than two nor more than one fewer than the cores; and as many columns
    /// made at once as the threads, twice as many on Medium and three times
    /// on High and Ultra, so a thread always has the next to hand.
    pub fn budget_for(self, cores: usize) -> Option<WorldBudget> {
        let cores = cores.max(1) as u32;
        let most = cores.saturating_sub(1).max(1);
        let share = |part: f32, least: u32| ((cores as f32 * part).round() as u32).max(least).min(most);
        let (threads, per_thread, upload, view_distance, far_distance, detail) = match self {
            WorldQuality::Low => (share(0.25, 2), 1, 4, 64.0, 1024.0, [0.5, 0.5, 0.5]),
            WorldQuality::Medium => (share(0.5, 2), 2, 8, 128.0, 4096.0, [0.75, 0.75, 0.75]),
            WorldQuality::High => (cores.saturating_sub(2).max(2).min(most), 3, 16, 192.0, 8192.0, [1.0, 1.0, 1.0]),
            WorldQuality::Ultra => (most, 3, 32, 512.0, 32_768.0, [1.5, 2.0, 1.5]),
            WorldQuality::Custom => return None,
        };
        let columns = (threads * per_thread).min(256);
        // The rings pushed out until their blocks show this many pixels at most, more on the cheaper presets; Low draws
        // them where the distances put them.
        let far_block_pixels = match self {
            WorldQuality::Low => 0.0,
            WorldQuality::Medium => 6.0,
            WorldQuality::High => 4.0,
            _ => 3.0,
        };
        let [near_detail, far_detail, plant_detail] = detail;
        Some(WorldBudget {
            quality: self,
            threads,
            columns,
            upload,
            view_distance,
            far_distance,
            near_detail,
            far_detail,
            plant_detail,
            far_block_pixels,
        })
    }
}

/// How many cores this machine has.
fn machine_cores() -> usize {
    std::thread::available_parallelism().map_or(4, usize::from)
}

/// What making worlds of blocks may spend (see the module's notes). A world
/// without end draws as many times as far as its own distances say as its
/// details, and no further than its view and far distances.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorldBudget {
    pub quality: WorldQuality,
    /// How many threads make terrain at once.
    pub threads: u32,
    /// How many chunk columns a world without end makes at once.
    pub columns: u32,
    /// How many megabytes of chunks go to the GPU a frame; a world of more
    /// comes over several frames, shown as it was until then.
    pub upload: u32,
    /// The furthest a world without end's own chunks reach, in metres.
    pub view_distance: f32,
    /// The furthest its coarser chunks reach, in metres.
    pub far_distance: f32,
    /// How many times as far as the world says its own chunks reach.
    pub near_detail: f32,
    /// How many times as far as the world says its coarser chunks reach.
    pub far_detail: f32,
    /// How many times as far as each Foliage says its plants are drawn, the
    /// woods they show as far off beginning that much further out; and how
    /// many of the world's coarser rings its structures are built into,
    /// fewer under 1.
    pub plant_detail: f32,
    /// The largest a coarser ring's blocks show at its inner edge, in
    /// pixels of a screen 1080 tall: the rings are pushed out, the world's
    /// own blocks drawn on past them, until they are no larger (see
    /// [`WorldBudget::far_pushed_for`]). 0 draws the rings where the
    /// distances put them.
    pub far_block_pixels: f32,
}

impl Default for WorldBudget {
    fn default() -> Self {
        WorldQuality::High.budget().expect("a preset")
    }
}

impl WorldBudget {
    /// Every number within what it may be, a preset's numbers its own:
    /// threads 1 to 64, columns 1 to 256, megabytes 1 to 512, a view
    /// distance of 32 m to 4 km and a far distance no shorter than it, up
    /// to 64 km, and each detail a quarter to four times.
    pub fn checked(self) -> Self {
        if let Some(preset) = self.quality.budget() {
            return preset;
        }
        let view_distance = finite_or(self.view_distance, 128.0).clamp(32.0, 4096.0);
        Self {
            quality: WorldQuality::Custom,
            threads: self.threads.clamp(1, 64),
            columns: self.columns.clamp(1, 256),
            upload: self.upload.clamp(1, 512),
            view_distance,
            far_distance: finite_or(self.far_distance, 1024.0).clamp(view_distance, 65_536.0),
            near_detail: detail(self.near_detail),
            far_detail: detail(self.far_detail),
            plant_detail: detail(self.plant_detail),
            far_block_pixels: block_pixels(self.far_block_pixels),
        }
    }

    /// How many times as far as its own blocks reach a world's first
    /// coarser ring is pushed out (`stream::StreamFrom::pushed`), so that
    /// ring's blocks show no larger than [`WorldBudget::far_block_pixels`]
    /// where it begins, on a screen 1080 tall seen with a view `fov_y`
    /// radians high: a world of blocks `block_size` metres a side whose own
    /// blocks reach `near` metres. 1 for none, and
    /// `stream::MOST_PUSHED` at most.
    pub fn far_pushed_for(&self, near: f32, block_size: f32, fov_y: f32) -> f32 {
        if self.far_block_pixels <= 0.0 {
            return 1.0;
        }
        let per_pixel = 2.0 * (fov_y.clamp(0.05, 3.0) * 0.5).tan() / 1080.0;
        // A block of the first coarser ring, twice as large as the world's own, where its ring would begin.
        let shown = 2.0 * block_size.max(0.01) / (near.max(1.0) * per_pixel);
        (shown / self.far_block_pixels).clamp(1.0, crate::blocks::stream::MOST_PUSHED)
    }

    /// How far a world whose own chunks reach `distance` metres has them
    /// drawn: as many times as far as its near detail, no further than its
    /// view distance, and a column at least.
    pub fn near(&self, distance: f32) -> f32 {
        (finite_or(distance, 0.0) * self.near_detail).min(self.view_distance).max(1.0)
    }

    /// How far a world whose coarser chunks reach `far` metres has them
    /// drawn, its own reaching `near` (see [`WorldBudget::near`]): as many
    /// times as far as its far detail, no further than its far distance, and
    /// no nearer than its own chunks.
    pub fn far(&self, far: f32, near: f32) -> f32 {
        (finite_or(far, 0.0) * self.far_detail).min(self.far_distance).max(near)
    }

    /// How far plants a Foliage draws out to `distance` metres are drawn.
    pub fn plants(&self, distance: f32) -> f32 {
        finite_or(distance, 0.0).max(0.0) * self.plant_detail
    }

    /// How many coarser rings a world's structures are built into, out of
    /// the `rings` they are at a plant detail of 1: one fewer under
    /// three-quarters, two fewer under half, one at least.
    pub fn structure_rings(&self, rings: u32) -> u32 {
        let fewer = match self.plant_detail {
            d if d < 0.5 => 2,
            d if d < 0.75 => 1,
            _ => 0,
        };
        rings.saturating_sub(fewer).max(1)
    }

    /// The budget in `world`, or the default where it has none.
    pub fn of(world: &World) -> Self {
        world.resource::<WorldBudget>().copied().unwrap_or_default()
    }

    /// Puts the budget where the streamer and the renderer read it, and
    /// lets the terrain's threads work as many at once as it says.
    pub fn put(self, world: &mut World) {
        let budget = self.checked();
        crate::workers::set_threads(budget.threads as usize);
        world.insert_resource(budget);
    }

    /// How many bytes of chunks go to the GPU a frame.
    pub fn upload_bytes(&self) -> u64 {
        u64::from(self.upload.max(1)) << 20
    }
}

fn finite_or(value: f32, otherwise: f32) -> f32 {
    if value.is_finite() { value } else { otherwise }
}

fn detail(value: f32) -> f32 {
    finite_or(value, 1.0).clamp(0.25, 4.0)
}

/// 0 for none, or a block's pixels from 1.5 to 32.
fn block_pixels(value: f32) -> f32 {
    match finite_or(value, 0.0) {
        v if v <= 0.0 => 0.0,
        v => v.clamp(1.5, 32.0),
    }
}

/// The file a machine keeps a game's own budget in, beside its saves.
pub const MACHINE_FILE: &str = "Graphics.toml";

/// Where this machine keeps its own budget for the game, and the project's
/// to go back to: kept in the world beside the budget in use. A game's
/// options change the budget as it runs ([`WorldBudget::put`]); kept, it is
/// what the game starts with on this machine from then on, in place of the
/// project's.
///
/// ```toml
/// # Graphics.toml
/// [world]
/// quality = "ultra"
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MachineBudget {
    /// The project's budget, as its `voxear.toml` has it.
    pub project: WorldBudget,
    /// This machine's file; none where there is nowhere to keep one.
    pub file: Option<std::path::PathBuf>,
    /// Whether this machine keeps a budget of its own.
    pub kept: bool,
}

/// What `Graphics.toml` holds.
#[derive(Default, Serialize, Deserialize)]
struct MachineFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    world: Option<WorldBudget>,
}

impl MachineBudget {
    /// The project's `project` budget and this machine's own in `file`, as
    /// the game starts: put into `world`, this machine's where it keeps one.
    /// What is wrong with a file that cannot be read, which leaves the
    /// project's in use.
    pub fn open(world: &mut World, project: WorldBudget, file: Option<std::path::PathBuf>) -> Option<String> {
        let read = file.as_deref().map(read_machine).unwrap_or(Ok(None));
        let (own, problem) = match read {
            Ok(own) => (own, None),
            Err(problem) => (None, Some(problem)),
        };
        own.unwrap_or(project).put(world);
        world.insert_resource(MachineBudget { project: project.checked(), file, kept: own.is_some() });
        problem
    }

    /// The machine's budget in `world`, or the project's alone.
    pub fn of(world: &World) -> MachineBudget {
        world.resource::<MachineBudget>().cloned().unwrap_or_default()
    }

    /// Keeps the budget `world` uses now for this machine, or forgets the
    /// one kept and goes back to the project's.
    pub fn keep(world: &mut World, keep: bool) -> std::io::Result<()> {
        let mut machine = Self::of(world);
        if let Some(file) = &machine.file {
            match keep {
                true => {
                    let text = toml::to_string(&MachineFile { world: Some(WorldBudget::of(world)) })
                        .map_err(std::io::Error::other)?;
                    if let Some(folder) = file.parent() {
                        std::fs::create_dir_all(folder)?;
                    }
                    std::fs::write(file, format!("# This machine's own graphics settings for the game.\n{text}"))?;
                }
                false => match std::fs::remove_file(file) {
                    Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
                    _ => {}
                },
            }
        }
        if !keep {
            machine.project.put(world);
        }
        machine.kept = keep && machine.file.is_some();
        world.insert_resource(machine);
        Ok(())
    }
}

/// The budget kept in a machine's `Graphics.toml`, where it holds one; none
/// where there is no file.
fn read_machine(file: &std::path::Path) -> Result<Option<WorldBudget>, String> {
    let Ok(text) = std::fs::read_to_string(file) else { return Ok(None) };
    let read = toml::from_str::<MachineFile>(&text).map_err(|error| format!("{}: {error}", file.display()))?;
    Ok(read.world.map(WorldBudget::checked))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preset_sets_every_number_and_a_number_changed_by_hand_is_custom_and_kept_within_bounds() {
        let low = WorldQuality::Low.budget().unwrap();
        assert!(low.threads >= 2 && low.columns == low.threads, "a column at a time on each thread");
        // A preset's numbers are its own, whatever was written beside it.
        let written: WorldBudget = toml::from_str("quality = \"low\"\nthreads = 12").unwrap();
        assert_eq!(written.checked(), low);
        // Custom keeps its own, within bounds.
        let custom = WorldBudget { quality: WorldQuality::Custom, threads: 0, far_distance: 10.0, ..low }.checked();
        assert_eq!((custom.threads, custom.view_distance, custom.far_distance), (1, 64.0, 64.0));
        assert_eq!(WorldBudget::default().quality, WorldQuality::High);
        // Read back as written.
        let text = toml::to_string(&custom).unwrap();
        assert_eq!(toml::from_str::<WorldBudget>(&text).unwrap(), custom);
        // A file from before the details were kept reads with High's.
        let older: WorldBudget = toml::from_str("quality = \"custom\"\nview_distance = 100.0").unwrap();
        assert_eq!((older.near_detail, older.far_detail, older.plant_detail), (1.0, 1.0, 1.0));
        let wild = WorldBudget { quality: WorldQuality::Custom, near_detail: 9.0, plant_detail: f32::NAN, ..low };
        assert_eq!((wild.checked().near_detail, wild.checked().plant_detail), (4.0, 1.0), "a quarter to four times");
    }

    #[test]
    fn a_presets_threads_are_a_share_of_the_machines_cores_whatever_it_draws() {
        let threads = |cores: usize| {
            [WorldQuality::Low, WorldQuality::Medium, WorldQuality::High, WorldQuality::Ultra]
                .map(|quality| quality.budget_for(cores).unwrap())
                .map(|budget| (budget.threads, budget.columns))
        };
        // Twenty-four cores: a quarter, half, all but two, all but one.
        assert_eq!(threads(24), [(6, 6), (12, 24), (22, 66), (23, 69)]);
        // Four: two at the least, and never every core.
        assert_eq!(threads(4), [(2, 2), (2, 4), (2, 6), (3, 9)]);
        // One core: one thread, which the pool keeps to.
        assert!(threads(1).iter().all(|&(threads, _)| threads == 1));
        // How far each draws is the preset's own, however many cores.
        let far = |cores: usize| {
            WorldQuality::Medium.budget_for(cores).map(|budget| (budget.view_distance, budget.far_distance))
        };
        assert_eq!(far(4), far(64));
    }

    #[test]
    fn details_draw_a_world_further_than_it_says_within_the_limits_and_high_draws_it_as_before() {
        let [low, medium, high, ultra] =
            [WorldQuality::Low, WorldQuality::Medium, WorldQuality::High, WorldQuality::Ultra]
                .map(|quality| quality.budget().unwrap());
        // A world that says 256 m of its own blocks and 16 km of rings.
        let drawn = |budget: WorldBudget| {
            let near = budget.near(256.0);
            (near, budget.far(16_384.0, near))
        };
        assert_eq!(drawn(high), (192.0, 8192.0), "High stops where it always did");
        assert_eq!(drawn(ultra), (384.0, 32_768.0), "Ultra half as far again near, twice as far off");
        assert_eq!(drawn(medium), (128.0, 4096.0));
        assert_eq!(drawn(low), (64.0, 1024.0));
        // One that says 128 m and 1 km, the default: Low draws half of it.
        assert_eq!((low.near(128.0), low.far(1024.0, low.near(128.0))), (64.0, 512.0));
        assert_eq!((high.near(128.0), high.far(1024.0, 128.0)), (128.0, 1024.0), "High as the world says");
        // The rings never come nearer than the world's own blocks.
        let short = WorldBudget { quality: WorldQuality::Custom, far_detail: 0.25, ..high }.checked();
        assert_eq!(short.far(400.0, short.near(300.0)), 192.0);
        // Plants as many times as far; structures in fewer rings below three-quarters.
        assert_eq!((ultra.plants(400.0), low.plants(400.0)), (600.0, 200.0));
        assert_eq!([low, medium, high, ultra].map(|budget| budget.structure_rings(3)), [2, 3, 3, 3]);
        let least = WorldBudget { quality: WorldQuality::Custom, plant_detail: 0.25, ..high }.checked();
        assert_eq!(least.structure_rings(3), 1);
    }

    #[test]
    fn rings_are_pushed_out_until_their_blocks_show_no_larger_than_the_preset_says() {
        let [low, medium, high, ultra] =
            [WorldQuality::Low, WorldQuality::Medium, WorldQuality::High, WorldQuality::Ultra]
                .map(|quality| quality.budget().unwrap());
        assert_eq!([low, medium, high, ultra].map(|budget| budget.far_block_pixels), [0.0, 6.0, 4.0, 3.0]);
        let fov = 70f32.to_radians();
        // Metre blocks seen 70° high: 2 m blocks show 8 pixels 192 m off, so High pushes its rings out twice as far.
        let pushed = high.far_pushed_for(192.0, 1.0, fov);
        assert!((pushed - 2.0).abs() < 0.02, "{pushed}");
        // Where the first ring already begins far enough, and on Low, the rings stay where they are.
        assert_eq!(high.far_pushed_for(2000.0, 1.0, fov), 1.0);
        assert_eq!(low.far_pushed_for(64.0, 1.0, fov), 1.0);
        // Ultra, further near, pushes less for smaller pixels; a narrow view pushes as far as it may and no further.
        assert!(ultra.far_pushed_for(384.0, 1.0, fov) < pushed);
        assert_eq!(high.far_pushed_for(32.0, 1.0, 0.1), crate::blocks::stream::MOST_PUSHED);
    }

    #[test]
    fn a_machine_keeps_its_own_budget_beside_the_saves_until_it_forgets_it() {
        let folder = std::env::temp_dir().join(format!("voxear-machine-budget-{}", std::process::id()));
        let file = folder.join(MACHINE_FILE);
        let _ = std::fs::remove_dir_all(&folder);
        let project = WorldQuality::Medium.budget().unwrap();
        // Nothing kept: the project's.
        let mut world = World::new();
        assert_eq!(MachineBudget::open(&mut world, project, Some(file.clone())), None);
        assert_eq!((WorldBudget::of(&world), MachineBudget::of(&world).kept), (project, false));
        // A game's options set Ultra and keep it: written beside the saves, and what the game starts with next.
        WorldQuality::Ultra.budget().unwrap().put(&mut world);
        MachineBudget::keep(&mut world, true).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("[world]") && text.contains("quality = \"ultra\""), "{text}");
        let mut next = World::new();
        MachineBudget::open(&mut next, project, Some(file.clone()));
        assert_eq!(WorldBudget::of(&next).quality, WorldQuality::Ultra);
        assert!(MachineBudget::of(&next).kept);
        // Forgotten: the file goes, and the project's is back at once.
        MachineBudget::keep(&mut next, false).unwrap();
        assert!(!file.exists() && !MachineBudget::of(&next).kept);
        assert_eq!(WorldBudget::of(&next), project);
        // A file that cannot be read leaves the project's, and says why.
        std::fs::write(&file, "[world]\nquality = \"enormous\"").unwrap();
        let problem = MachineBudget::open(&mut World::new(), project, Some(file.clone()));
        assert!(problem.is_some_and(|problem| problem.contains("Graphics.toml")));
        let _ = std::fs::remove_dir_all(&folder);
    }
}
