//! Previews in the editor: the Project panel's tiles and rows, the object
//! picker's rows, the thumbnails in the Inspector's asset fields, and the
//! Inspector's large preview of the asset selected in the Project, which a
//! drag turns. Assets' previews come from [`AssetPreviews`], a few new ones
//! a frame, so a folder of models opens without a hitch. A Mesh Renderer's
//! Mesh and Material fields show its own mesh and material, which need not
//! be assets.

use std::collections::HashMap;

use voxear_core::math::{Mat4, Quat, Vec2};
use voxear_ecs::Entity;
use voxear_engine::preview_parts;
use voxear_render::{MeshRenderer, PreviewId, PreviewImage, PreviewOptions, PreviewPart, Primitive, Renderer};
use voxear_ui::ImageRef;

use super::Editor;
use crate::{
    object_picker,
    props::{self, Prop, Value},
};

/// New asset previews started in one frame at most; the rest wait for the
/// next frames.
const NEW_PER_FRAME: usize = 4;
/// How big the Inspector's previews show, as Editor.vss sizes
/// `field-thumb` and `asset-preview-image`.
const FIELD_PREVIEW: f32 = 18.0;
const INSPECTOR_PREVIEW: f32 = 200.0;
/// How far a drag turns the Inspector's preview, in radians per pixel.
const TURN_PER_PIXEL: f32 = 0.01;
/// Frames a field's preview is kept after it last showed.
const FIELD_KEPT: u64 = 300;

/// The Inspector's large preview: of which asset, its preview, and how far
/// drags have turned it (around the vertical, then across).
pub(super) struct InspectorPreview {
    path: String,
    id: PreviewId,
    size: u32,
    turn: Vec2,
    /// Where the pointer was last frame, while a drag turns it.
    dragging: Option<(f32, f32)>,
}

impl InspectorPreview {
    fn options(&self) -> PreviewOptions {
        let rotation = Quat::from_rotation_x(self.turn.y)
            * Quat::from_rotation_y(self.turn.x)
            * PreviewOptions::default().rotation;
        PreviewOptions { size: self.size, rotation, ..PreviewOptions::default() }
    }
}

/// Previews of a Mesh Renderer's own mesh and material, for its fields, by
/// the mesh, material and size they show, with the frame each last showed.
#[derive(Default)]
pub(super) struct FieldPreviews {
    made: HashMap<(u32, u32, u32), (PreviewId, u64)>,
    frame: u64,
}

impl FieldPreviews {
    fn image(&mut self, renderer: &mut Renderer, part: PreviewPart, size: u32) -> Option<PreviewImage> {
        let key = (part.mesh.0, part.material.0, size);
        let frame = self.frame;
        let id = match self.made.get_mut(&key) {
            Some((id, seen)) => {
                *seen = frame;
                *id
            }
            None => {
                let id = renderer.add_preview(vec![part], PreviewOptions { size, ..PreviewOptions::default() });
                self.made.insert(key, (id, frame));
                id
            }
        };
        renderer.preview(id)
    }

    /// Lets go of previews not shown for a while, and draws the rest again,
    /// when `redraw`, as after a material changed.
    fn sweep(&mut self, renderer: &mut Renderer, redraw: bool) {
        self.frame += 1;
        let frame = self.frame;
        self.made.retain(|_, (id, seen)| {
            let kept = *seen + FIELD_KEPT > frame;
            if !kept {
                renderer.free_preview(*id);
            }
            kept
        });
        if redraw {
            for (&(_, _, size), (id, _)) in &self.made {
                renderer.set_preview(*id, None, PreviewOptions { size, ..PreviewOptions::default() });
            }
        }
    }
}

impl Editor {
    /// Fills in every preview shown this frame.
    pub(super) fn update_previews(&mut self, renderer: &mut Renderer) {
        let mut budget = NEW_PER_FRAME;
        let size = |logical: f32, scale: f32| (logical * scale).ceil() as u32;

        let tile = size(self.project.preview_size(), self.scale);
        let shown = self.asset_images(renderer, self.project.previewable(&self.ui), tile, &mut budget);
        self.project.set_previews(&mut self.ui, shown);

        if self.picker.is_open() {
            let row = size(object_picker::PREVIEW, self.scale);
            let shown = self.asset_images(renderer, self.picker.previewable(), row, &mut budget);
            self.picker.set_previews(&mut self.ui, shown);
        }

        let thumb = size(FIELD_PREVIEW, self.scale);
        let redraw = std::mem::take(&mut self.redraw_field_previews);
        self.field_previews.sweep(renderer, redraw);
        if let Some(entity) = self.selected() {
            for (prop, node) in self.inspector.field_thumbs(&self.ui) {
                let image = self.field_image(renderer, entity, &prop, thumb, &mut budget);
                self.ui.set_image(node, image);
                self.ui.set_visible(node, image.is_some());
            }
            // [PreviewField]'s larger images, under their fields.
            for (prop, node, logical) in self.inspector.field_previews().to_vec() {
                let image = self.field_image(renderer, entity, &prop, size(logical as f32, self.scale), &mut budget);
                self.ui.set_image(node, image);
            }
        }

        self.update_inspector_preview(renderer);
    }

