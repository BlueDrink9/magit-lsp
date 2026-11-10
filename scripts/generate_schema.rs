//! Directional JSON Schema generation for Plain wire payloads.

use schemars::generate::SchemaSettings;
use schemars::JsonSchema;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// Wire direction a payload schema describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Client-to-server input, generated from the deserialization contract.
    Input,
    /// Server-to-client output, generated from the serialization contract.
    Output,
}

/// One committed schema artifact under `docs/schema/`.
pub struct Artifact {
    /// File name relative to `docs/schema/`.
    pub file: &'static str,
    /// Generated root schema.
    pub schema: Value,
}

/// Generate the root schema for one payload using the requested wire direction.
pub fn directional_schema<T: JsonSchema>(direction: Direction) -> Value {
    let settings = match direction {
        Direction::Input => SchemaSettings::draft2020_12().for_deserialize(),
        Direction::Output => SchemaSettings::draft2020_12().for_serialize(),
    };
    let schema = settings.into_generator().into_root_schema_for::<T>();
    serde_json::to_value(schema).expect("generated schema must serialize")
}

/// Generate every committed artifact in stable file order.
pub fn artifacts() -> Vec<Artifact> {
    vec![
        Artifact {
            file: "plain-action-params.json",
            schema: directional_schema::<magit_lsp::dispatch::PlainActionParams>(Direction::Input),
        },
        Artifact {
            file: "edit-confirm-params.json",
            schema: directional_schema::<magit_lsp::protocol::EditConfirmParams>(Direction::Input),
        },
        Artifact {
            file: "edit-abort-params.json",
            schema: directional_schema::<magit_lsp::protocol::EditAbortParams>(Direction::Input),
        },
        Artifact {
            file: "edit-confirmation.json",
            schema: directional_schema::<magit_lsp::protocol::EditConfirmation>(Direction::Output),
        },
        Artifact {
            file: "edit-abort-result.json",
            schema: directional_schema::<magit_lsp::protocol::EditAbortResult>(Direction::Output),
        },
        Artifact {
            file: "effect.json",
            schema: directional_schema::<magit_lsp::protocol::Effect>(Direction::Output),
        },
        Artifact {
            file: "menu.json",
            schema: directional_schema::<magit_lsp::menu::Menu>(Direction::Output),
        },
        Artifact {
            file: "view-render.json",
            schema: directional_schema::<magit_lsp::protocol::ViewRender>(Direction::Output),
        },
        Artifact {
            file: "view-open-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewOpenParams>(Direction::Input),
        },
        Artifact {
            file: "view-descriptor.json",
            schema: directional_schema::<magit_lsp::view::ViewDescriptor>(Direction::Output),
        },
        Artifact {
            file: "view-close-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewCloseParams>(Direction::Input),
        },
        Artifact {
            file: "view-invoke-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewInvokeParams>(Direction::Input),
        },
        Artifact {
            file: "view-invoke-response.json",
            schema: directional_schema::<magit_lsp::protocol::ViewInvokeResponse>(Direction::Output),
        },
        Artifact {
            file: "view-key-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewKeyParams>(Direction::Input),
        },
        Artifact {
            file: "view-set-mode-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewSetModeParams>(Direction::Input),
        },
        Artifact {
            file: "view-resync-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewResyncParams>(Direction::Input),
        },
        Artifact {
            file: "view-rendered-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewRenderedParams>(Direction::Input),
        },
        Artifact {
            file: "view-resize-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewResizeParams>(Direction::Input),
        },
        Artifact {
            file: "view-effects-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewEffectsParams>(Direction::Output),
        },
        Artifact {
            file: "view-keymap-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewKeymapParams>(Direction::Output),
        },
        Artifact {
            file: "view-prompt-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewPromptParams>(Direction::Output),
        },
        Artifact {
            file: "view-prompt-respond-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewPromptRespondParams>(Direction::Input),
        },
        Artifact {
            file: "view-pick-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewPickParams>(Direction::Output),
        },
        Artifact {
            file: "view-pick-respond-params.json",
            schema: directional_schema::<magit_lsp::protocol::ViewPickRespondParams>(Direction::Input),
        },
    ]
}

/// Render one artifact as deterministic pretty JSON with a trailing newline.
pub fn render(artifact: &Artifact) -> String {
    let mut out = serde_json::to_string_pretty(&artifact.schema)
        .expect("generated schema must render as JSON");
    out.push('\n');
    out
}

/// Write every artifact under `root/docs/schema`.
pub fn write_all(root: &Path) -> std::io::Result<()> {
    let dir = root.join("docs/schema");
    fs::create_dir_all(&dir)?;
    for artifact in artifacts() {
        fs::write(dir.join(artifact.file), render(&artifact))?;
    }
    Ok(())
}

/// Compare fresh generation with every committed artifact.
///
/// Returns one relative path per stale or missing file.
pub fn stale_files(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let dir = root.join("docs/schema");
    let mut stale = Vec::new();
    for artifact in artifacts() {
        let path = dir.join(artifact.file);
        let fresh = render(&artifact);
        let committed = fs::read_to_string(&path).unwrap_or_default();
        if committed != fresh {
            stale.push(PathBuf::from("docs/schema").join(artifact.file));
        }
    }
    Ok(stale)
}
