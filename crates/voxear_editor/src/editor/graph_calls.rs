//! The calls a tool reaches graphs' comments by, `graph.comments` and
//! `graph.comment.add`, `.set` and `.remove`, for a generator's graph or a
//! VoxShade shader's; and `block_world.write_land`, which writes a Block
//! World's laid-in land into its generator. A graph open in its panel is
//! changed there, as a step to undo, and saved as the panel saves; one that
//! is not is read from its file and written again.

use serde_json::{Value as Json, json};
use voxear_assets::AssetKind;
use voxear_engine::{
    blocks::BlockWorldAsset,
    local_link::{ErrorKind, LinkError},
};
use voxear_render::Renderer;
use voxear_terrain::generate::{Comment, NodeId as GraphNode, WorldGen};

use super::{
    Editor,
    link_calls::{Called, done},
};
use crate::{
    graph_canvas::NEW_COMMENT_SIZE,
    history::ObjState,
    link::{self, bad, need, need_text, opt_pair, opt_text, opt_wholes, refuse},
};

/// A graph's file, by its path: a generator's or a VoxShade shader's.
enum GraphFile {
    WorldGen(String),
    VoxShade(String),
}

fn comment_json(comment: &Comment) -> Json {
    json!({ "id": comment.id, "at": comment.at, "size": comment.size, "title": comment.title, "text": comment.text })
}

/// A VoxShade shader's comments, as a generator's are written: both are
/// the same four things.
fn from_shade(comments: &[voxear_voxshade::Comment]) -> Vec<Comment> {
    let one = |c: &voxear_voxshade::Comment| Comment {
        id: c.id,
        at: c.at,
        size: c.size,
        title: c.title.clone(),
        text: c.text.clone(),
    };
    comments.iter().map(one).collect()
}

fn to_shade(comments: &[Comment]) -> Vec<voxear_voxshade::Comment> {
    let one = |c: &Comment| voxear_voxshade::Comment {
        id: c.id,
        at: c.at,
        size: c.size,
        title: c.title.clone(),
        text: c.text.clone(),
    };
    comments.iter().map(one).collect()
}

/// A size given for a comment: larger than nothing.
fn size_arg(args: &Json) -> Result<Option<[f32; 2]>, LinkError> {
    match opt_pair(args, "size")? {
        Some(size) if size[0] <= 0.0 || size[1] <= 0.0 => Err(bad("`size` is two numbers larger than nothing")),
        size => Ok(size),
    }
}

impl Editor {
    pub(super) fn graph_comments(&mut self, args: &Json) -> Called {
        let file = self.graph_file(args)?;
        let comments = self.comments_of(&file)?;
        done(json!({ "comments": comments.iter().map(comment_json).collect::<Vec<_>>() }))
    }

    pub(super) fn graph_comment(&mut self, call: &str, args: &Json, renderer: &mut Renderer) -> Called {
        let file = self.graph_file(args)?;
        let mut comments = self.comments_of(&file)?;
        let numbered = |comments: &[Comment]| -> Result<usize, LinkError> {
            let id = need(args, "comment")?.as_u64().ok_or_else(|| bad("`comment` is a whole number"))?;
            let found = comments.iter().position(|comment| u64::from(comment.id) == id);
            found.ok_or_else(|| refuse(ErrorKind::Failed, format!("the graph has no comment {id}")))
        };
        let answer = match call {
            "graph.comment.add" => {
                let title = need_text(args, "title")?.to_string();
                let text = opt_text(args, "text").unwrap_or_default().to_string();
                let round = match opt_wholes(args, "around")? {
                    Some(nodes) => {
                        let nodes: Vec<GraphNode> = nodes.into_iter().map(GraphNode).collect();
                        let round = self.comment_round(&file, &nodes)?;
                        Some(round.ok_or_else(|| bad("none of `around`'s nodes is in the graph"))?)
                    }
                    None => None,
                };
                let at = opt_pair(args, "at")?.or(round.map(|(at, _)| at)).unwrap_or_default();
                let size = size_arg(args)?.or(round.map(|(_, size)| size)).unwrap_or(NEW_COMMENT_SIZE);
                let id = comments.iter().map(|comment| comment.id).max().unwrap_or(0) + 1;
                comments.push(Comment { id, at, size, title, text });
                json!({ "comment": id })
            }
            "graph.comment.set" => {
                let place = numbered(&comments)?;
                let before = comments[place].clone();
                let comment = &mut comments[place];
                if let Some(title) = opt_text(args, "title") {
                    comment.title = title.to_string();
                }
                if let Some(text) = opt_text(args, "text") {
                    comment.text = text.to_string();
                }
                comment.at = opt_pair(args, "at")?.unwrap_or(comment.at);
                comment.size = size_arg(args)?.unwrap_or(comment.size);
                json!({ "changed": *comment != before })
            }
            _ => {
                let place = numbered(&comments)?;
                comments.remove(place);
                json!({ "removed": true })
            }
        };
        self.set_comments(&file, comments, renderer)?;
        done(answer)
    }

