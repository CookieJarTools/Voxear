//! The Entities panel, the ECS inspector: the world's archetypes (each set of
//! components entities share, with how many entities and chunks it has),
//! the entities of the one picked, and the picked entity's components with
//! their values, beside the engine's stages and the systems in each, with
//! their milliseconds when the Profiler is recording. A filter of component
//! names, as "Transform, Spin", keeps the archetypes that have them all, so
//! it shows what a query over them would walk.
//!
//! Picking an entity that is an Obj selects it, so the Inspector shows and
//! edits it, with undo and saving. A plain entity's values are edited here,
//! in the running world: its Transform, its Name and its C# components'
//! fields. The picked archetype's chunks show how full each is. The lists
//! are read again a few times a second while the panel shows, so they
//! follow a running game without costing it every frame, and the values
//! are left alone while one is being typed in.

use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use voxear_core::{
    math::{EulerRot, Quat, Vec3},
    profiling::Samples,
};
use voxear_ecs::{ComponentId, Entity, World};
use voxear_engine::{Game, STAGES, script_types::ScriptTypes};
use voxear_scene::{EditorOnly, Name, Transform};
use voxear_ui::{ItemFns, NodeId, Style, Ui};

use crate::{fields::Step, props::Value};

/// How often the lists are read again while the panel shows.
const REFRESH: Duration = Duration::from_millis(400);
const ROW: f32 = 22.0;

/// An archetype as the list shows it.
#[derive(Clone, Debug, PartialEq)]
struct ArchetypeRow {
    components: Vec<ComponentId>,
    names: String,
    entities: usize,
    chunks: usize,
}

/// The most chunks the strip shows; past them it says how many more.
const SHOWN_CHUNKS: usize = 64;

/// A value of a plain entity that can be changed: the field showing it, and
/// where what is typed goes.
struct Editable {
    field: NodeId,
    entity: Entity,
    target: Target,
    /// The value as it showed, which says how to read what is typed.
    shown: Value,
}

enum Target {
    /// The Transform's position, rotation in degrees, or scale.
    Transform(usize),
    Name,
    /// A C# component's field, by the component's full name.
    Script {
        component: String,
        field: String,
    },
}

/// What the lists show, shared with their rows.
#[derive(Default)]
struct Shown {
    archetypes: Vec<ArchetypeRow>,
    entities: Vec<(Entity, String)>,
}

pub struct EntitiesPanel {
    summary: NodeId,
    filter: NodeId,
    archetypes: NodeId,
    entities: NodeId,
    details: NodeId,
    schedule: NodeId,
    /// The picked archetype's chunks, each filled as far as it is.
    chunks: NodeId,
    chunk_summary: NodeId,
    editables: Vec<Editable>,
    shown: Rc<RefCell<Shown>>,
    picked_archetype: Option<Vec<ComponentId>>,
    picked_entity: Option<Entity>,
    /// The filter as the lists were last read with it, to read them again as it is typed.
    filtered: String,
    read: Option<Instant>,
}

impl EntitiesPanel {
    pub fn new(ui: &mut Ui, root: NodeId) -> Self {
        let bar = ui.panel(root, Style::row());
        ui.add_class(bar, "console-toolbar");
        let summary = ui.add_label(bar, "");
        ui.add_class(summary, "entities-summary");
        let search = ui.panel(bar, Style::row());
        ui.add_class(search, "search");
        ui.add_class(search, "entities-search");
        let icon = ui.icon(search, "search");
        ui.add_class(icon, "search-icon");
        let filter = ui.text_field(search, "");
        ui.set_placeholder(filter, "Components, as Transform, Spin");
        ui.add_class(filter, "search-field");
        let line = ui.panel(root, Style::default());
        ui.add_class(line, "divider");

        let columns = ui.panel(root, Style::row());
        ui.add_class(columns, "fill");
        ui.add_class(columns, "entities-columns");
        let shown: Rc<RefCell<Shown>> = Rc::default();
        // Side by side, with lines between that drag to share the room out.
        let panes = crate::columns::panes(ui, columns, 4);
        let column = |ui: &mut Ui, pane: NodeId, title: &str, class: &str| {
            let column = ui.panel(pane, Style::default());
            ui.add_class(column, "entities-column");
            ui.add_class(column, class);
            let heading = ui.add_label(column, title);
            ui.add_class(heading, "profiler-heading");
            column
        };

        let left = column(ui, panes[0], "Archetypes", "entities-archetypes");
        let rows = shown.clone();
        // Counts first, at a fixed width, so a long set of components runs off the
        // list's edge rather than over its count.
        let archetypes = ui.list_view(
            left,
            ROW,
            0,
            ItemFns {
                make: make_archetype_row,
                bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                    let shown: std::cell::Ref<Shown> = rows.borrow();
                    let Some(archetype) = shown.archetypes.get(index) else { return };
                    let chunks =
                        if archetype.chunks == 1 { "1 chunk".into() } else { format!("{} chunks", archetype.chunks) };
                    fill_row(ui, row, &format!("{} · {chunks}", archetype.entities), &archetype.names);
                },
            },
        );
        ui.add_class(archetypes, "fill");

