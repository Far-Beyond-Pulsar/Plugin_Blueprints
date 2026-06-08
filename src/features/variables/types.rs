//! Variable type definitions

use gpui::*;
use ui::dropdown::DropdownItem;

/// Represents a class variable with name, type, and optional default value
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ClassVariable {
    pub name: String,
    pub var_type: String,
    pub default_value: Option<String>,
}

/// Drag data for variables - used when dragging variables from the panel to the graph
#[derive(Clone, Debug)]
pub struct VariableDrag {
    pub var_index: usize,
    pub var_name: String,
    pub var_type: String,
}

/// Wrapper type for dropdown items with colors - displays type information
#[derive(Clone, Debug)]
pub struct TypeItem {
    type_str: SharedString,
    display_name: SharedString,
}

impl TypeItem {
    pub fn new(type_str: String) -> Self {
        Self {
            display_name: type_str.clone().into(),
            type_str: type_str.into(),
        }
    }
}

impl DropdownItem for TypeItem {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.display_name.clone()
    }

    fn display_title(&self) -> Option<AnyElement> {
        // Get the color for this type
        let pin_color = crate::core::types::PinDataType::from_type_str(self.type_str.clone()).display_color();

        Some(
            ui::h_flex()
                .gap_2()
                .items_center()
                .child(
                    // Colored dot
                    div()
                        .w(px(10.))
                        .h(px(10.))
                        .rounded_full()
                        .bg(gpui::Rgba {
                            r: pin_color[0],
                            g: pin_color[1],
                            b: pin_color[2],
                            a: pin_color[3],
                        })
                        .border_1()
                        .border_color(gpui::Rgba {
                            r: 0.3,
                            g: 0.3,
                            b: 0.3,
                            a: 1.0,
                        }),
                )
                .child(div().flex_1().child(self.display_name.clone()))
                .child(
                    div()
                        .text_xs()
                        .text_color(gpui::Rgba {
                            r: 0.5,
                            g: 0.5,
                            b: 0.5,
                            a: 1.0,
                        })
                        .child(format!("({})", self.type_str)),
                )
                .into_any_element(),
        )
    }

    fn value(&self) -> &Self::Value {
        &self.type_str
    }
}