    pub(super) fn block_world_write_land(&mut self, args: &Json, renderer: &mut Renderer) -> Called {
        let entity = link::resolve_obj(&self.game, need(args, "obj")?)?;
        if self.game.engine.world.get::<BlockWorldAsset>(entity).is_none() {
            return Err(refuse(ErrorKind::NoSuchComponent, "the Obj has no Block World"));
        }
        let before = ObjState::of(&self.game.engine.world, entity)
            .ok_or_else(|| refuse(ErrorKind::NoSuchObj, "the Obj is gone"))?;
        let written = crate::block_props::write_land(&mut self.game, Some(renderer), entity)
            .map_err(|error| refuse(ErrorKind::Failed, format!("the world's land was not written: {error}")))?;
        self.record_change(entity, before);
        done(json!({ "generator": written.path, "nodes": written.nodes, "comments": written.comments }))
    }

    /// The graph a call's `asset` names.
    fn graph_file(&self, args: &Json) -> Result<GraphFile, LinkError> {
        let (_, path, kind) = link::resolve_project_asset(&self.game, need(args, "asset")?)?;
        match kind {
            AssetKind::WorldGen => Ok(GraphFile::WorldGen(path)),
            AssetKind::VoxShade => Ok(GraphFile::VoxShade(path)),
            _ => Err(bad(format!("{path} is not a graph: give a .worldgen or a .voxshade"))),
        }
    }

    fn read_world_gen(&self, path: &str) -> Result<WorldGen, LinkError> {
        let text = std::fs::read_to_string(self.game.content.db().file(path));
        let read = text.map_err(anyhow::Error::from).and_then(|text| WorldGen::parse(&text));
        read.map_err(|error| refuse(ErrorKind::Failed, format!("could not read {path}: {error:#}")))
    }

    fn read_voxshade(&self, path: &str) -> Result<voxear_voxshade::Graph, LinkError> {
        let text = std::fs::read_to_string(self.game.content.db().file(path));
        let read = text.map_err(anyhow::Error::from).and_then(|text| voxear_voxshade::Graph::from_toml(&text));
        read.map_err(|error| refuse(ErrorKind::Failed, format!("could not read {path}: {error:#}")))
    }

    /// A graph's comments: as its panel has them where it is open there.
    fn comments_of(&self, file: &GraphFile) -> Result<Vec<Comment>, LinkError> {
        match file {
            GraphFile::WorldGen(path) => match self.generator.world().filter(|_| self.generator.path() == Some(path)) {
                Some(open) => Ok(open.graph().comments.clone()),
                None => Ok(self.read_world_gen(path)?.graph().comments.clone()),
            },
            GraphFile::VoxShade(path) => match self.voxshade.graph().filter(|_| self.voxshade.path() == Some(path)) {
                Some(open) => Ok(from_shade(&open.comments)),
                None => Ok(from_shade(&self.read_voxshade(path)?.comments)),
            },
        }
    }

    /// Where a comment round these nodes of a graph goes, and how large it is.
    fn comment_round(&self, file: &GraphFile, nodes: &[GraphNode]) -> Result<Option<([f32; 2], [f32; 2])>, LinkError> {
        match file {
            GraphFile::WorldGen(path) => {
                let read;
                let world = match self.generator.world().filter(|_| self.generator.path() == Some(path)) {
                    Some(open) => open,
                    None => {
                        read = self.read_world_gen(path)?;
                        &read
                    }
                };
                Ok(crate::generator_panel::comment_round(world.graph(), nodes))
            }
            GraphFile::VoxShade(path) => {
                let read;
                let graph = match self.voxshade.graph().filter(|_| self.voxshade.path() == Some(path)) {
                    Some(open) => open,
                    None => {
                        read = self.read_voxshade(path)?;
                        &read
                    }
                };
                let nodes: Vec<voxear_voxshade::NodeId> =
                    nodes.iter().map(|node| voxear_voxshade::NodeId(node.0)).collect();
                Ok(crate::voxshade_panel::comment_round(graph, &nodes))
            }
        }
    }

    /// Gives a graph these comments: in its panel, as a step to undo, where it is open there; else in its file.
    fn set_comments(
        &mut self,
        file: &GraphFile,
        comments: Vec<Comment>,
        renderer: &mut Renderer,
    ) -> Result<(), LinkError> {
        match file {
            GraphFile::WorldGen(path) => {
                if self.generator.path() == Some(path.as_str())
                    && let Some(open) = self.generator.world()
                {
                    let world = open.with_comments(comments);
                    self.generator.change(world, None);
                    return Ok(());
                }
                let world = self.read_world_gen(path)?.with_comments(comments);
                self.save_world_gen(path, &world);
            }
            GraphFile::VoxShade(path) => {
                if self.voxshade.path() == Some(path.as_str())
                    && let Some(open) = self.voxshade.graph()
                {
                    let mut graph = open.clone();
                    graph.comments = to_shade(&comments);
                    self.voxshade.change(graph);
                    return Ok(());
                }
                let mut graph = self.read_voxshade(path)?;
                graph.comments = to_shade(&comments);
                self.save_voxshade(path, &graph, renderer);
            }
        }
        Ok(())
    }
}
