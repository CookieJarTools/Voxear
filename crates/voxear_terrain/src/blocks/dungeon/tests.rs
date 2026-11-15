use std::sync::Arc;

use super::*;
use crate::blocks::structure::Ground;
use crate::generate::{BiomeDef, Biomes, LevelPlan, Preset, Regions, WorldGen, nodes::Levels, structures::Placement};

/// A temple dungeon of blocks 30 and up: stone, trim, floor, rubble,
/// cracked, mossy, a slab, stairs, statues, iron, fire, crystal, water,
/// lava and vines.
pub(crate) fn temple() -> Dungeon {
    Dungeon {
        blocks: DungeonBlocks {
            stone: 30,
            trim: 31,
            floor: 32,
            rubble: 33,
            cracked: 34,
            mossy: 35,
            slab: 36,
            stairs: [37, 37, 37, 37],
            statue: 38,
            metal: 39,
            light: 40,
            cold_light: 41,
            water: 42,
            lava: 43,
            vine: 44,
        },
        floors: [2, 2],
        ..Dungeon::default()
    }
}

/// A world whose land climbs in escarpments: areas 90 m across, from 4 m
/// up to 120 m in four levels, walls between.
fn escarpments() -> Arc<WorldGen> {
    let world = Arc::new(Preset::RollingHills.world());
    let levels = Levels { size: 90.0, levels: 4, low: 4.0, high: 120.0, peaks: 0.0, ..Levels::default() };
    let list = vec![BiomeDef { temperature: [-60.0, 60.0], humidity: [0.0, 1.0], ..BiomeDef::default() }];
    let biomes = Biomes::new(list, 15.0, 0.3, world.seed());
    let regions = Regions::new(90.0, 16.0, biomes.clone(), world.clone());
    let regions = Arc::new(regions.with_levels(LevelPlan { keep: None, ..levels.plan() }));
    let generate = crate::blocks::generate::BlockGenerate {
        levels: Some(levels),
        placed: Some(Arc::new(crate::generate::WorldBiomes {
            regions: Some(regions),
            biomes,
            blocks: vec![crate::generate::BiomeBlocks { top: 4, ..Default::default() }],
        })),
        ..Default::default()
    };
    crate::blocks::generate::landed(&generate, &world)
}

/// The sites of a dungeon placed every 400 m on land within 2 km.
fn sites(world: &WorldGen) -> Vec<[i32; 2]> {
    let placement = Placement { spacing: 400.0, separation: 100.0, chance: 1.0, slope: 80.0, ..Placement::default() };
    placement
        .tries_near(world.seed(), [-2000.0; 2], [2000.0; 2], 0.0)
        .into_iter()
        .filter(|site| world.height_at(site.at[0], site.at[1]) > world.sea_level() + 3.0)
        .map(|site| [site.at[0].floor() as i32, site.at[1].floor() as i32])
        .collect()
}

/// Checks every space of `layout` lies wholly under the land, with
/// `cover` blocks of rock over it and round it but for a block or two the
/// land's coarse reading misses, and every passage under the land too.
fn assert_under(layout: &DungeonLayout, ground: &Ground, cover: i32) {
    for space in layout.spaces.iter().filter(|space| space.height >= 0) {
        let (low, high) = (space.low(), space.high());
        let mut columns = Vec::new();
        for z in (low[1] - 4..=high[1] + 4).step_by(2) {
            for x in (low[0] - 4..=high[0] + 4).step_by(2) {
                columns.push([x, z]);
            }
        }
        let least = ground.tops(&columns).into_iter().min().unwrap();
        assert!(
            least >= space.top() + cover - 3,
            "a {:?} on floor {} reaches {} with the land at {} over it",
            space.kind,
            space.floor,
            space.top(),
            least
        );
    }
    for way in layout.ways.iter().filter(|way| way.kind == WayKind::Passage && way.to < layout.spaces.len()) {
        let r = way.width as f32 * 0.5;
        let cells: Vec<&WayCell> = way.cells.iter().filter(|cell| cell.off <= r).step_by(3).collect();
        let tops = ground.tops(&cells.iter().map(|cell| cell.at).collect::<Vec<_>>());
        for (cell, top) in cells.iter().zip(tops) {
            let roof = cell.surface.div_euclid(2) + way.height;
            assert!(top > roof + 1, "a passage's roof at {roof} comes out under land at {top}, at {:?}", cell.at);
        }
    }
}