        let middle = column(ui, panes[1], "Entities", "entities-entities");
        let chunk_summary = ui.add_label(middle, "");
        ui.add_class(chunk_summary, "entities-hint");
        let chunks = ui.panel(middle, Style::row());
        ui.add_class(chunks, "entities-chunks");
        let rows = shown.clone();
        let entities = ui.list_view(
            middle,
            ROW,
            0,
            ItemFns {
                make: make_row,
                bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                    let shown: std::cell::Ref<Shown> = rows.borrow();
                    let Some((entity, name)) = shown.entities.get(index) else { return };
                    fill_row(ui, row, name, &format!("{}v{}", entity.index(), entity.generation()));
                },
            },
        );
        ui.add_class(entities, "fill");

        let details = column(ui, panes[2], "Components", "entities-details");
        let schedule = column(ui, panes[3], "Systems", "entities-schedule");
        Self {
            summary,
            filter,
            archetypes,
            entities,
            details,
            schedule,
            chunks,
            chunk_summary,
            editables: Vec::new(),
            shown,
            picked_archetype: None,
            picked_entity: None,
            filtered: String::new(),
            read: None,
        }
    }

    /// Applies values typed in, reads the world again when due, or when a
    /// pick or the filter changed, and returns an Obj picked in the list,
    /// for the editor to select.
    pub fn update(
        &mut self,
        ui: &mut Ui,
        shown: bool,
        game: &mut Game,
        types: &ScriptTypes,
        frame: Option<&Samples>,
    ) -> Option<Entity> {
        if !shown {
            return None;
        }
        self.apply_edits(ui, &mut game.engine.world, types);
        let world = &game.engine.world;
        let mut picked_obj = None;
        if ui.selection_changed(self.archetypes) {
            let index = ui.selection(self.archetypes).first().copied();
            self.picked_archetype =
                index.and_then(|i| self.shown.borrow().archetypes.get(i).map(|a| a.components.clone()));
            self.picked_entity = None;
            self.read = None;
        }
        if ui.selection_changed(self.entities) {
            let index = ui.selection(self.entities).first().copied();
            self.picked_entity = index.and_then(|i| self.shown.borrow().entities.get(i).map(|(e, _)| *e));
            picked_obj = self.picked_entity.filter(|&e| world.has::<Name>(e) && world.has::<Transform>(e));
            self.read = None;
        }
        if ui.text(self.filter) != self.filtered {
            self.filtered = ui.text(self.filter);
            self.read = None;
        }
        if self.read.is_some_and(|read| read.elapsed() < REFRESH) {
            return picked_obj;
        }
        self.read = Some(Instant::now());
        self.read_world(ui, world);
        // Rebuilt, the field being typed in would lose what is typed.
        let typing = ui.focused().is_some_and(|focused| self.editables.iter().any(|e| e.field == focused));
        if !typing {
            self.show_details(ui, world, types);
        }
        self.show_schedule(ui, game, types, frame);
        picked_obj
    }

    /// Writes what was typed into a value into the world, when it reads as
    /// that value: numbers, or a vector's or colour's numbers between commas.
    fn apply_edits(&mut self, ui: &Ui, world: &mut World, types: &ScriptTypes) {
        for editable in &self.editables {
            if !ui.changed(editable.field) || !world.is_alive(editable.entity) {
                continue;
            }
            let typed = ui.text(editable.field);
            let Some(value) = parse(&editable.shown, &typed) else { continue };
            let entity = editable.entity;
            match (&editable.target, value) {
                (Target::Transform(part), Value::Vector(v)) => {
                    let Some(transform) = world.get_mut::<Transform>(entity) else { continue };
                    let v = Vec3::new(v[0], v[1], v[2]);
                    match part {
                        0 => transform.position = v,
                        1 => {
                            let [x, y, z] = v.to_array().map(f32::to_radians);
                            transform.rotation = Quat::from_euler(EulerRot::YXZ, y, x, z);
                        }
                        _ => transform.scale = v,
                    }
                }
                (Target::Name, Value::Text(text)) => {
                    if let Some(name) = world.get_mut::<Name>(entity) {
                        name.0 = text;
                    }
                }
                (Target::Script { component, field }, value) => {
                    crate::live::set(world, types, entity, component, &[Step::Key(field.clone())], &value);
                }
                _ => {}
            }
        }
    }

    /// The archetypes the filter keeps, and the picked one's entities.
    fn read_world(&mut self, ui: &mut Ui, world: &World) {
        let terms: Vec<String> =
            ui.text(self.filter).split(',').map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty()).collect();
        let hidden = world.components().id_of::<EditorOnly>();
        let mut archetypes = Vec::new();
        let (mut entity_count, mut chunk_count) = (0, 0);
        for archetype in world.archetypes() {
            if archetype.is_empty() || hidden.is_some_and(|id| archetype.has(id)) {
                continue;
            }
            let names: Vec<String> = archetype.components().iter().map(|&id| short(world, id)).collect();
            let keeps = terms.iter().all(|term| names.iter().any(|name| name.to_lowercase() == *term))
                || terms.iter().all(|term| names.iter().any(|name| name.to_lowercase().contains(term.as_str())));
            if !keeps {
                continue;
            }
            entity_count += archetype.len();
            chunk_count += archetype.chunk_count();
            archetypes.push(ArchetypeRow {
                components: archetype.components().to_vec(),
                names: names.join(" · "),
                entities: archetype.len(),
                chunks: archetype.chunk_count(),
            });
        }
        // The busiest first, as what matters most usually has the most.
        archetypes.sort_by(|a, b| b.entities.cmp(&a.entities).then_with(|| a.names.cmp(&b.names)));
        let what = if terms.is_empty() { "" } else { " match" };
        let summary = format!("{entity_count} entities in {} archetypes{what}, {chunk_count} chunks", archetypes.len());
        ui.set_text(self.summary, &summary);

        let picked =
            self.picked_archetype.as_ref().and_then(|key| archetypes.iter().position(|a| a.components == *key));
        let entities: Vec<(Entity, String)> = picked
            .and_then(|index| world.archetypes().iter().find(|a| a.components() == archetypes[index].components))
            .map(|archetype| {
                (0..archetype.chunk_count())
                    .flat_map(|chunk| archetype.chunk_entities(chunk).iter().copied())
                    .map(|entity| (entity, world.get::<Name>(entity).map_or_else(|| "Entity".into(), |n| n.0.clone())))
                    .collect()
            })
            .unwrap_or_default();
        let picked_archetype =
            picked.and_then(|index| world.archetypes().iter().find(|a| a.components() == archetypes[index].components));
        self.show_chunks(ui, picked_archetype);
        let picked_entity = self.picked_entity.and_then(|picked| entities.iter().position(|(e, _)| *e == picked));
        let (archetype_count, entity_rows) = (archetypes.len(), entities.len());
        *self.shown.borrow_mut() = Shown { archetypes, entities };
        ui.set_list_count(self.archetypes, archetype_count);
        ui.set_list_count(self.entities, entity_rows);
        ui.set_selection(self.archetypes, picked.as_slice());
        ui.set_selection(self.entities, picked_entity.as_slice());
        // Selections set here are not the user's picks.
        let _ = (ui.selection_changed(self.archetypes), ui.selection_changed(self.entities));
    }

    /// The picked archetype's chunks, each a bar filled as far as the chunk
    /// is, with how full they are together.
    fn show_chunks(&mut self, ui: &mut Ui, archetype: Option<&voxear_ecs::Archetype>) {
        for child in ui.children(self.chunks) {
            ui.remove(child);
        }
        let Some(archetype) = archetype else {
            ui.set_visible(self.chunk_summary, false);
            return;
        };
        let capacity = archetype.chunk_capacity().max(1);
        let count = archetype.chunk_count();
        for chunk in 0..count.min(SHOWN_CHUNKS) {
            let held = archetype.chunk_entities(chunk).len();
            let bar = ui.panel(self.chunks, Style::default());
            ui.add_class(bar, "entities-chunk");
            let fill = ui.panel(bar, Style::default());
            ui.add_class(fill, "entities-chunk-fill");
            ui.set_inline_style(fill, &format!("height: {}%", held * 100 / capacity));
        }
        let full = archetype.len() * 100 / (count * capacity).max(1);
        let more = if count > SHOWN_CHUNKS { format!(", the first {SHOWN_CHUNKS} shown") } else { String::new() };
        let chunks = if count == 1 { "1 chunk".to_string() } else { format!("{count} chunks") };
        ui.set_text(self.chunk_summary, &format!("{chunks} holding {capacity} each · {full}% full{more}"));
        ui.set_visible(self.chunk_summary, true);
    }

    /// The picked entity's components, each with what it holds; a plain
    /// entity's values are fields to change.
    fn show_details(&mut self, ui: &mut Ui, world: &World, types: &ScriptTypes) {
        clear(ui, self.details);
        self.editables.clear();
        let Some(entity) = self.picked_entity.filter(|&e| world.is_alive(e)) else {
            let hint = ui.add_label(self.details, "Pick an entity to see its components.");
            ui.add_class(hint, "entities-hint");
            return;
        };
        let Some(archetype) =
            world.archetypes().iter().find(|a| (0..a.chunk_count()).any(|c| a.chunk_entities(c).contains(&entity)))
        else {
            return;
        };
        // An Obj is changed in the Inspector, where it can be undone and saved.
        let plain = !(world.has::<Name>(entity) && world.has::<Transform>(entity));
        for &id in archetype.components() {
            let name = short(world, id);
            let full = world.components().info(id).name().to_owned();
            let card = ui.panel(self.details, Style::default());
            ui.add_class(card, "entities-component");
            let title = ui.add_label(card, &name);
            ui.add_class(title, "entities-component-name");
            for (field, value, target) in values(world, types, entity, id, &full) {
                let row = ui.panel(card, Style::row());
                ui.add_class(row, "entities-value");
                let label = ui.add_label(row, &field);
                ui.add_class(label, "entities-value-name");
                let editable = target.filter(|_| plain && parse(&value, &text(&value)).is_some());
                let Some(target) = editable else {
                    let shown = ui.add_label(row, &text(&value));
                    ui.add_class(shown, "entities-value-text");
                    continue;
                };
                let input = ui.text_field(row, &text(&value));
                ui.add_class(input, "value-input");
                ui.add_class(input, "entities-value-input");
                self.editables.push(Editable { field: input, entity, target, shown: value });
            }
        }
    }

    /// Each stage, with its C# systems and then the engine's, as they run.
    fn show_schedule(&mut self, ui: &mut Ui, game: &Game, types: &ScriptTypes, frame: Option<&Samples>) {
        clear(ui, self.schedule);
        let ms = |name: &str| frame.and_then(|samples| samples.find(name)).map(|node| frame.unwrap().total_ms(node));
        let engine: Vec<(&str, Vec<String>)> = game
            .engine
            .schedule
            .stages()
            .map(|(stage, systems)| (stage, systems.map(str::to_owned).collect()))
            .collect();
        for stage in STAGES {
            let heading = ui.add_label(self.schedule, stage);
            ui.add_class(heading, "entities-stage");
            let mut scripts: Vec<_> = types.systems.iter().filter(|s| s.stage == *stage).collect();
            scripts.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.name.cmp(&b.name)));
            let scripted = scripts.iter().map(|s| (s.name.clone(), ms(&format!("{}.Update()", s.name)), true));
            let own = engine.iter().filter(|(s, _)| s == stage).flat_map(|(_, systems)| systems.clone());
            let rows: Vec<(String, Option<f32>, bool)> = scripted
                .chain(own.map(|name| {
                    let t = ms(&name);
                    (name, t, false)
                }))
                .collect();
            if rows.is_empty() {
                let none = ui.add_label(self.schedule, "No systems");
                ui.add_class(none, "entities-hint");
            }
            for (name, time, script) in rows {
                let row = ui.panel(self.schedule, Style::row());
                ui.add_class(row, "entities-value");
                let label = ui.add_label(row, &name);
                ui.add_class(label, "entities-value-name");
                ui.set_class(label, "entities-script", script);
                let shown = ui.add_label(row, &time.map_or_else(String::new, |ms| format!("{ms:.2} ms")));
                ui.add_class(shown, "entities-value-text");
            }
        }
    }

    pub fn filter(&self) -> NodeId {
        self.filter
    }

    pub fn archetypes(&self) -> NodeId {
        self.archetypes
    }

    pub fn entities(&self) -> NodeId {
        self.entities
    }

    pub fn details(&self) -> NodeId {
        self.details
    }
}