    /// The previews of these assets that are drawn, by path.
    fn asset_images(
        &mut self,
        renderer: &mut Renderer,
        references: Vec<String>,
        size: u32,
        budget: &mut usize,
    ) -> HashMap<String, ImageRef> {
        references
            .into_iter()
            .filter_map(|reference| {
                let image = self.asset_image(renderer, &reference, size, budget)?;
                Some((reference, image))
            })
            .collect()
    }

    fn asset_image(
        &mut self,
        renderer: &mut Renderer,
        reference: &str,
        size: u32,
        budget: &mut usize,
    ) -> Option<ImageRef> {
        if !self.asset_previews.has(reference, size) {
            *budget = budget.checked_sub(1)?;
        }
        let image = self.asset_previews.image(&mut self.game.content, renderer, reference, size)?;
        Some(ImageRef { texture: image.texture.0, uv: image.uv })
    }

    /// What an asset field's thumbnail shows: a Mesh Renderer's own mesh in
    /// its material, or its material on a sphere, and for a script's field,
    /// the asset it points at.
    fn field_image(
        &mut self,
        renderer: &mut Renderer,
        entity: Entity,
        prop: &Prop,
        size: u32,
        budget: &mut usize,
    ) -> Option<ImageRef> {
        let drawn = self.game.engine.world.get::<MeshRenderer>(entity).copied();
        let part = match (prop, drawn) {
            (Prop::Mesh, Some(drawn)) => {
                PreviewPart { mesh: drawn.mesh, material: drawn.material, transform: Mat4::IDENTITY }
            }
            (Prop::Material, Some(drawn)) => {
                let sphere = renderer.primitive(Primitive::Sphere).ok()?;
                PreviewPart { mesh: sphere, material: drawn.material, transform: Mat4::IDENTITY }
            }
            (Prop::Field { .. }, _) => {
                let Some(Value::Asset(path)) = props::get(&self.game, self.inspector.types(), entity, prop) else {
                    return None;
                };
                return self.asset_image(renderer, &path, size, budget);
            }
            _ => return None,
        };
        let image = self.field_previews.image(renderer, part, size)?;
        Some(ImageRef { texture: image.texture.0, uv: image.uv })
    }

    /// The Inspector's large preview of the asset selected in the Project:
    /// made when one shows, turned by drags on it, and let go with it.
    fn update_inspector_preview(&mut self, renderer: &mut Renderer) {
        let Some((path, frame, image)) = self.inspector.asset_preview() else {
            if let Some(shown) = self.inspector_preview.take() {
                renderer.free_preview(shown.id);
            }
            return;
        };
        if self.inspector_preview.as_ref().is_none_or(|shown| shown.path != path) {
            if let Some(shown) = self.inspector_preview.take() {
                renderer.free_preview(shown.id);
            }
            let Ok(parts) = preview_parts(&mut self.game.content, renderer, &path) else { return };
            let size = (INSPECTOR_PREVIEW * self.scale).ceil() as u32;
            let mut shown = InspectorPreview { path, id: PreviewId(0), size, turn: Vec2::ZERO, dragging: None };
            shown.id = renderer.add_preview(parts, shown.options());
            self.inspector_preview = Some(shown);
        }
        let shown = self.inspector_preview.as_mut().expect("made above");
        let pointer = self.ui.pointer_position();
        match (self.ui.is_pressed(frame), shown.dragging, pointer) {
            (true, Some(last), Some(now)) => {
                shown.turn += Vec2::new(now.0 - last.0, now.1 - last.1) * TURN_PER_PIXEL;
                shown.turn.y = shown.turn.y.clamp(-1.4, 1.4);
                shown.dragging = Some(now);
                renderer.set_preview(shown.id, None, shown.options());
            }
            (true, None, now) => shown.dragging = now,
            _ => shown.dragging = None,
        }
        let drawn = renderer.preview(shown.id).map(|image| ImageRef { texture: image.texture.0, uv: image.uv });
        self.ui.set_image(image, drawn);
        self.ui.set_visible(image, drawn.is_some());
    }

    /// How far drags have turned the Inspector's preview: around the
    /// vertical, then across, in radians.
    pub fn inspector_preview_turn(&self) -> Option<Vec2> {
        self.inspector_preview.as_ref().map(|shown| shown.turn)
    }

    /// After an asset changed: its previews are drawn again.
    pub(super) fn asset_changed(&mut self, renderer: &mut Renderer, path: &str) {
        self.asset_previews.refresh(&mut self.game.content, renderer, path);
        if let Some(shown) = self.inspector_preview.as_ref().filter(|shown| shown.path == path) {
            renderer.set_preview(shown.id, None, shown.options());
        }
        self.redraw_field_previews = true;
    }
}
