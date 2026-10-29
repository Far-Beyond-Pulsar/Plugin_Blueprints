//! Authored-graph snapshots of the open tabs, for saving, compiling,
//! validating and AI tools. The canvases own the editing documents; these
//! helpers only convert at the boundary.

use graphy_blueprint::GraphDescription;

use crate::core::subgraph_ref::SubGraphReference;
use crate::editor::panel::BlueprintEditorPanel;
use gpui::App;

/// Subgraph call nodes carry the compiler's historical node type in saved and
/// compiled graphs.
pub(crate) fn encode_subgraph_calls(
    graph: &mut GraphDescription,
    subgraphs: &[graphy_blueprint::SubGraph],
) {
    for node in graph.nodes.values_mut() {
        if let Some(reference) = SubGraphReference::decode(&node.node_type, subgraphs) {
            node.node_type = reference.encode_for_compiler();
        }
    }
}

impl BlueprintEditorPanel {
    /// The authored graph of tab `tab_id`, from its live canvas or, before the
    /// canvas exists, from the tab's document.
    pub(crate) fn tab_authored_graph(&self, tab_id: &str, cx: &App) -> Option<GraphDescription> {
        let mut graph = match self.graph_panels.iter().find(|(id, _)| id == tab_id) {
            Some((_, canvas)) => canvas.read(cx).authored_graph(cx),
            None => {
                let doc = self
                    .open_tabs
                    .iter()
                    .find(|tab| tab.id == tab_id)?
                    .pending
                    .as_ref()?;
                crate::io::canvas_doc::to_authored(&doc.graph, &doc.sidecar)
            }
        };
        encode_subgraph_calls(&mut graph, &self.subgraphs);
        Some(graph)
    }

    /// The main event graph in the authored schema.
    pub(crate) fn main_authored_graph(&self, cx: &App) -> Result<GraphDescription, String> {
        let tab = self
            .open_tabs
            .iter()
            .find(|tab| tab.is_main)
            .ok_or("No main graph tab found")?;
        self.tab_authored_graph(&tab.id, cx)
            .ok_or_else(|| "The main graph is not available".to_string())
    }
}
