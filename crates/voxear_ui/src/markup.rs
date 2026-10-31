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
//! Elements are `Panel`, `ScrollView`, `ListView`, `TreeView`, `Label`,
//! `Button`, `TextField` (`placeholder`, `multiline`, `value` or content),
//! `Toggle` (`value`), `Slider` (`min`, `max`, `value`, `step`) and `Foldout`
//! (`text`, `open`, children as its content), `SplitView` (`direction`,
//! `size` in pixels, negative for the second pane's, or a percentage share,
//! `min`, and two elements for its panes), `Tabs` (of `Tab`s with
//! `text`, and `selected`), `Dropdown` (of `Option`s, and `value`),
//! `DockSpace` (of `DockPanel`s with `id`, `title` and `icon`, and `layout`),
//! `Segmented` (of `Segment`s with `text` and `icon`, and `value`) and
//! `Icon` (`icon`, one of [`crate::ICONS`]), each taking `name`, `class`
//! (several, separated by spaces) and `style` (declarations that win over
//! sheets). A label's or button's text is its content, or its `text`
//! attribute; a button may instead hold elements, such as an icon and a label. A list or tree view takes `item-height`, and gets its rows
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

    /// Builds one element and what is in it.
    fn build_one(&mut self, parent: NodeId, xml: Node, builder: &mut Builder) -> Result<(), UiError> {
        self.build_nodes(parent, std::iter::once(xml), builder)
    }

    fn build_children(&mut self, parent: NodeId, xml: Node, builder: &mut Builder) -> Result<(), UiError> {
        self.build_nodes(parent, xml.children(), builder)
    }

    fn build_nodes<'a, 'input: 'a>(
        &mut self,
        parent: NodeId,
        nodes: impl Iterator<Item = Node<'a, 'input>>,
        builder: &mut Builder,
    ) -> Result<(), UiError> {
        for child in nodes {
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
                "TextField" => {
                    let content: String = child.children().filter_map(|n| n.text()).collect();
                    let text = child.attribute("value").map_or_else(|| content.trim().to_string(), str::to_string);
                    let node = self.text_field(parent, &text);
                    self.attributes(
                        node,
                        child,
                        &["name", "class", "style", "value", "placeholder", "multiline"],
                        builder,
                    );
                    if let Some(placeholder) = child.attribute("placeholder") {
                        self.set_placeholder(node, placeholder);
                    }
                    if flag(child, "multiline", builder)? {
                        self.set_multiline(node, true);
                    }
                }
                "Toggle" => {
                    let content: String = child.children().filter_map(|n| n.text()).collect();
                    let text = child.attribute("text").map_or_else(|| content.trim().to_string(), str::to_string);
                    let node = self.toggle(parent, &text, flag(child, "value", builder)?);
                    self.attributes(node, child, &["name", "class", "style", "text", "value"], builder);
                }
                "Slider" => {
                    let (min, max) = (number(child, "min", 0.0, builder)?, number(child, "max", 1.0, builder)?);
                    let (value, step) = (number(child, "value", min, builder)?, number(child, "step", 0.0, builder)?);
                    let node = self.slider(parent, min, max, min);
                    self.set_slider_step(node, step);
                    self.set_slider_value(node, value);
                    self.attributes(node, child, &["name", "class", "style", "min", "max", "value", "step"], builder);
                }
                "Foldout" => {
                    let title = child.attribute("text").unwrap_or_default();
                    let open = child.attribute("open").is_none() || flag(child, "open", builder)?;
                    let node = self.foldout(parent, title, open);
                    self.attributes(node, child, &["name", "class", "style", "text", "open"], builder);
                    let content = self.foldout_content(node).expect("a foldout has content");
                    self.build_children(content, child, builder)?;
                }
                "SplitView" => {
                    let row = child.attribute("direction") != Some("column");
                    let node = match child.attribute("size").and_then(|size| size.trim().strip_suffix('%')) {
                        Some(share) => {
                            let share = share.trim().parse::<f32>();
                            let share = share.map_err(|_| builder.error(child, "size is pixels or a percentage"))?;
                            self.split_view_fraction(parent, row, share / 100.0)
                        }
                        None => self.split_view(parent, row, number(child, "size", 240.0, builder)?),
                    };
                    self.set_split_min(node, number(child, "min", 40.0, builder)?);
                    self.attributes(node, child, &["name", "class", "style", "direction", "size", "min"], builder);
                    let panes = self.split_panes(node).expect("a split view has panes");
                    let elements: Vec<Node> = child.children().filter(Node::is_element).collect();
                    let [first, second] = elements[..] else {
                        return Err(
                            builder.error(child, format!("a SplitView holds two elements, not {}", elements.len()))
                        );
                    };
                    self.build_one(panes.0, first, builder)?;
                    self.build_one(panes.1, second, builder)?;
                }
                "Tabs" => {
                    let node = self.tabs(parent);
                    self.attributes(node, child, &["name", "class", "style", "selected"], builder);
                    for tab in child.children().filter(Node::is_element) {
                        if tab.tag_name().name() != "Tab" {
                            return Err(builder.error(tab, "Tabs hold Tab elements"));
                        }
                        let page = self.add_tab(node, tab.attribute("text").unwrap_or_default());
                        self.attributes(page, tab, &["name", "class", "style", "text"], builder);
                        self.build_children(page, tab, builder)?;
                    }
                    self.set_selected_tab(node, number(child, "selected", 0.0, builder)? as usize);
                }
                "Dropdown" => {
                    let options: Vec<String> = child
                        .children()
                        .filter(|n| n.is_element() && n.tag_name().name() == "Option")
                        .map(|n| n.children().filter_map(|t| t.text()).collect::<String>().trim().to_string())
                        .collect();
                    let options: Vec<&str> = options.iter().map(String::as_str).collect();
                    let node = self.dropdown(parent, &options, number(child, "value", 0.0, builder)? as usize);
                    self.attributes(node, child, &["name", "class", "style", "value"], builder);
                }
                "DockSpace" => {
                    let node = self.dock_space(parent);
                    self.attributes(node, child, &["name", "class", "style", "layout"], builder);
                    for panel in child.children().filter(Node::is_element) {
                        if panel.tag_name().name() != "DockPanel" {
                            return Err(builder.error(panel, "a DockSpace holds DockPanel elements"));
                        }
                        let id =
                            panel.attribute("id").ok_or_else(|| builder.error(panel, "a DockPanel needs an id"))?;
                        let content = self.add_dock_panel(node, id, panel.attribute("title").unwrap_or(id));
                        if let Some(icon) = panel.attribute("icon") {
                            self.set_dock_panel_icon(node, id, icon);
                        }
                        self.attributes(content, panel, &["name", "class", "style", "id", "title", "icon"], builder);
                        self.build_children(content, panel, builder)?;
                    }
                    if let Some(layout) = child.attribute("layout") {
                        self.set_dock_layout(node, layout).map_err(|e| builder.error(child, e))?;
                    }
                }
                "Icon" => {
                    let icon = child.attribute("icon").ok_or_else(|| builder.error(child, "<Icon> needs icon"))?;
                    if crate::icons::parts(icon).is_none() {
                        builder.warnings.push(format!("{}: there is no icon '{icon}'", builder.place(child)));
                    }
                    let node = self.icon(parent, icon);
                    self.attributes(node, child, &["name", "class", "style", "icon"], builder);
                }
                "Segmented" => {
                    let node = self.segmented(parent);
                    self.attributes(node, child, &["name", "class", "style", "value"], builder);
                    for segment in child.children().filter(Node::is_element) {
                        if segment.tag_name().name() != "Segment" {
                            return Err(builder.error(segment, "Segmented holds Segment elements"));
                        }
                        let text = segment.attribute("text").unwrap_or_default();
                        let made = self.add_segment(node, text, segment.attribute("icon"));
                        self.attributes(made, segment, &["name", "class", "style", "text", "icon"], builder);
                    }
                    self.set_segment(node, number(child, "value", 0.0, builder)? as usize);
                }
                "Button" if child.children().any(|n| n.is_element()) => {
                    let element = Element::new(Kind::Button, Style::row());
                    let node = self.add(parent, element);
                    self.attributes(node, child, &["name", "class", "style"], builder);
                    self.build_children(node, child, builder)?;
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
                    let known = "Panel, ScrollView, ListView, TreeView, SplitView, Tabs, DockSpace, Label, Button, \
                                 Icon, TextField, Toggle, Slider, Foldout, Dropdown, Segmented or Style";
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

/// A `true` or `false` attribute; absent is false.
fn flag(xml: Node, name: &str, builder: &Builder) -> Result<bool, UiError> {
    match xml.attribute(name) {
        None | Some("false") => Ok(false),
        Some("true") => Ok(true),
        Some(other) => Err(builder.error(xml, format!("{name} is true or false, not '{other}'"))),
    }
}

/// A number attribute, or `default` when absent.
fn number(xml: Node, name: &str, default: f32, builder: &Builder) -> Result<f32, UiError> {
    match xml.attribute(name) {
        None => Ok(default),
        Some(text) => {
            let text = text.trim().trim_end_matches("px");
            text.parse().map_err(|_| builder.error(xml, format!("{name} is a number, not '{text}'")))
        }
    }
}
