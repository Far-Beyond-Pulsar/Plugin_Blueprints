//! The basics and intermediate example set (`examples/basics/...`): small, commented Blueprints,
//! one idea each. Generated from `examples_assets/basics/`; see `examples_assets/README.md`.

use engine_fs::EmbeddedFile;

/// The files of the set, at their path inside the plugin's folder.
pub(crate) const FILES: &[EmbeddedFile] = &[
    EmbeddedFile {
        path: "examples/README.md",
        bytes: include_bytes!("../../examples_assets/README.md"),
    },
    EmbeddedFile {
        path: "examples/basics/01_getting_started/HelloWorld.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/01_getting_started/HelloWorld.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/01_getting_started/PrintingValues.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/01_getting_started/PrintingValues.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/02_variables/Variables.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/02_variables/Variables.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/03_math/FloatMath.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/03_math/FloatMath.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/03_math/IntegerMath.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/03_math/IntegerMath.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/04_logic/BooleanLogic.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/04_logic/BooleanLogic.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/04_logic/Comparisons.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/04_logic/Comparisons.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/05_flow/Branching.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/05_flow/Branching.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/05_flow/FlowGates.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/05_flow/FlowGates.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/05_flow/Gate.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/05_flow/Gate.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/05_flow/MultiBranch.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/05_flow/MultiBranch.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/05_flow/Switches.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/05_flow/Switches.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/06_loops/ForLoop.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/06_loops/ForLoop.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/06_loops/NestedLoops.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/06_loops/NestedLoops.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/06_loops/WhileLoop.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/06_loops/WhileLoop.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/07_macros/Macros.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/07_macros/Macros.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/08_events/CustomEvents.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/08_events/CustomEvents.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/08_events/Lifecycle.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/08_events/Lifecycle.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/09_timing/Delays.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/09_timing/Delays.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/09_timing/Timers.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/09_timing/Timers.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/10_strings/StringBasics.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/10_strings/StringBasics.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/10_strings/StringFormatting.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/10_strings/StringFormatting.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/10_strings/StringSplitJoin.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/10_strings/StringSplitJoin.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/11_collections/Arrays.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/11_collections/Arrays.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/11_collections/Maps.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/11_collections/Maps.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/11_collections/Sets.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/11_collections/Sets.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/12_data_types/Conversions.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/12_data_types/Conversions.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/12_data_types/EnumStyleStates.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/12_data_types/EnumStyleStates.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/12_data_types/StructLikeValues.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/12_data_types/StructLikeValues.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/12_data_types/Vectors.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/12_data_types/Vectors.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/13_utilities/Assertions.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/13_utilities/Assertions.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/13_utilities/DateTime.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/13_utilities/DateTime.class/graph_save.json"),
    },
    EmbeddedFile {
        path: "examples/basics/13_utilities/Random.class/graph_save.json",
        bytes: include_bytes!("../../examples_assets/basics/13_utilities/Random.class/graph_save.json"),
    },
];