fn make_row(ui: &mut Ui, row: NodeId) {
    ui.add_class(row, "entities-row");
    let name = ui.add_label(row, "");
    ui.add_class(name, "entities-row-name");
    let count = ui.add_label(row, "");
    ui.add_class(count, "entities-row-count");
}

fn make_archetype_row(ui: &mut Ui, row: NodeId) {
    ui.add_class(row, "entities-row");
    let count = ui.add_label(row, "");
    ui.add_class(count, "entities-row-count");
    ui.add_class(count, "entities-archetype-count");
    let names = ui.add_label(row, "");
    ui.add_class(names, "entities-row-name");
}

/// Fills a row's two labels, in the order its make put them.
fn fill_row(ui: &mut Ui, row: NodeId, first: &str, second: &str) {
    let children = ui.children(row);
    if let [one, two, ..] = children[..] {
        ui.set_text(one, first);
        ui.set_text(two, second);
    }
}

fn clear(ui: &mut Ui, column: NodeId) {
    // The heading stays.
    for child in ui.children(column).into_iter().skip(1) {
        ui.remove(child);
    }
}

/// A component's name as people say it: a C# struct's full name without
/// its namespace, and a Rust type's without its path.
fn short(world: &World, id: ComponentId) -> String {
    let name = world.components().info(id).name();
    let name = name.rsplit("::").next().unwrap_or(name);
    name.rsplit('.').next().unwrap_or(name).to_owned()
}

