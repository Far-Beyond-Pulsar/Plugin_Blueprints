/// Layout constants for node rendering.
/// All values are unscaled — multiply by zoom_level before converting to pixels.
///
/// Node height formula:
///   HEADER_H + SEP_H + BODY_PAD*2 + max_pins * PIN_ROW_H + (max_pins-1) * PIN_GAP

pub const HEADER_H: f32 = 28.0;
pub const SEP_H: f32 = 2.0;
pub const HEADER_PAD_X: f32 = 9.0;
pub const BODY_PAD: f32 = 8.0;
pub const PIN_ROW_H: f32 = 18.0;
pub const PIN_GAP: f32 = 4.0;
pub const PIN_SIZE: f32 = 12.0;

/// Grid snap interval (graph-space units). Node dimensions are rounded up to
/// the nearest multiple of this value so they align with grid snapping.
pub const GRID_SNAP: f32 = 10.0;

pub const NODE_BASE_H: f32 = HEADER_H + SEP_H + BODY_PAD * 2.0;

pub fn node_height_for_pin_rows(pin_rows: usize) -> f32 {
    let rows = pin_rows.max(1) as f32;
    NODE_BASE_H + rows * PIN_ROW_H + ((rows - 1.0).max(0.0)) * PIN_GAP
}

/// Minimum node width for its title, optional header return pin, and the
/// combined input/output labels that share each body row.
pub fn node_width_for_labels(
    current_width: f32,
    title_width: f32,
    header_output: Option<f32>,
    pin_label_rows: &[(f32, f32)],
) -> f32 {
    const LABEL_GAP: f32 = 12.0;
    let header_width = match header_output {
        Some(output_width) => {
            // Leave room between the title and the return label, and keep the
            // label clear of the header pin at node.width - 24.
            HEADER_PAD_X + title_width + LABEL_GAP + output_width + 24.0 + PIN_SIZE * 0.5 + 5.0
        }
        None => HEADER_PAD_X + title_width + HEADER_PAD_X,
    };
    let body_width = pin_label_rows
        .iter()
        .map(|(input_width, output_width)| {
            let label_gap = if *input_width > 0.0 && *output_width > 0.0 {
                LABEL_GAP
            } else {
                0.0
            };
            BODY_PAD * 2.0 + PIN_SIZE + 10.0 + input_width + output_width + label_gap
        })
        .fold(0.0_f32, f32::max);
    snap_to_grid(current_width.max(header_width).max(body_width))
}

/// Round `value` up to the nearest multiple of `GRID_SNAP`.
pub fn snap_to_grid(value: f32) -> f32 {
    (value / GRID_SNAP).ceil() * GRID_SNAP
}
