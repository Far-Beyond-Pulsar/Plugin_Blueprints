//! Stress-test example — generates a massive, sensibly laid-out blueprint graph
//! to benchmark the GPU renderer under high node and connection counts.
//!
//!   cargo run --example standalone_stress --release
//!
//! Graph stats at default settings:
//!   ~2 000 nodes, ~5 000 connections, ~14 000 × 6 000 graph units.
//!
//! Pan freely and zoom in/out to see the renderer hold up.
//! Right-click → Add Node, context menus, and connection drag all work normally.

use blueprint_editor_plugin::{
    BlueprintEditorPanel, BlueprintGraph, BlueprintNode, Connection, NodeType, Pin, PinType,
    VirtualizationStats,
};
use gpui::*;
use std::collections::HashMap;
use ui::graph::{ConnectionType, DataType};
use ui::{Assets, Root, Theme, ThemeMode};

// ── tuning knobs ─────────────────────────────────────────────────────────────

/// How many pipeline stages run left-to-right.
const COLS: usize = 200;
/// How many parallel tracks run top-to-bottom.
const ROWS: usize = 120;
/// Horizontal distance between column centres (graph units).
const COL_STRIDE: f32 = 270.0;
/// Vertical distance between row centres (graph units).
const ROW_STRIDE: f32 = 150.0;
/// How many rows belong to one named "module" section.
const MODULE_ROWS: usize = 10;

// ── data-type helpers ─────────────────────────────────────────────────────────

const TYPE_STRINGS: &[&str] = &["f32", "bool", "i64", "String", "execution"];

fn dt(s: &str) -> DataType {
    DataType::from_type_str(s)
}
fn exec() -> DataType {
    dt("execution")
}
fn float() -> DataType {
    dt("f32")
}
fn boolean() -> DataType {
    dt("bool")
}
fn integer() -> DataType {
    dt("i64")
}
fn string() -> DataType {
    dt("String")
}

/// Cycles through a variety of non-exec data types based on an index.
fn data_type(idx: usize) -> DataType {
    match idx % 4 {
        0 => float(),
        1 => boolean(),
        2 => integer(),
        _ => string(),
    }
}

fn connection_type_for(dt: &DataType) -> ConnectionType {
    if *dt == exec() {
        ConnectionType::Execution
    } else {
        ConnectionType::Data
    }
}

// ── node-type layout pattern ──────────────────────────────────────────────────

/// Returns the node type for a given column.
/// Every 10 columns form one "module":
///   0 → Event (entry)   1,4,8 → Logic   2,3,6 → Math   5,9 → Object   7 → Reroute
fn col_node_type(col: usize) -> NodeType {
    match col % 10 {
        0 => NodeType::Event,
        1 | 4 => NodeType::Logic,
        2 | 3 => NodeType::Math,
        5 => NodeType::Object,
        6 => NodeType::Math,
        7 => NodeType::Reroute,
        8 => NodeType::Logic,
        _ => NodeType::Object,
    }
}

fn node_icon(nt: &NodeType) -> &'static str {
    match nt {
        NodeType::Event => "⚡",
        NodeType::Logic => "⟐",
        NodeType::Math => "∑",
        NodeType::Object => "⬡",
        NodeType::Reroute => "•",
        NodeType::MacroEntry => "→",
        NodeType::MacroExit => "←",
        NodeType::MacroInstance => "⊡",
    }
}

// Title banks — cycled by row so adjacent nodes have varied names.
const EVENT_TITLES: &[&str] = &[
    "On Begin Play",
    "On Tick",
    "On Overlap",
    "On Damage",
    "On Input",
    "On Destroyed",
    "On Replicated",
    "On Notify",
    "On Timer",
    "On Custom",
];
const LOGIC_TITLES: &[&str] = &[
    "Branch",
    "Sequence",
    "Do Once",
    "For Loop",
    "Gate",
    "Flip Flop",
    "Multi Gate",
    "While Loop",
    "Do N",
    "Select",
];
const MATH_TITLES: &[&str] = &[
    "Add",
    "Multiply",
    "Lerp",
    "Clamp",
    "Normalize",
    "Dot Product",
    "Cross Product",
    "Abs",
    "Floor",
    "Sqrt",
    "Sin",
    "Cos",
    "Power",
    "Log",
    "Map Range",
];
const OBJECT_TITLES: &[&str] = &[
    "Set Location",
    "Get Component",
    "Spawn Actor",
    "Apply Damage",
    "Get Class",
    "Set Variable",
    "Cast To",
    "Get Actor",
    "Destroy Actor",
    "Get Owner",
];

