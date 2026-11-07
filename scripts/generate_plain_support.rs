//! Generate `docs/plain-support.md` from command-registry metadata.

use magit_lsp::commands::{registry, Command, CommandStatus, EffectKind};
use magit_lsp::presentation::{PlainAvailability, PlainCapabilities};

/// Render the capability-profile support table for the Plain presentation.
pub fn render() -> String {
    let minimal = PlainCapabilities::default();
    let edits = PlainCapabilities {
        apply_edit: true,
        document_changes: true,
        code_actions: true,
        disabled_actions: true,
        completion: true,
        folding_range: true,
        ..PlainCapabilities::default()
    };
    let full = PlainCapabilities {
        apply_edit: true,
        document_changes: true,
        code_actions: true,
        disabled_actions: true,
        show_document: true,
        work_done_progress: true,
        completion: true,
        folding_range: true,
        position_encodings: vec!["utf-8".to_owned()],
        commit_draft_expired: false,
    };
    let profiles: [(&str, &PlainCapabilities); 3] =
        [("minimal", &minimal), ("edits", &edits), ("full", &full)];

    let mut commands: Vec<&Command> = registry().iter().collect();
    commands.sort_by(|a, b| a.id.cmp(b.id));

    let mut out = String::from(
        "# Plain presentation command support\n\n\
This table is generated from command registry metadata (`Command::effects`, `Command::scope` and `Command::status`) and `PlainCapabilities::plain_availability`; it records declarations, not the result of executing each command. The `minimal`, `edits` and `full` columns evaluate three representative capability sets. Effect declarations are enforced when effects are delivered, so an available command still needs the listed capability before an optional effect reaches the client. Do not hand-edit this file; regenerate it with `UPDATE_PLAIN_SUPPORT=1 make test-rust FILTER=\"--test plain_support\"`.\n\n\
| Command | Scope | Status | Effects | Needs capability | minimal | edits | full | Reason |\n\
|---|---|---|---|---|---|---|---|---|\n",
    );
    for command in commands {
        let status = match command.status {
            CommandStatus::Implemented => "Implemented".to_owned(),
            CommandStatus::Planned { roadmap_id } => format!("Planned ({roadmap_id})"),
        };
        let effects = if command.effects.is_empty() {
            "—".to_owned()
        } else {
            command
                .effects
                .iter()
                .map(|effect| effect.name())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let needs = needs_capability(command.effects);
        let cells: Vec<String> = profiles
            .iter()
            .map(|(_, caps)| availability_cell(caps, command))
            .collect();
        let reason = reason_cell(command, &profiles);
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            command.id,
            command.scope.as_str(),
            status,
            effects,
            needs,
            cells[0],
            cells[1],
            cells[2],
            reason,
        ));
    }
    out
}

/// The capability a command's declared effects require, as table text.
fn needs_capability(effects: &[EffectKind]) -> String {
    if effects.is_empty() {
        return "—".to_owned();
    }
    let mut parts = Vec::new();
    for effect in effects {
        let part = match effect {
            EffectKind::EditDocument => "`workspace.applyEdit`",
            EffectKind::VisitFile => "`window.showDocument.support`",
            EffectKind::Clipboard => "none (Plain)",
        };
        if !parts.contains(&part) {
            parts.push(part);
        }
    }
    parts.join(", ")
}

/// One profile cell: planned status wins, otherwise the availability result.
fn availability_cell(caps: &PlainCapabilities, command: &Command) -> String {
    if let CommandStatus::Planned { roadmap_id } = command.status {
        return format!("Planned ({roadmap_id})");
    }
    match caps.plain_availability(command) {
        PlainAvailability::Available => "Available".to_owned(),
        PlainAvailability::Unavailable { reason } => format!("Unavailable — {reason}"),
    }
}

/// The distinct unavailable reasons across the profiles, or an em dash.
fn reason_cell(command: &Command, profiles: &[(&str, &PlainCapabilities)]) -> String {
    if matches!(command.status, CommandStatus::Planned { .. }) {
        return "—".to_owned();
    }
    let mut reasons: Vec<&str> = Vec::new();
    for (_, caps) in profiles {
        if let PlainAvailability::Unavailable { reason } = caps.plain_availability(command) {
            if !reasons.contains(&reason) {
                reasons.push(reason);
            }
        }
    }
    if reasons.is_empty() {
        "—".to_owned()
    } else {
        reasons.join("; ")
    }
}
