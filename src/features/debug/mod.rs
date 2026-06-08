//! Blueprint Debugger — breakpoints, call-stack navigation, and pin-value inspection.
//!
//! # Architecture
//!
//! Breakpoints are stored as a `HashSet<String>` of node IDs on the panel.  When
//! the fake executor (or eventually the real VM) visits a breakpointed node it
//! calls `hit_breakpoint()`, which pushes a `BreakpointFrame` onto the
//! `DebugSession` stack and sets `is_paused = true`.
//!
//! While paused, the debug HUD (rendered from `rendering/graph.rs`) shows a
//! bottom-anchored control bar with frame navigation and live pin values for
//! the current frame.  Red stop-sign badges are rendered over every node that
//! carries a breakpoint.
//!
//! Navigation (`step_forward` / `step_backward`) scrolls through the recorded
//! frame history and flies the viewport to each node via a smooth cubic-ease
//! animation.

use gpui::*;
use ui::PixelsExt;

use crate::editor::panel::BlueprintEditorPanel;

// ─── Public types ─────────────────────────────────────────────────────────────

/// A single resolved value on a data pin at the moment a breakpoint was hit.
#[derive(Clone, Debug)]
pub struct PinValue {
    pub pin_id: String,
    pub pin_name: String,
    pub type_label: String,
    pub value: String,
}

/// One entry in the debug call-stack.  Captures the node that was executing
/// and all resolved input-pin values visible at that point.
#[derive(Clone, Debug)]
pub struct BreakpointFrame {
    pub node_id: String,
    pub node_title: String,
    pub step_index: usize,
    /// Input pin values resolved up to this frame (data pins only).
    pub pin_values: Vec<PinValue>,
}

/// The live debug session; present only while execution is suspended at a
/// breakpoint or while the user navigates the frame history.
#[derive(Clone, Debug)]
pub struct DebugSession {
    /// All frames recorded since the session started (oldest first).
    pub frames: Vec<BreakpointFrame>,
    /// Which frame is currently shown in the HUD (0 = oldest).
    pub current_frame: usize,
    /// True while execution is suspended and waiting for a user command.
    pub is_paused: bool,
    /// Total execution steps taken in this session (monotonically increasing).
    pub total_steps: usize,
    /// When true, the simulation will pause again after executing exactly one
    /// more node (single-step mode). Set by `debug_step_forward` when paused.
    pub pause_after_step: bool,
}

impl DebugSession {
    pub fn new() -> Self {
        Self {
            frames: Vec::new(),
            current_frame: 0,
            is_paused: false,
            total_steps: 0,
            pause_after_step: false,
        }
    }

    /// The frame the HUD should display right now.
    pub fn current(&self) -> Option<&BreakpointFrame> {
        self.frames.get(self.current_frame)
    }

    pub fn can_step_backward(&self) -> bool {
        self.current_frame > 0
    }

    /// True only when the executor is suspended and can be single-stepped.
    /// Frame-history navigation is separate and always available when frames exist.
    pub fn can_step_forward(&self) -> bool {
        self.is_paused
    }
}

// ─── Breakpoint API on the panel ─────────────────────────────────────────────

impl crate::editor::workspace_panels::GraphCanvasPanel {
    pub fn toggle_breakpoint(&mut self, node_id: String, cx: &mut Context<Self>) {
        if self.breakpoints.contains(&node_id) {
            self.breakpoints.remove(&node_id);
        } else {
            self.breakpoints.insert(node_id);
        }
        cx.notify();
    }

    pub fn has_breakpoint(&self, node_id: &str) -> bool {
        self.breakpoints.contains(node_id)
    }

    /// Called by the executor when it reaches a breakpointed node.
    pub fn hit_breakpoint(&mut self, frame: BreakpointFrame, cx: &mut Context<Self>) {
        let node_id = frame.node_id.clone();
        let session = self.debug_session.get_or_insert_with(DebugSession::new);
        session.total_steps += 1;
        session.frames.push(frame);
        // Cap frame history at 256 to avoid unbounded growth.
        if session.frames.len() > 256 {
            session.frames.remove(0);
            if session.current_frame > 0 {
                session.current_frame -= 1;
            }
        }
        session.current_frame = session.frames.len() - 1;
        session.is_paused = true;
        self.animate_viewport_to_node(&node_id, cx);
        cx.notify();
    }