/// What a component holds, as a field's name, its value and where a change
/// to it goes, for the ones whose layout is known: the Transform, a Name,
/// and C# components.
fn values(
    world: &World,
    types: &ScriptTypes,
    entity: Entity,
    id: ComponentId,
    full: &str,
) -> Vec<(String, Value, Option<Target>)> {
    if world.components().id_of::<Transform>() == Some(id) {
        let Some(t) = world.get::<Transform>(entity) else { return Vec::new() };
        let (y, x, z) = t.rotation.to_euler(EulerRot::YXZ);
        let three = |v: [f32; 3]| Value::Vector(v.to_vec());
        return vec![
            ("Position".into(), three(t.position.to_array()), Some(Target::Transform(0))),
            ("Rotation".into(), three([x, y, z].map(f32::to_degrees)), Some(Target::Transform(1))),
            ("Scale".into(), three(t.scale.to_array()), Some(Target::Transform(2))),
        ];
    }
    if world.components().id_of::<Name>() == Some(id) {
        let Some(name) = world.get::<Name>(entity) else { return Vec::new() };
        return vec![("Name".into(), Value::Text(name.0.clone()), Some(Target::Name))];
    }
    let Some(declared) = types.components.iter().find(|c| c.name == full) else { return Vec::new() };
    declared
        .fields
        .iter()
        .filter_map(|field| {
            let path = [Step::Key(field.name.clone())];
            let value = crate::live::get(world, types, entity, full, &path)?;
            let target = Target::Script { component: full.to_owned(), field: field.name.clone() };
            Some((crate::props::nice_name(&field.name), value, Some(target)))
        })
        .collect()
}

