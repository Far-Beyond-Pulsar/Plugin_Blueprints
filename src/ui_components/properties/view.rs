//! Read-only snapshots of canvas content for the inspector. The document is the
//! Graphy graph; these carry what the inspector shows (presentation title,
//! catalogue description, laid-out size) without being a second model.

use std::collections::HashMap;

use crate::core::definitions::NodeDefinitions;
use crate::core::types::{NodeType, PinDataType};
use crate::editor::graph_canvas_panel::GraphCanvasPanel;
use gpui::{App, Point, Size};

#[derive(Clone, Debug)]
pub struct PinView {
    pub id: String,
    pub name: String,
    pub data_type: PinDataType,
}

#[derive(Clone, Debug)]
pub struct NodeView {
    pub id: String,
    pub definition_id: String,
    pub title: String,
    pub icon: String,
    pub node_type: NodeType,
    /// Catalogue category name (`Logic`, `Math`, ...), empty if uncategorised.
    pub category: String,
    pub is_event: bool,
    pub position: Point<f32>,
    pub size: Size<f32>,
    pub inputs: Vec<PinView>,
    pub outputs: Vec<PinView>,
    pub properties: HashMap<String, String>,
    pub description: String,
}

#[derive(Clone, Debug)]
pub struct CommentView {
    pub index: usize,
    pub id: String,
    pub text: String,
    pub position: Point<f32>,
    pub size: Size<f32>,
    pub color: gpui::Hsla,
    pub contained_node_ids: Vec<String>,
}

fn pin_view(pin: &graphy::PinInstance) -> PinView {
    PinView {
        id: pin.id.clone(),
        name: pin.pin.name.clone(),
        data_type: match pin.data_type() {
            graphy::DataType::Exec => PinDataType::execution(),
            graphy::DataType::Data(info) => PinDataType::from_type_str(info.type_string.clone()),
        },
    }
}

pub fn hsla_from_rgba(color: [f32; 4]) -> gpui::Hsla {
    gpui::Hsla::from(gpui::Rgba {
        r: color[0],
        g: color[1],
        b: color[2],
        a: color[3],
    })
}

pub fn rgba_from_hsla(color: gpui::Hsla) -> [f32; 4] {
    let rgba = gpui::Rgba::from(color);
    [rgba.r, rgba.g, rgba.b, rgba.a]
}

impl GraphCanvasPanel {
    /// The inspector's view of node `id`.
    pub fn node_view(&self, id: &str, cx: &App) -> Option<NodeView> {
        let node = self.graph(cx).nodes.get(id)?;
        let canvas = self.canvas.read(cx);
        let scene_node = canvas.scene().node(id);
        let definitions = NodeDefinitions::load();
        let definition = definitions.get_node_definition(&node.node_type);
        let category = definitions
            .get_category_for_node(&node.node_type)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let presentation = scene_node.map(|n| &n.presentation);
        let size = scene_node.map_or(Size::new(0.0, 0.0), |n| {
            Size::new(n.layout.size.x, n.layout.size.y)
        });
        Some(NodeView {
            id: node.id.clone(),
            definition_id: node.node_type.clone(),
            title: presentation.map_or_else(|| node.node_type.clone(), |p| p.title.clone()),
            icon: presentation
                .and_then(|p| p.icon)
                .map(String::from)
                .unwrap_or_default(),
            node_type: NodeType::of(&node.node_type),
            category,
            is_event: definition.is_some_and(|d| d.is_event)
                || crate::editor::panel::BlueprintEditorPanel::node_type_is_event(&node.node_type),
            position: Point::new(node.position.x as f32, node.position.y as f32),
            size,
            inputs: node.inputs.iter().map(pin_view).collect(),
            outputs: node.outputs.iter().map(pin_view).collect(),
            properties: node
                .properties
                .iter()
                .map(|(k, v)| (k.clone(), crate::io::canvas_doc::property_string(v)))
                .collect(),
            description: definition.map(|d| d.description.clone()).unwrap_or_default(),
        })
    }

    /// The inspector's view of the comment at `index`.
    pub fn comment_view(&self, index: usize, cx: &App) -> Option<CommentView> {
        let comment = self.graph(cx).comments.get(index)?;
        Some(CommentView {
            index,
            id: comment.id.clone(),
            text: comment.text.clone(),
            position: Point::new(comment.position.x as f32, comment.position.y as f32),
            size: Size::new(comment.size.0 as f32, comment.size.1 as f32),
            color: hsla_from_rgba(comment.color),
            contained_node_ids: comment.contained_node_ids.clone(),
        })
    }

    /// Whether the pin has a wire on it.
    pub fn is_pin_connected(&self, node: &str, pin: &str, input: bool, cx: &App) -> bool {
        self.graph(cx).connections.iter().any(|c| {
            if input {
                c.target_node == node && c.target_pin == pin
            } else {
                c.source_node == node && c.source_pin == pin
            }
        })
    }

    /// The colour picker for the comment with this id, created on first use and
    /// bound to the comment's colour.
    pub fn comment_picker(
        &mut self,
        index: usize,
        id: &str,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Entity<ui::color_picker::ColorPickerState> {
        use gpui::AppContext as _;
        if let Some(picker) = self.comment_pickers.get(id) {
            return picker.clone();
        }
        let picker = cx.new(|cx| ui::color_picker::ColorPickerState::new(window, cx));
        let comment_id = id.to_string();
        let sub = cx.subscribe_in(
            &picker,
            window,
            move |this: &mut GraphCanvasPanel,
                  _picker,
                  event: &ui::color_picker::ColorPickerEvent,
                  _window,
                  cx| {
                if let ui::color_picker::ColorPickerEvent::Change(Some(color)) = event {
                    let index = this
                        .graph(cx)
                        .comments
                        .iter()
                        .position(|c| c.id == comment_id)
                        .unwrap_or(index);
                    this.set_comment_color(index, rgba_from_hsla(*color), cx);
                }
            },
        );
        self.subscriptions.push(sub);
        self.comment_pickers.insert(id.to_string(), picker.clone());
        picker
    }
}

impl NodeView {
    /// The badge text: `Event Node`, `Math Node`, `Reroute Node`...
    pub fn kind_label(&self) -> String {
        let kind = if self.is_event {
            "Event".to_string()
        } else if !self.category.is_empty() {
            self.category.clone()
        } else {
            format!("{:?}", self.node_type)
        };
        format!("{kind} Node")
    }
}