#[test]
fn every_space_lies_under_the_land_with_rock_over_it_in_rolling_hills_and_on_escarpments() {
    let dungeon = temple();
    let mut planned = 0;
    let mut kinds = std::collections::BTreeSet::new();
    for world in [Arc::new(Preset::RollingHills.world()), escarpments()] {
        let ground = Ground { world: &world, size: 1.0, position: [0.0, 0.0], biomes: None, halved: false };
        for site in sites(&world).into_iter().take(10) {
            let Some(layout) = plan(&dungeon, &ground, site, 7, -400) else { continue };
            planned += 1;
            kinds.insert(format!("{:?}", layout.entrance.kind));
            assert_under(&layout, &ground, dungeon.cover as i32);
        }
    }
    assert!(planned >= 6, "{planned} dungeons planned");
    assert!(kinds.len() >= 2, "entrances of one kind alone: {kinds:?}");
}

#[test]
fn a_dungeon_goes_down_floor_by_floor_through_its_set_pieces_to_the_boss() {
    let dungeon = temple();
    let world = Preset::RollingHills.world();
    let ground = Ground { world: &world, size: 1.0, position: [0.0, 0.0], biomes: None, halved: false };
    let layout = sites(&world).into_iter().find_map(|site| plan(&dungeon, &ground, site, 7, -400)).expect("a dungeon");
    let live: Vec<&Space> = layout.spaces.iter().filter(|space| space.height >= 0).collect();
    assert_eq!(layout.floors, 2);
    assert_eq!(live.iter().filter(|space| space.kind == SpaceKind::Arena).count(), 1, "one boss");
    assert!(live.iter().any(|space| space.kind == SpaceKind::Descent), "a shaft down");
    assert!(live.iter().any(|space| space.kind == SpaceKind::Hall), "a pillared hall");
    assert!(live.iter().any(|space| space.kind == SpaceKind::Gorge), "a gorge");
    // Every space reached from the landing along the ways.
    let mut reached = vec![false; layout.spaces.len()];
    reached[0] = true;
    loop {
        let mut more = false;
        for way in &layout.ways {
            if let Some(from) = way.from
                && way.to < reached.len()
                && reached[from] != reached[way.to]
            {
                reached[from] = true;
                reached[way.to] = true;
                more = true;
            }
        }
        if !more {
            break;
        }
    }
    for (at, space) in layout.spaces.iter().enumerate() {
        assert!(space.height < 0 || reached[at], "a {:?} no way reaches", space.kind);
    }
    // Spaces as large as a third-person camera wants: halls 24 m high and more, rooms 16 m across.
    for space in &live {
        match space.kind {
            SpaceKind::Hall => assert!(space.height >= 24 && space.half[0].max(space.half[1]) * 2 >= 48),
            SpaceKind::Room => assert!(space.half[0] * 2 >= 16 && space.height >= 8),
            SpaceKind::Arena => assert!(space.half[0] * 2 >= 32 && space.height >= 20),
            _ => {}
        }
    }
    // The same from the same seed.
    let again = sites(&world).into_iter().find_map(|site| plan(&dungeon, &ground, site, 7, -400)).unwrap();
    assert_eq!(again, layout);
}

#[test]
#[ignore]
fn timing_probe() {
    let dungeon = temple();
    let world = Preset::RollingHills.world();
    let ground = Ground { world: &world, size: 1.0, position: [0.0, 0.0], biomes: None, halved: false };
    for site in sites(&world).into_iter().take(6) {
        let t = std::time::Instant::now();
        let tl = t.elapsed();
        let t = std::time::Instant::now();
        let layout = plan(&dungeon, &ground, site, 7, -400);
        eprintln!(
            "land {:?} plan {:?} ways (of {:?}) {:?} spaces {:?}",
            tl,
            t.elapsed(),
            layout.as_ref().map(|l| l.ways.len()),
            layout.as_ref().map(|l| l.joins),
            layout.as_ref().map(|l| l.spaces.iter().filter(|s| s.height >= 0).count())
        );
    }
}
