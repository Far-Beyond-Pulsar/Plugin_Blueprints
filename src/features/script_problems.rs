//! Script problems from Play-in-Editor on the graph (Pulsar-Native#854,
//! #868).
//!
//! The editor publishes the running game's script problems on the host bus
//! (`pulsar_events::publish_script_problem`), each naming the class,
//! function and, from the module's debug info, the graph node. A problem
//! for the class open in this editor selects that node and is listed with
//! the validation problems.

use std::sync::{Arc, Mutex};

use gpui::Context;
use plugin_editor_api::pulsar_events::{self, ScriptProblem, ScriptProblemsEvent};

use crate::editor::panel::BlueprintEditorPanel;

/// How often the panel looks for new problems.
const POLL: std::time::Duration = std::time::Duration::from_millis(250);

/// Prefix of the entries this adds to `validation_problems`.
const PREFIX: &str = "Play: ";

impl BlueprintEditorPanel {
    /// Follow script problems for as long as the panel lives.
    pub(crate) fn watch_script_problems(cx: &mut Context<Self>) {
        let inbox: Arc<Mutex<Vec<ScriptProblemsEvent>>> = Arc::default();
        let sink = Arc::clone(&inbox);
        let subscription = pulsar_events::subscribe_script_problems(move |event| {
            sink.lock().unwrap_or_else(|p| p.into_inner()).push(event.clone());
        });
        cx.spawn(async move |this, cx| {
            let _subscription = subscription;
            loop {
                cx.background_executor().timer(POLL).await;
                let events = std::mem::take(&mut *inbox.lock().unwrap_or_else(|p| p.into_inner()));
                if events.is_empty() {
                    continue;
                }
                if this.update(cx, |panel, cx| panel.apply_script_problems(events, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn is_open_class(&self, problem: &ScriptProblem) -> bool {
        let Some(class_path) = &self.current_class_path else { return false };
        if let Some(path) = &problem.path {
            return path.starts_with(class_path);
        }
        let name = crate::features::class_dirs::class_name_of(class_path);
        name.is_some() && problem.class.as_deref().map(crate::features::class_dirs::strip_class_ext) == name.as_deref()
    }

    fn apply_script_problems(&mut self, events: Vec<ScriptProblemsEvent>, cx: &mut Context<Self>) {
        let mut changed = false;
        for event in events {
            match event {
                ScriptProblemsEvent::Cleared => {
                    self.validation_problems.retain(|p| !p.starts_with(PREFIX));
                    changed = true;
                }
                ScriptProblemsEvent::Reported(problem) if self.is_open_class(&problem) => {
                    let line = format!("{PREFIX}{}", problem.summary());
                    if !self.validation_problems.contains(&line) {
                        self.validation_problems.push(line);
                    }
                    if let (Some(node), Some(canvas)) = (problem.node.clone(), self.active_canvas().cloned()) {
                        canvas.update(cx, |canvas, cx| {
                            if canvas.graph.nodes.iter().any(|n| n.id == node) {
                                canvas.graph.selected_nodes = vec![node];
                                cx.notify();
                            }
                        });
                    }
                    changed = true;
                }
                ScriptProblemsEvent::Reported(_) => {}
            }
        }
        if changed {
            cx.notify();
        }
    }
}