fn node_title(nt: &NodeType, row: usize) -> String {
    let bank: &[&str] = match nt {
        NodeType::Event => EVENT_TITLES,
        NodeType::Logic => LOGIC_TITLES,
        NodeType::Math => MATH_TITLES,
        NodeType::Object => OBJECT_TITLES,
        NodeType::Reroute => return "•".into(),
        _ => LOGIC_TITLES,
    };
    bank[row % bank.len()].to_string()
}

// ── node height calculator (mirrors layout::node_height_for_pin_rows) ─────────

fn node_height(pin_rows: usize) -> f32 {
    // HEADER_H(28) + SEP_H(2) + BODY_PAD*2(16) + rows*PIN_ROW_H(18) + (rows-1)*PIN_GAP(4)
    let r = pin_rows.max(1) as f32;
    28.0 + 2.0 + 16.0 + r * 18.0 + (r - 1.0).max(0.0) * 4.0
}

// ── graph generator ───────────────────────────────────────────────────────────

fn build_stress_graph() -> BlueprintGraph {
    let total = COLS * ROWS;
    let mut nodes = Vec::with_capacity(total);
    // node_id[col][row] for quick lookup during connection building
    let mut node_ids: Vec<Vec<String>> = vec![vec![String::new(); ROWS]; COLS];
    // output-exec pin id, output-data pin id for each node
    let mut exec_out: Vec<Vec<Option<String>>> = vec![vec![None; ROWS]; COLS];
    let mut data_out: Vec<Vec<Option<String>>> = vec![vec![None; ROWS]; COLS];
    // input-exec pin id, input-data pin id for each node
    let mut exec_in: Vec<Vec<Option<String>>> = vec![vec![None; ROWS]; COLS];
    let mut data_in_p: Vec<Vec<Option<String>>> = vec![vec![None; ROWS]; COLS];

    // ── nodes ────────────────────────────────────────────────────────────────
    for col in 0..COLS {
        for row in 0..ROWS {
            let id = format!("n_{col}_{row}");
            let nt = col_node_type(col);
            let is_reroute = nt == NodeType::Reroute;
            let is_event = nt == NodeType::Event;

            let x = col as f32 * COL_STRIDE;
            let y = row as f32 * ROW_STRIDE;

            // The primary data type this node works with (cycles per row)
            let primary_dt = data_type(row + col);

            let (inputs, outputs, w, h) = if is_reroute {
                let in_id = format!("{id}_in");
                let out_id = format!("{id}_out");
                exec_out[col][row] = Some(out_id.clone());
                exec_in[col][row] = Some(in_id.clone());
                data_out[col][row] = Some(out_id.clone());
                data_in_p[col][row] = Some(in_id.clone());
                (
                    vec![Pin {
                        id: in_id,
                        name: "".into(),
                        pin_type: PinType::Input,
                        data_type: primary_dt.clone(),
                    }],
                    vec![Pin {
                        id: out_id,
                        name: "".into(),
                        pin_type: PinType::Output,
                        data_type: primary_dt,
                    }],
                    22.0_f32,
                    22.0_f32,
                )
            } else {
                let mut ins = Vec::new();
                let mut outs = Vec::new();

                // All non-event nodes accept an exec in
                if !is_event {
                    let pid = format!("{id}_exec_in");
                    exec_in[col][row] = Some(pid.clone());
                    ins.push(Pin {
                        id: pid,
                        name: "".into(),
                        pin_type: PinType::Input,
                        data_type: exec(),
                    });
                }

                // Data input (except events which only produce)
                if !is_event {
                    let pid = format!("{id}_data_in");
                    data_in_p[col][row] = Some(pid.clone());
                    ins.push(Pin {
                        id: pid,
                        name: pin_in_label(&nt),
                        pin_type: PinType::Input,
                        data_type: primary_dt.clone(),
                    });
                }

                // Extra inputs for variety on some node types
                if matches!(nt, NodeType::Math | NodeType::Logic) {
                    ins.push(Pin {
                        id: format!("{id}_aux_in"),
                        name: aux_in_label(&nt),
                        pin_type: PinType::Input,
                        data_type: data_type(row + col + 1),
                    });
                }

                // Exec output
                {
                    let pid = format!("{id}_exec_out");
                    exec_out[col][row] = Some(pid.clone());
                    outs.push(Pin {
                        id: pid,
                        name: "".into(),
                        pin_type: PinType::Output,
                        data_type: exec(),
                    });
                }

                // Logic nodes get True/False branches
                if nt == NodeType::Logic {
                    let t_pid = format!("{id}_true");
                    let f_pid = format!("{id}_false");
                    outs.push(Pin {
                        id: t_pid,
                        name: "True".into(),
                        pin_type: PinType::Output,
                        data_type: exec(),
                    });
                    outs.push(Pin {
                        id: f_pid,
                        name: "False".into(),
                        pin_type: PinType::Output,
                        data_type: exec(),
                    });
                }

                // Data output
                {
                    let pid = format!("{id}_data_out");
                    data_out[col][row] = Some(pid.clone());
                    outs.push(Pin {
                        id: pid,
                        name: pin_out_label(&nt),
                        pin_type: PinType::Output,
                        data_type: primary_dt,
                    });
                }

                let max_pins = ins.len().max(outs.len());
                let h = node_height(max_pins);
                (ins, outs, 210.0_f32, h)
            };

            node_ids[col][row] = id.clone();

            nodes.push(BlueprintNode {
                id,
                definition_id: format!("stress_{col}_{row}"),
                title: node_title(&nt, row),
                icon: node_icon(&nt).to_string(),
                node_type: nt,
                position: Point::new(x, y),
                size: Size::new(w, h),
                inputs,
                outputs,
                properties: HashMap::new(),
                is_selected: false,
                description: String::new(),
                color: None,
            });
        }
    }

    // ── connections ──────────────────────────────────────────────────────────
    let mut connections = Vec::new();
    let mut cid = 0u32;

    // Helper to push a connection safely
    let mut push = |connections: &mut Vec<Connection>,
                    cid: &mut u32,
                    src_node: &str,
                    src_pin: &str,
                    dst_node: &str,
                    dst_pin: &str,
                    dt: &DataType| {
        connections.push(Connection {
            id: format!("c{cid}"),
            source_node: src_node.to_string(),
            source_pin: src_pin.to_string(),
            target_node: dst_node.to_string(),
            target_pin: dst_pin.to_string(),
            connection_type: connection_type_for(dt),
        });
        *cid += 1;
    };

    for col in 0..COLS {
        let nc = col + 1;
        if nc >= COLS {
            break;
        }

        for row in 0..ROWS {
            // ── 1. Exec chain: straight across to next column, same row ──────
            if let (Some(ref sp), Some(ref dp)) =
                (exec_out[col][row].clone(), exec_in[nc][row].clone())
            {
                push(
                    &mut connections,
                    &mut cid,
                    &node_ids[col][row],
                    sp,
                    &node_ids[nc][row],
                    dp,
                    &exec(),
                );
            }

            // ── 2. Data: same row, next column ───────────────────────────────
            if let (Some(ref sp), Some(ref dp)) =
                (data_out[col][row].clone(), data_in_p[nc][row].clone())
            {
                // Only connect if the data type is compatible (always true for stress test)
                push(
                    &mut connections,
                    &mut cid,
                    &node_ids[col][row],
                    sp,
                    &node_ids[nc][row],
                    dp,
                    &data_type(row + col),
                );
            }

            // ── 3. Diagonal data: row+1, 2 cols ahead ────────────────────────
            let dc = col + 2;
            if dc < COLS && row + 1 < ROWS && col % 4 == 1 {
                if let (Some(ref sp), Some(ref dp)) =
                    (data_out[col][row].clone(), data_in_p[dc][row + 1].clone())
                {
                    push(
                        &mut connections,
                        &mut cid,
                        &node_ids[col][row],
                        sp,
                        &node_ids[dc][row + 1],
                        dp,
                        &data_type(row + col + 1),
                    );
                }
            }

            // ── 4. Long-range: 3-5 cols ahead, same row ──────────────────────
            let lc = col + 3 + (row % 3);
            if lc < COLS && col % 5 == 0 && row % 4 == 0 {
                if let (Some(ref sp), Some(ref dp)) =
                    (data_out[col][row].clone(), data_in_p[lc][row].clone())
                {
                    push(
                        &mut connections,
                        &mut cid,
                        &node_ids[col][row],
                        sp,
                        &node_ids[lc][row],
                        dp,
                        &data_type(row + col + 2),
                    );
                }
            }

            // ── 5. Cross-row merge: collect from row above every 6 rows ──────
            if row > 0 && row % 6 == 0 && nc < COLS {
                if let (Some(ref sp), Some(ref dp)) =
                    (data_out[col][row - 1].clone(), data_in_p[nc][row].clone())
                {
                    push(
                        &mut connections,
                        &mut cid,
                        &node_ids[col][row - 1],
                        sp,
                        &node_ids[nc][row],
                        dp,
                        &data_type(row + col),
                    );
                }
            }
        }
    }

    eprintln!(
        "Stress graph: {} nodes, {} connections",
        nodes.len(),
        connections.len()
    );

    BlueprintGraph {
        nodes,
        connections,
        comments: Vec::new(),
        selected_nodes: Vec::new(),
        selected_comments: Vec::new(),
        // Zoomed far out to show the broad shape; zoom in to inspect detail.
        // At 0.07 the full 54 000 × 18 000 unit graph fits on a 1600p wide screen.
        zoom_level: 0.07,
        pan_offset: Point::new(30.0, 30.0),
        virtualization_stats: VirtualizationStats::default(),
        custom_event_defs: HashMap::new(),
    }
}