    /// Resume execution.
    pub fn debug_continue(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = &mut self.debug_session {
            s.is_paused = false;
        }
        cx.notify();
    }

    /// Single-step the executor forward by one graph node.
    ///
    /// Sets `pause_after_step` so the executor, currently suspended in its
    /// poll loop, will advance to the next connected node and immediately
    /// re-pause there — never at the same node it was already at.
    pub fn debug_step_forward(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = &mut self.debug_session {
            if s.is_paused {
                s.is_paused = false;
                s.pause_after_step = true;
            }
        }
        cx.notify();
    }

    /// Walk backward through the recorded frame history.
    pub fn debug_step_backward(&mut self, cx: &mut Context<Self>) {
        let node_id = if let Some(s) = &mut self.debug_session {
            if s.can_step_backward() {
                s.current_frame -= 1;
                Some(s.frames[s.current_frame].node_id.clone())
            } else {
                None
            }
        } else {
            None
        };
        if let Some(id) = node_id {
            self.animate_viewport_to_node(&id, cx);
        }
        cx.notify();
    }

    /// Tear down the debug session completely.
    pub fn debug_stop(&mut self, cx: &mut Context<Self>) {
        self.debug_session = None;
        cx.notify();
    }

    /// Smoothly animate the viewport so `node_id` is centred on screen.
    pub fn animate_viewport_to_node(&mut self, node_id: &str, cx: &mut Context<Self>) {
        let Some(node) = self.graph.nodes.iter().find(|n| n.id == node_id) else {
            return;
        };
        let target_graph_cx = node.position.x + node.size.width * 0.5;
        let target_graph_cy = node.position.y + node.size.height * 0.5;

        let (vw, vh) = self
            .element_bounds
            .map(|b| (b.size.width.as_f32(), b.size.height.as_f32()))
            .unwrap_or((1280.0, 720.0));
        let zoom = self.graph.zoom_level;

        // Solve: (target + pan) * zoom = viewport/2  →  pan = viewport/(2*zoom) - target
        let target_pan_x = vw / (2.0 * zoom) - target_graph_cx;
        let target_pan_y = vh / (2.0 * zoom) - target_graph_cy;
        let start_pan_x = self.graph.pan_offset.x;
        let start_pan_y = self.graph.pan_offset.y;

        let weak = cx.weak_entity();
        // `cx` in the spawn closure is `&mut AsyncApp` per GPUI's spawn signature.
        cx.spawn(async move |_, cx| {
            const STEPS: u32 = 18;
            const STEP_MS: u64 = 14; // ~250 ms total
            for i in 1..=STEPS {
                let t = i as f32 / STEPS as f32;
                let t_eased = 1.0 - (1.0 - t).powi(3); // cubic ease-out
                let pan_x = start_pan_x + (target_pan_x - start_pan_x) * t_eased;
                let pan_y = start_pan_y + (target_pan_y - start_pan_y) * t_eased;
                if weak
                    .update(cx, |panel, cx| {
                        panel.graph.pan_offset.x = pan_x;
                        panel.graph.pan_offset.y = pan_y;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
                smol::Timer::after(std::time::Duration::from_millis(STEP_MS)).await;
            }
        })
        .detach();
    }

    /// Generate a plausible fake pin value for display in the debug HUD.
    pub fn fake_pin_value(pin_name: &str, step: usize) -> String {
        let seed: usize = pin_name
            .bytes()
            .fold(step.wrapping_mul(2654435769), |acc, b| {
                acc.wrapping_mul(31).wrapping_add(b as usize)
            });

        let name_lower = pin_name.to_lowercase();

        if name_lower.contains("bool")
            || name_lower.contains("flag")
            || name_lower.contains("enabled")
        {
            return if seed & 1 == 0 {
                "true".into()
            } else {
                "false".into()
            };
        }
        if name_lower.contains("string")
            || name_lower.contains("text")
            || name_lower.contains("name")
        {
            let phrases = [
                "\"Hello World\"",
                "\"Pulsar\"",
                "\"debug_val\"",
                "\"node_result\"",
                "\"test\"",
            ];
            return phrases[seed % phrases.len()].into();
        }
        if name_lower.contains("vec")
            || name_lower.contains("pos")
            || name_lower.contains("location")
        {
            let x = (seed % 1000) as f32 / 10.0 - 50.0;
            let y = ((seed >> 4) % 1000) as f32 / 10.0 - 50.0;
            let z = ((seed >> 8) % 1000) as f32 / 10.0 - 50.0;
            return format!("({:.2}, {:.2}, {:.2})", x, y, z);
        }
        if name_lower.contains("rot") || name_lower.contains("angle") {
            let v = (seed % 36000) as f32 / 100.0;
            return format!("{:.1}°", v);
        }
        if name_lower.contains("color") || name_lower.contains("colour") {
            let r = (seed % 256) as u8;
            let g = ((seed >> 3) % 256) as u8;
            let b = ((seed >> 6) % 256) as u8;
            return format!("#{:02X}{:02X}{:02X}", r, g, b);
        }

        if seed & 3 == 0 {
            format!("{}", (seed % 10000) as i32 - 5000)
        } else {
            let f = (seed % 10000) as f32 / 100.0 - 50.0;
            format!("{:.3}", f)
        }
    }

    /// Collect fake pin values for all data inputs of `node_id`.
    fn fake_frame_for_node(&self, node_id: &str, step: usize) -> BreakpointFrame {
        let (title, pin_values) = self
            .graph
            .nodes
            .iter()
            .find(|n| n.id == node_id)
            .map(|node| {
                let values: Vec<PinValue> = node
                    .inputs
                    .iter()
                    .filter(|p| !p.data_type.is_execution())
                    .enumerate()
                    .map(|(i, pin)| PinValue {
                        pin_id: pin.id.clone(),
                        pin_name: pin.name.clone(),
                        type_label: format!("{:?}", pin.data_type)
                            .split('(')
                            .next()
                            .unwrap_or("?")
                            .to_string(),
                        value: Self::fake_pin_value(&pin.name, step.wrapping_add(i * 37)),
                    })
                    .collect();
                (node.title.clone(), values)
            })
            .unwrap_or_default();

        BreakpointFrame {
            node_id: node_id.to_string(),
            node_title: title,
            step_index: step,
            pin_values,
        }
    }

    /// Breakpoint-aware graph-walking executor.
    ///
    /// Maintains a live "program counter" (`current`) that follows execution
    /// connections node-by-node, exactly as a real executor would:
    ///
    ///   current → exec_next[current][branch] → …
    ///
    /// When a chain ends (no exec outputs), the simulator jumps to the next
    /// entry-point node (Event / MacroEntry) to simulate a new game-loop tick.
    ///
    /// Pauses on:
    ///   - Any node that carries a breakpoint.
    ///   - The very next node after the user pressed "Step Forward"
    ///     (`pause_after_step = true`).  This means Step always advances to a
    ///     *different* node — never loops back to the same breakpoint node.
    pub fn start_fake_execution_simulation(&mut self, cx: &mut Context<Self>) {
        let node_ids: Vec<String> = self.graph.nodes.iter().map(|n| n.id.clone()).collect();
        if node_ids.is_empty() {
            return;
        }

        // node_id → index
        let mut node_index = std::collections::HashMap::with_capacity(node_ids.len());
        for (idx, id) in node_ids.iter().enumerate() {
            node_index.insert(id.as_str().to_owned(), idx);
        }

        // Execution-pin adjacency list: src → [dst, …]
        let mut exec_next: Vec<Vec<usize>> = vec![Vec::<usize>::new(); node_ids.len()];
        // Also track which nodes have at least one incoming exec connection
        // so we can identify true entry points.
        let mut has_exec_input = vec![false; node_ids.len()];
        for conn in &self.graph.connections {
            if matches!(conn.connection_type, ui::graph::ConnectionType::Execution) {
                if let (Some(&src), Some(&dst)) = (
                    node_index.get(&conn.source_node),
                    node_index.get(&conn.target_node),
                ) {
                    exec_next[src].push(dst);
                    has_exec_input[dst] = true;
                }
            }
        }

        // Entry points: prefer Event/MacroEntry nodes; fall back to any node
        // that has no incoming execution connection.
        let mut entry_nodes: Vec<usize> = self
            .graph
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                matches!(
                    n.node_type,
                    crate::core::types::NodeType::Event | crate::core::types::NodeType::MacroEntry
                )
            })
            .map(|(i, _)| i)
            .collect();

        if entry_nodes.is_empty() {
            // No event nodes — treat every node without exec inputs as a root.
            entry_nodes = (0..node_ids.len())
                .filter(|&i| !has_exec_input[i])
                .collect();
        }
        if entry_nodes.is_empty() {
            // Fully cyclic graph (unlikely) — just start at 0.
            entry_nodes = vec![0];
        }

        let weak_panel = cx.weak_entity();
        cx.spawn(async move |_, cx| {
            let mut step: usize = 0;
            let mut entry_idx: usize = 0;
            // Program counter: the node we are about to execute.
            let mut current: usize = entry_nodes[entry_idx];
            // Simple LCG for branch selection when a node has multiple exec outputs.
            let mut rng: u64 = 0xDEAD_BEEF_C0FFEEu64;

            'outer: loop {
                let node_id = node_ids[current].clone();
                step = step.wrapping_add(1);

                // ── Pause check ───────────────────────────────────────────────
                // Read both flags in one atomic update to avoid TOCTOU.
                let (has_bp, do_step_pause) = weak_panel
                    .update(cx, |panel, _cx| {
                        let bp = panel.has_breakpoint(&node_id);
                        let sp = panel
                            .debug_session
                            .as_ref()
                            .map(|s| s.pause_after_step)
                            .unwrap_or(false);
                        (bp, sp)
                    })
                    .unwrap_or((false, false));

                if has_bp || do_step_pause {
                    // Push a frame and suspend.
                    if weak_panel
                        .update(cx, |panel, cx| {
                            let frame = panel.fake_frame_for_node(&node_id, step);
                            // Clear the single-step flag before pausing so that
                            // when the user presses Continue we don't re-pause.
                            if let Some(s) = &mut panel.debug_session {
                                s.pause_after_step = false;
                            }
                            panel.hit_breakpoint(frame, cx);
                        })
                        .is_err()
                    {
                        break 'outer;
                    }

                    // Poll until resumed (Continue sets is_paused=false;
                    // Step sets is_paused=false AND pause_after_step=true).
                    loop {
                        smol::Timer::after(std::time::Duration::from_millis(100)).await;
                        let still_paused = weak_panel
                            .update(cx, |panel, _cx| {
                                panel
                                    .debug_session
                                    .as_ref()
                                    .map(|s| s.is_paused)
                                    .unwrap_or(false)
                            })
                            .unwrap_or(false);
                        if !still_paused {
                            break;
                        }
                    }
                    // After resuming, fall through immediately to advance the
                    // program counter — the pause check at the TOP of the loop
                    // for the *next* node will catch pause_after_step if set.
                }

                // ── Show this node as "running" ───────────────────────────────
                if weak_panel
                    .update(cx, |panel, cx| {
                        panel.set_running_nodes(std::iter::once(node_id.as_str()), cx);
                    })
                    .is_err()
                {
                    break 'outer;
                }

                // ── Advance program counter ───────────────────────────────────
                let successors = &exec_next[current];
                let delay_ms;
                if successors.is_empty() {
                    // End of this execution chain → jump to the next entry node.
                    entry_idx = (entry_idx + 1) % entry_nodes.len();
                    current = entry_nodes[entry_idx];
                    delay_ms = 300u64; // brief pause simulating a game-loop boundary
                } else {
                    // Follow an execution output.  Rotate through branches so all
                    // paths of a branching node get exercised over time.
                    rng = rng
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    let branch = (rng >> 33) as usize % successors.len();
                    current = successors[branch];
                    delay_ms = 80;
                }

                smol::Timer::after(std::time::Duration::from_millis(delay_ms)).await;
            }
        })
        .detach();
    }
}
