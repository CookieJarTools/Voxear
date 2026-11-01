//! A behaviour's code, run for the Inspector while editing: `[Button]`s,
//! `[OnValueChanged]` callbacks, `[ValidateInput]` checks, the values a
//! `[ValueDropdown]` offers, and the drawers fields are drawn with (see
//! `field_drawers`). The scripts start for it without playing, as a
//! guide's check starts them, and each call runs on a copy of the behaviour
//! made from the Obj's scene values (see `voxear_scripting::InspectorAsk`).
//! The fields a method changes go back into the scene: a callback's with
//! the change that called it, as one step to undo, and a button's as a step
//! of its own. Checks and values are asked again only when the selection or
//! the Obj's scene values change, so a method that throws is not asked over
//! and over.

use voxear_ecs::{Entity, World};
use voxear_engine::{
    EditBehaviours,
    script_types::{FieldKind, ScriptTypes},
};
use voxear_scene::BehaviourSpec;
use voxear_scripting::{DrawerField, InspectorAnswer, InspectorAsk};

use super::Editor;
use crate::{
    field_drawers::ToOpen,
    fields,
    history::{Edit, ObjState},
    inspector::ScriptAnswers,
    props,
};

impl Editor {
    /// Calls the `[OnValueChanged]` methods of fields the Inspector changed
    /// this frame, folding what they change into that change.
    pub(super) fn run_inspector_callbacks(&mut self, edit: Option<Edit>) -> Option<Edit> {
        let calls = self.inspector.take_callbacks();
        let entity = self.inspector.shown().filter(|_| !calls.is_empty() && !self.is_playing());
        let Some(entity) = entity else { return edit };
        let before = match &edit {
            Some(Edit::Changed { before, .. }) => before.clone(),
            _ => ObjState::of(&self.game.engine.world, entity)?,
        };
        for (behaviour, method) in calls {
            self.call_behaviour(entity, &behaviour, &method);
        }
        let after = ObjState::of(&self.game.engine.world, entity)?;
        (before != after).then_some(Edit::Changed { entity, before, after })
    }

    /// Opens the shown fields' drawers once the scripts have started, and
    /// draws those to be drawn this frame, folding what they change into
    /// the frame's edit.
    pub(super) fn run_inspector_drawers(&mut self, edit: Option<Edit>) -> Option<Edit> {
        let Some(entity) = self.inspector.shown().filter(|_| !self.is_playing()) else { return edit };
        if let Some(fields) = self.inspector.drawers_to_open() {
            let asked: Vec<DrawerField> = fields.iter().map(ToOpen::field).collect();
            let opened = self.with_edit_scripts(|scripts, game| scripts.open_drawers(&mut game.engine.world, &asked));
            let answers = match opened {
                // Opened once the scripts have started.
                None => return edit,
                Some(Ok(answers)) => answers,
                Some(Err(error)) => {
                    tracing::error!("{error:#}");
                    asked.iter().map(|_| Err(format!("{error}"))).collect()
                }
            };
            self.inspector.place_drawers(&mut self.ui, &mut self.game, answers);
        }
        let requests = self.inspector.drawer_requests(&self.ui, &self.game);
        let Some(scripts) = self.play.edit_scripts().filter(|_| !requests.is_empty()) else { return edit };
        let before = match &edit {
            Some(Edit::Changed { before, .. }) => before.clone(),
            _ => ObjState::of(&self.game.engine.world, entity)?,
        };
        let mut ui = self.inspector.drawer_ui(&mut self.ui);
        let drawn = scripts.draw_fields(&mut self.game.engine.world, &mut ui, &requests);
        match drawn {
            Ok(answers) => {
                for error in self.inspector.take_drawn(&mut self.game, answers) {
                    self.notify(&error);
                }
            }
            Err(error) => tracing::error!("{error:#}"),
        }
        let after = ObjState::of(&self.game.engine.world, entity)?;
        (before != after).then_some(Edit::Changed { entity, before, after })
    }

    /// Calls the `[Button]` clicked this frame, as a step of its own to undo.
    pub(super) fn run_inspector_button(&mut self) {
        let Some((behaviour, method)) = self.inspector.clicked_button(&self.ui) else { return };
        let Some(entity) = self.inspector.shown() else { return };
        let Some(before) = ObjState::of(&self.game.engine.world, entity) else { return };
        if !self.call_behaviour(entity, &behaviour, &method) {
            return;
        }
        let Some(after) = ObjState::of(&self.game.engine.world, entity) else { return };
        if before != after {
            self.history.record_step(Edit::Changed { entity, before, after });
            self.unsaved = true;
        }
    }

