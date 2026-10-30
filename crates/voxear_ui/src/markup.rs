//! Markup (`.vui`): the elements of a piece of UI, as XML, in the spirit of
//! Unity's UXML.
//!
//! ```xml
//! <vui>
//!     <Style src="Stats.vss" />
//!     <Panel name="stats" class="panel">
//!         <Label class="title">Stats</Label>
//!         <Button name="close" text="Close" />
//!     </Panel>
//! </vui>
//! ```
//!
//! Elements are `Panel`, `ScrollView`, `ListView`, `TreeView`, `Label` and
//! `Button`, each taking `name`, `class`
//! (several, separated by spaces) and `style` (declarations that win over
//! sheets). A label's or button's text is its content, or its `text`
//! attribute. A list or tree view takes `item-height`, and gets its rows
//! from code with [`Ui::bind_list`] or [`Ui::bind_tree`]. `<Style src>`
//! attaches a sheet to the element it is in, so it styles that element and
//! everything in it.

use roxmltree::{Document, Node};

use crate::{
    NodeId, Style, Ui, UiError,
    sheet::{StyleSheet, parse_inline},
    tree::{Element, Kind, Text},
};

/// Reads a file a document refers to, by the path written in it.
pub type LoadFile<'a> = dyn FnMut(&str) -> Result<String, String> + 'a;

struct Builder<'a, 'd> {
    source: &'a str,
    document: &'d Document<'d>,
    load: &'a mut LoadFile<'a>,
    warnings: Vec<String>,
}

impl Builder<'_, '_> {
    fn place(&self, node: Node) -> String {
        let at = self.document.text_pos_at(node.range().start);
        format!("{}:{}:{}", self.source, at.row, at.col)
    }

    fn error(&self, node: Node, message: impl std::fmt::Display) -> UiError {
        UiError(format!("{}: {message}", self.place(node)))
    }
}

impl Ui {
    /// Builds what a `.vui` document describes inside `parent`, in a
    /// container that covers it, and returns the container. `source` names
    /// the document in messages; `load` reads the files it refers to, such
    /// as its style sheets. Nothing is added when it fails.
    pub fn load_markup(
        &mut self,
        parent: NodeId,
        source: &str,
        markup: &str,
        load: &mut LoadFile,
    ) -> Result<NodeId, UiError> {
        let document = Document::parse(markup).map_err(|e| UiError(format!("{source}: {e}")))?;
        let root = document.root_element();
        let mut builder = Builder { source, document: &document, load, warnings: Vec::new() };
        if root.tag_name().name() != "vui" {
            return Err(builder.error(root, format!("a document starts with <vui>, not <{}>", root.tag_name().name())));
        }
        let container = self.add(parent, Element::new(Kind::Panel, Style::cover()));
        if let Err(error) = self.build_children(container, root, &mut builder) {
            self.remove(container);
            return Err(error);
        }
        self.warn(builder.warnings);
        Ok(container)
    }

    fn build_children(&mut self, parent: NodeId, xml: Node, builder: &mut Builder) -> Result<(), UiError> {
        for child in xml.children() {
            if child.is_text() {
                if child.text().is_some_and(|t| !t.trim().is_empty()) {
                    builder
                        .warnings
                        .push(format!("{}: text in a Panel is not shown; put it in a Label", builder.place(child)));
                }
                continue;
            }
            if !child.is_element() {
                continue;
            }
            match child.tag_name().name() {
                "Style" => {
                    let src = child.attribute("src").ok_or_else(|| builder.error(child, "<Style> needs src"))?;
                    let text =
                        (builder.load)(src).map_err(|e| builder.error(child, format!("could not read {src}: {e}")))?;
                    self.add_sheet(parent, StyleSheet::parse(src, &text));
                }
                "Panel" => {
                    let node = self.add(parent, Element::new(Kind::Panel, Style::column()));
                    self.attributes(node, child, &["name", "class", "style"], builder);
                    self.build_children(node, child, builder)?;
                }
                "ScrollView" => {
                    let node = self.add(parent, Element::new(Kind::ScrollView, Style::column().scroll()));
                    self.attributes(node, child, &["name", "class", "style"], builder);
                    self.build_children(node, child, builder)?;
                }
                tag @ ("ListView" | "TreeView") => {
                    if let Some(inner) = child.children().find(Node::is_element) {
                        return Err(builder
                            .error(inner, format!("a {tag}'s rows come from code, through bind_list or bind_tree")));
                    }
                    let kind = if tag == "ListView" { Kind::ListView } else { Kind::TreeView };
                    let node = self.add(parent, Element::new(kind, Style::column().scroll()));
                    self.attributes(node, child, &["name", "class", "style", "item-height"], builder);
                    if let Some(height) = child.attribute("item-height") {
                        let height = height.trim_end_matches("px").parse::<f32>();
                        let height = height.map_err(|_| builder.error(child, "item-height is a number of pixels"))?;
                        self.set_item_height(node, height);
                    }
                }
                tag @ ("Label" | "Button") => {
                    if let Some(inner) = child.children().find(Node::is_element) {
                        return Err(builder.error(inner, format!("a {tag} holds only text")));
                    }
                    let content: String = child.children().filter_map(|n| n.text()).collect();
                    let text = child.attribute("text").map_or_else(|| content.trim().to_string(), str::to_string);
                    let kind = if tag == "Label" { Kind::Label } else { Kind::Button };
                    let element = Element { text: Some(Text::new(&text)), ..Element::new(kind, Style::default()) };
                    let node = self.add(parent, element);
                    self.attributes(node, child, &["name", "class", "style", "text"], builder);
                }
                other => {
                    let known = "Panel, ScrollView, ListView, TreeView, Label, Button or Style";
                    return Err(builder.error(child, format!("unknown element <{other}>; expected {known}")));
                }
            }
        }
        Ok(())
    }

    fn attributes(&mut self, node: NodeId, xml: Node, known: &[&str], builder: &mut Builder) {
        for attribute in xml.attributes() {
            match attribute.name() {
                "name" => self.set_name(node, attribute.value()),
                "class" => attribute.value().split_whitespace().for_each(|class| self.add_class(node, class)),
                "style" => {
                    let place = builder.place(xml);
                    let inline = parse_inline(&place, attribute.value(), &mut builder.warnings);
                    if let Some(element) = self.element_mut(node) {
                        element.inline = inline;
                    }
                }
                "text" => {}
                other if !known.contains(&other) => {
                    builder.warnings.push(format!(
                        "{}: <{}> has no attribute '{other}'",
                        builder.place(xml),
                        xml.tag_name().name()
                    ));
                }
                _ => {}
            }
        }
    }
}
