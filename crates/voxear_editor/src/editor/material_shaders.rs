//! A material's custom shader, picked in its Inspector (see
//! `shader_fields`): the shader itself, from the object picker or dropped
//! from the Project panel, and its picture properties. Each is written into
//! the material's file and shows at once wherever it is drawn.

use voxear_assets::AssetKind;
use voxear_engine::{
    MaterialData, PropertyValue,
    shaders::{BuiltinShader, ShaderRef},
};
use voxear_render::Renderer;
use voxear_ui::{DragSource, NodeId};

use super::Editor;
use crate::project_view::ProjectEvent;
use crate::{
    asset_inspector::AssetAction,
    fields::Pick,
    object_picker::{self, Choice, Chosen, PickFor},
    project_files::ShaderStart,
};

impl Editor {
    /// Opens the object picker under `anchor` for the shader of the
    /// material at `material`: the Standard shader, one built in, or one of
    /// the project's.
    pub(super) fn browse_material_shader(&mut self, material: &str, anchor: NodeId) {
        // The project's shaders, but those that draw UI, which draw no materials.
        let registry = self.game.content.shaders();
        let draws_ui =
            |path: &str| registry.shaders.iter().any(|s| s.path == path && s.info.as_ref().is_ok_and(|info| info.ui || info.look));
        let db = self.game.content.db();
        let materials = |e: &&voxear_assets::AssetEntry| e.kind == AssetKind::Shader && !draws_ui(&e.path);
        let mut paths: Vec<String> = db.entries().filter(materials).map(|e| e.path.clone()).collect();
        paths.sort();
        let mut listed = vec![("None".to_string(), Pick::Asset(None))];
        listed.extend(paths.into_iter().map(|path| (path.clone(), Pick::Asset(Some(path)))));
        let mut choices = object_picker::decorate(&self.game, listed);
        if let Some(standard) = choices.first_mut() {
            standard.label = "Standard".to_string();
        }
        let builtin =
            BuiltinShader::ALL.map(|shader| (shader.title().to_string(), Pick::Asset(Some(shader.reference()))));
        let at = choices.len().min(1);
        choices.splice(at..at, object_picker::decorate(&self.game, builtin.to_vec()));
        let target = PickFor::MaterialShader { material: material.to_string() };
        self.picker.open(&mut self.ui, (target, anchor), "Select Shader", choices, "");
    }

    /// Draws a material with the shader picked, or the Standard one for
    /// none. Its properties were the last shader's, so they go with it. A
    /// built-in shader no material used before is compiled in.
    pub(super) fn pick_material_shader(&mut self, material: &str, pick: &Pick, renderer: &mut Renderer) {
        let shader = match pick {
            Pick::Asset(Some(path)) => match BuiltinShader::from_reference(path) {
                Some(builtin) => Some(ShaderRef::Builtin(builtin)),
                None => match self.game.content.db().guid_of(path) {
                    Some(guid) => Some(ShaderRef::Asset(guid)),
                    None => return,
                },
            },
            _ => None,
        };
        let Some(mut data) = self.material_file(material) else { return };
        if data.shader == shader {
            return;
        }
        data.shader = shader;
        data.properties.clear();
        self.save_and_show_material(material, data, renderer);
        self.game.apply_custom_shaders(renderer);
    }

    /// A shader dragged from the Project panel onto a material's Shader.
    pub(super) fn drop_material_shader(&mut self, material: &str, source: &DragSource, renderer: &mut Renderer) {
        let Some(path) = self.dragged_asset(source) else { return };
        if self.asset_kind_of(&path) != Some(AssetKind::Shader) {
            return self.notify("Only a shader goes in a material's Shader");
        }
        self.pick_material_shader(material, &Pick::Asset(Some(path)), renderer);
    }

    /// Opens the object picker under `anchor` for a picture property of a
    /// material's shader.
    pub(super) fn browse_shader_picture(&mut self, material: &str, property: &str, anchor: NodeId) {
        let choices = self.asset_choices(AssetKind::Texture);
        let target = PickFor::ShaderPicture { material: material.to_string(), property: property.to_string() };
        self.picker.open(&mut self.ui, (target, anchor), "Select Texture", choices, "");
    }

    /// Sets a picture property of a material's shader to the picture
    /// picked, or none.
    pub(super) fn pick_shader_picture(&mut self, material: &str, property: &str, pick: &Pick, renderer: &mut Renderer) {
        let picture = match pick {
            Pick::Asset(Some(path)) => match self.game.content.db().guid_of(path) {
                Some(guid) => Some(guid),
                None => return,
            },
            _ => None,
        };
        let Some(mut data) = self.material_file(material) else { return };
        match picture {
            Some(guid) => data.properties.insert(property.to_string(), PropertyValue::Picture(guid)),
            None => data.properties.remove(property),
        };
        self.save_and_show_material(material, data, renderer);
    }

    /// A picture dragged from the Project panel onto a picture property.
    pub(super) fn drop_shader_picture(
        &mut self,
        material: &str,
        property: &str,
        source: &DragSource,
        renderer: &mut Renderer,
    ) {
        let Some(path) = self.dragged_asset(source) else { return };
        if self.asset_kind_of(&path) != Some(AssetKind::Texture) {
            return self.notify("Only a picture goes in a picture property");
        }
        self.pick_shader_picture(material, property, &Pick::Asset(Some(path)), renderer);
    }

    /// Opens the object picker under `anchor` for what a new shader in
    /// `folder` starts as: a lit or unlit one to write, or a copy of one
    /// built in, to change.
    pub(super) fn browse_shader_starts(&mut self, folder: String, anchor: NodeId) {
        let choices = ShaderStart::all()
            .into_iter()
            .map(|start| Choice {
                label: start.label().to_string(),
                detail: start.detail().to_string(),
                icon: "shader",
                pick: Chosen::Shader(start),
                preview: None,
                folder: String::new(),
            })
            .collect();
        self.picker.open(&mut self.ui, (PickFor::NewShader { folder }, anchor), "New Shader", choices, "");
    }

    /// Makes a new shader in `folder` as `start` says, and selects it,
    /// ready to rename.
    pub(super) fn make_shader(&mut self, folder: &str, start: ShaderStart, renderer: &mut Renderer) {
        let namespace = super::script_namespace(&self.game.project);
        let made = crate::project_files::place(self.game.content.db().mounts(), folder).and_then(|at| {
            crate::project_files::create_shader(&at.root, &at.path, start, &namespace).map(|made| at.back(&made))
        });
        match made {
            Ok(made) => self.project_event(ProjectEvent::Changed { select: Some(made), rename: true }, renderer),
            Err(error) => self.notify(&format!("{error:#}")),
        }
    }

    /// The material asset at `material` as its file has it.
    fn material_file(&self, material: &str) -> Option<MaterialData> {
        let text = std::fs::read_to_string(self.game.content.db().file(material)).ok()?;
        toml::from_str(&text).ok()
    }

    /// Saves a material, and shows it again in the Inspector when it is
    /// shown there, as its fields may be others now.
    fn save_and_show_material(&mut self, material: &str, data: MaterialData, renderer: &mut Renderer) {
        self.asset_action(AssetAction::SaveMaterial(material.to_string(), data), renderer);
        let shown = self.inspector.shown_asset().filter(|item| item.path == material).cloned();
        if let Some(item) = shown {
            self.inspect_asset(&item);
        }
    }

    fn asset_kind_of(&self, path: &str) -> Option<AssetKind> {
        let db = self.game.content.db();
        db.guid_of(path).and_then(|guid| db.get(guid)).map(|entry| entry.kind)
    }
}