    /// Calls a method on a copy of the behaviour made from the Obj's scene
    /// values, and writes the fields it changed into the scene. False when
    /// the scripts are not ready, as while they start, or it failed, which
    /// the status bar says.
    fn call_behaviour(&mut self, entity: Entity, behaviour: &str, method: &str) -> bool {
        let Some(spec) = spec_of(&self.game.engine.world, entity, behaviour) else { return false };
        let asked = self.with_edit_scripts(|scripts, game| {
            scripts.inspector(&mut game.engine.world, entity, &spec, InspectorAsk::Call(method))
        });
        match asked {
            None => self.notify("The scripts are starting; try again in a moment"),
            Some(Ok(InspectorAnswer::Changed(changed))) => {
                let described = self.inspector.types().behaviour(behaviour).cloned();
                let world = &mut self.game.engine.world;
                for (name, value) in changed {
                    // JSON writes 10.0 as 10, but a float field stays a float in the scene.
                    let kind = described.as_ref().and_then(|b| b.field(&name)).map(|f| &f.kind);
                    let value = match (kind, value) {
                        (Some(FieldKind::Number), toml::Value::Integer(i)) => toml::Value::Float(i as f64),
                        (_, value) => value,
                    };
                    fields::set_raw(world, entity, behaviour, &name, value);
                }
                return true;
            }
            Some(Ok(_)) => {}
            Some(Err(error)) => {
                tracing::error!("{error:#}");
                self.notify(&format!("{error}"));
            }
        }
        false
    }

    /// Asks the shown Obj's behaviours what their `[ValidateInput]` checks
    /// and `[ValueDropdown]`s say, when the selection or its scene values
    /// changed, and gives the Inspector the answers.
    pub(super) fn ask_inspector_answers(&mut self) {
        let Some(entity) = self.inspector.shown().filter(|_| !self.is_playing()) else { return };
        let world = &self.game.engine.world;
        let Some(kept) = world.get::<EditBehaviours>(entity) else { return };
        let asks = questions(self.inspector.types(), &kept.0);
        let key = format!("{entity:?} {:?}", kept.0.iter().map(|spec| &spec.fields).collect::<Vec<_>>());
        if !self.inspector.ask_again(&key) {
            return;
        }
        if asks.is_empty() {
            return self.inspector.set_answers(&mut self.ui, ScriptAnswers::default());
        }
        let specs = kept.0.clone();
        let answered = self.with_edit_scripts(|scripts, game| answer(scripts, game, entity, &specs, &asks));
        match answered {
            Some(answers) => self.inspector.set_answers(&mut self.ui, answers),
            // Asked again once the scripts have started.
            None => {
                self.inspector.ask_again("");
            }
        }
    }
}

/// What the Inspector asks a behaviour about one of its fields.
struct Question {
    behaviour: String,
    field: String,
    kind: voxear_engine::script_types::FieldKind,
    label: String,
    validate: Option<(String, String)>,
    dropdown: Option<String>,
}

/// The questions the shown behaviours' fields ask.
fn questions(types: &ScriptTypes, specs: &[BehaviourSpec]) -> Vec<Question> {
    let described = specs.iter().filter_map(|spec| types.behaviour(&spec.type_name));
    described
        .flat_map(|behaviour| behaviour.fields.iter().map(move |field| (behaviour, field)))
        .filter_map(|(behaviour, field)| {
            let meta = &field.inspector;
            let validate = meta.validate.as_ref().map(|v| (v.method.clone(), v.message.clone()));
            (validate.is_some() || meta.dropdown.is_some()).then(|| Question {
                behaviour: behaviour.name.clone(),
                field: field.name.clone(),
                kind: field.kind.clone(),
                label: meta.label.clone().unwrap_or_else(|| props::nice_name(&field.name)),
                validate,
                dropdown: meta.dropdown.clone(),
            })
        })
        .collect()
}

/// Asks each question of a copy of its behaviour. A question whose method
/// threw goes unanswered; the Console has what it threw.
fn answer(
    scripts: &mut voxear_scripting::ScriptRuntime,
    game: &mut voxear_engine::Game,
    entity: Entity,
    specs: &[BehaviourSpec],
    questions: &[Question],
) -> ScriptAnswers {
    let mut answers = ScriptAnswers::default();
    for question in questions {
        let Some(spec) = specs.iter().find(|s| s.type_name == question.behaviour) else { continue };
        let key = (question.behaviour.clone(), question.field.clone());
        let world = &mut game.engine.world;
        if let Some((method, message)) = &question.validate {
            let ask = InspectorAsk::Validate { method, field: &question.field };
            if let Ok(InspectorAnswer::Valid(false, said)) = scripts.inspector(world, entity, spec, ask) {
                let error = [said, message.clone()].into_iter().find(|m| !m.is_empty());
                answers.errors.insert(key.clone(), error.unwrap_or_else(|| format!("{} is not valid", question.label)));
            }
        }
        if let Some(member) = &question.dropdown
            && let Ok(InspectorAnswer::Options(options)) =
                scripts.inspector(world, entity, spec, InspectorAsk::Dropdown(member))
        {
            let world = &game.engine.world;
            let shown =
                options.into_iter().map(|(label, value)| (label, fields::typed(world, &question.kind, Some(&value))));
            answers.options.insert(key, shown.collect());
        }
    }
    answers
}

fn spec_of(world: &World, entity: Entity, behaviour: &str) -> Option<BehaviourSpec> {
    world.get::<EditBehaviours>(entity)?.0.iter().find(|s| s.type_name == behaviour).cloned()
}