/// What is typed into a value's field, read as the kind of value it showed;
/// `None` when it does not read as one, or the kind is not typed in.
fn parse(shown: &Value, typed: &str) -> Option<Value> {
    let numbers = |count: usize| -> Option<Vec<f32>> {
        let parts: Vec<f32> = typed.split(',').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
        (parts.len() == count).then_some(parts)
    };
    Some(match shown {
        Value::Number(_) => Value::Number(typed.trim().parse().ok()?),
        Value::Whole(_) => Value::Whole(typed.trim().parse().ok()?),
        Value::Text(_) => Value::Text(typed.to_owned()),
        Value::Vector(v) => Value::Vector(numbers(v.len())?),
        Value::Color(_) => {
            let rgb = numbers(3)?;
            Value::Color([rgb[0], rgb[1], rgb[2]])
        }
        _ => return None,
    })
}

fn text(value: &Value) -> String {
    match value {
        Value::Number(n) => format!("{n}"),
        Value::Whole(n) => n.to_string(),
        Value::Toggle(b) => {
            if *b {
                "Yes".into()
            } else {
                "No".into()
            }
        }
        // Adding zero turns a rounding's -0.00 into 0.00.
        Value::Vector(v) => v.iter().map(|x| format!("{:.2}", x + 0.0)).collect::<Vec<_>>().join(", "),
        Value::Color(rgb) => rgb.iter().map(|x| format!("{x:.3}")).collect::<Vec<_>>().join(", "),
        Value::Choice(index, names) => names.get(*index).cloned().unwrap_or_default(),
        Value::Obj(name) | Value::Text(name) | Value::Asset(name) | Value::Fixed(name) => name.clone(),
        _ => String::new(),
    }
}