// ── pin label helpers ─────────────────────────────────────────────────────────

fn pin_in_label(nt: &NodeType) -> String {
    match nt {
        NodeType::Logic => "Condition".into(),
        NodeType::Math => "A".into(),
        NodeType::Object => "Target".into(),
        _ => "In".into(),
    }
}

fn aux_in_label(nt: &NodeType) -> String {
    match nt {
        NodeType::Logic => "Alt".into(),
        NodeType::Math => "B".into(),
        _ => "Aux".into(),
    }
}

fn pin_out_label(nt: &NodeType) -> String {
    match nt {
        NodeType::Event => "".into(),
        NodeType::Logic => "Value".into(),
        NodeType::Math => "Result".into(),
        NodeType::Object => "Object".into(),
        _ => "Out".into(),
    }
}

// ── entry point ──────────────────────────────────────────────────────────────

fn main() {
    Application::new().with_assets(Assets).run(|cx: &mut App| {
        ui::init(cx);
        ui::themes::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);

        let graph = build_stress_graph();

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point {
                        x: px(60.0),
                        y: px(60.0),
                    },
                    size: Size {
                        width: px(1600.0),
                        height: px(960.0),
                    },
                })),
                titlebar: Some(TitlebarOptions {
                    title: Some(
                        format!(
                        "Blueprint Stress Test — {:} nodes  (check stderr for connection count)",
                        COLS * ROWS,
                    )
                        .into(),
                    ),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |window, cx| {
                let panel = cx.new(|cx| {
                    let mut p = BlueprintEditorPanel::new(window, cx);
                    // Keep editor shadow graph and main tab graph in sync.
                    p.graph = graph.clone();
                    if let Some(main_tab) = p.open_tabs.get_mut(0) {
                        main_tab.graph = graph.clone();
                    }
                    p.start_compilation(cx);
                    p
                });
                cx.new(|cx| Root::new(panel.into(), window, cx))
            },
        )
        .expect("failed to open window");
    });
}
