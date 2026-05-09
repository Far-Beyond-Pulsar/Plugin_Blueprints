use gpui::{px, Hsla, Pixels};

pub fn body_bg() -> Hsla {
    // Opaque dark panel, slightly lighter than pure black for readability.
    Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.09,
        a: 1.0,
    }
}

pub fn title_bg(node_color: Hsla) -> Hsla {
    Hsla {
        h: node_color.h,
        s: (node_color.s * 0.90).min(1.0),
        l: (node_color.l * 0.65).clamp(0.14, 0.44),
        a: 1.0,
    }
}

pub fn idle_border() -> Hsla {
    Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.18,
        a: 1.0,
    }
}

/// A bright, saturated ring derived from the node's category color — used for
/// selected-node borders so the selection feels intentional, not generic.
pub fn selected_border(node_color: Hsla) -> Hsla {
    Hsla {
        h: node_color.h,
        s: (node_color.s * 0.7 + 0.3).min(1.0),
        l: 0.78,
        a: 1.0,
    }
}

/// Thin accent line between header and body — slightly brighter than the
/// header fill to add definition without a gradient.
pub fn accent_separator(node_color: Hsla) -> Hsla {
    Hsla {
        h: node_color.h,
        s: (node_color.s * 1.05).min(1.0),
        l: (node_color.l * 1.55).min(0.72),
        a: 1.0,
    }
}

pub fn separator_bg() -> Hsla {
    Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.13,
        a: 1.0,
    }
}

pub fn label_color() -> Hsla {
    Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.86,
        a: 1.0,
    }
}

pub fn corner_radius(z: f32) -> Pixels {
    px(7.0 * z)
}
