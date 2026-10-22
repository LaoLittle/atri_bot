use atri_core::{
    ExtensionInfo, ExtensionRole, ExtensionState, InteractionRequest, OutputTable, RuntimeNotice,
    TaskRecord, Value,
};
use comfy_table::{Table, presets::ASCII_FULL};
use std::fmt::Write;

pub fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}
pub fn state(state: &ExtensionState) -> &'static str {
    match state {
        ExtensionState::Disabled => "disabled",
        ExtensionState::Starting => "starting",
        ExtensionState::Running => "running",
        ExtensionState::Stopping => "stopping",
        ExtensionState::Failed => "failed",
    }
}
fn role(role: ExtensionRole) -> &'static str {
    match role {
        ExtensionRole::Adapter => "adapter",
        ExtensionRole::Service => "service",
    }
}
pub fn extensions(extensions: &[ExtensionInfo]) -> String {
    let mut table = Table::new();
    table.load_preset(ASCII_FULL).set_header([
        "Extension",
        "Version",
        "Kind",
        "Role",
        "State",
        "Tasks",
        "Diagnostic",
    ]);
    for info in extensions {
        table.add_row(vec![
            clean(&info.metadata.id),
            clean(&info.metadata.version),
            clean(&info.kind),
            role(info.metadata.role).to_owned(),
            state(&info.state).to_owned(),
            info.tasks.len().to_string(),
            clean(info.error.as_deref().unwrap_or("")),
        ]);
    }
    let mut output = format!("{table}\n");
    if extensions.is_empty() {
        output.push_str("No deployed extensions found.\n");
    }
    output
}
pub fn extension_info(info: &ExtensionInfo) -> String {
    let mut output = format!(
        "{} ({})\nVersion: {}\nState: {}\nKind: {}\nRole: {}\nArtifact: {}\n{}\n",
        clean(&info.metadata.name),
        clean(&info.metadata.id),
        clean(&info.metadata.version),
        state(&info.state),
        clean(&info.kind),
        role(info.metadata.role),
        clean(&info.path),
        clean(&info.metadata.description)
    );
    let _ = writeln!(
        output,
        "Configuration: {}\nCapabilities: {}",
        clean(&info.metadata.configuration.format),
        info.metadata
            .capabilities
            .iter()
            .map(|s| clean(s))
            .collect::<Vec<_>>()
            .join(", ")
    );
    for service in &info.metadata.services {
        let _ = writeln!(
            output,
            "Service: {} v{} [{}]",
            clean(&service.name),
            service.version,
            service
                .methods
                .iter()
                .map(|s| clean(s))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if let Some(error) = &info.error {
        let _ = writeln!(output, "Diagnostic: {}", clean(error));
    }
    output.push_str(&tasks(&info.tasks));
    output
}
pub fn tasks(tasks: &[TaskRecord]) -> String {
    let mut table = Table::new();
    table
        .load_preset(ASCII_FULL)
        .set_header(["ID", "Name", "State"]);
    for task in tasks {
        table.add_row([task.id.to_string(), clean(&task.name), clean(&task.state)]);
    }
    let mut output = format!("{table}\n");
    if tasks.is_empty() {
        output.push_str("No managed tasks.\n");
    }
    output
}
pub fn output_table(table: &OutputTable) -> String {
    let mut output = Table::new();
    output
        .load_preset(ASCII_FULL)
        .set_header(table.columns.iter().map(|s| clean(s)));
    for row in &table.rows {
        output.add_row(row.iter().map(|s| clean(s)));
    }
    format!("{}\n{output}", clean(&table.title))
}
pub fn interaction(request: &InteractionRequest) -> String {
    let mut text = format!("{} [{}]", clean(&request.title), clean(&request.kind));
    if let Some(account) = &request.account {
        let _ = write!(text, " — {}", clean(account));
    }
    text.push('\n');
    if request.kind == "qr" {
        match qrcode::QrCode::new(request.content.as_bytes()) {
            Ok(code) => {
                text.push_str(
                    &code
                        .render::<qrcode::render::unicode::Dense1x2>()
                        .quiet_zone(true)
                        .build(),
                );
                text.push('\n');
            }
            Err(_) => text.push_str("QR content cannot be encoded; use the text below.\n"),
        }
    }
    text.push_str(&clean(&request.content));
    text
}
pub fn event(event: &atri_core::Event) -> String {
    let mut text = format!(
        "event {} source={}{}",
        clean(event.kind.as_str()),
        clean(&event.source),
        event
            .account
            .as_ref()
            .map(|account| format!(" account={}", clean(account)))
            .unwrap_or_default()
    );
    if matches!(event.kind, atri_core::EventKind::BotOffline)
        && let Ok(offline) = event.decode::<atri_core::BotOffline>()
    {
        let _ = write!(text, " reason={}", clean(&offline.reason));
    }
    text
}

pub fn notice(notice: &RuntimeNotice) {
    match notice {
        RuntimeNotice::Log { extension, record } => println!(
            "[{:?}] {}: {}",
            record.level,
            clean(extension),
            clean(&record.message)
        ),
        RuntimeNotice::Event(value) => println!("{}", event(value)),
        RuntimeNotice::State(info) => {
            println!(
                "extension {}: {}{}",
                clean(&info.metadata.id),
                state(&info.state),
                info.error
                    .as_ref()
                    .map(|s| format!(" — {}", clean(s)))
                    .unwrap_or_default()
            );
        }
        RuntimeNotice::Table { extension, table } => {
            println!("output {}\n{}", clean(extension), output_table(table))
        }
        // Interactions are presented directly, never forwarded into persistent logs.
        RuntimeNotice::Interaction { extension, request } => {
            println!("interaction {}\n{}", clean(extension), interaction(request))
        }
    }
}
pub fn value(value: &Value) -> String {
    if let Ok(json) = serde_json::to_string_pretty(value) {
        return clean(&json);
    }
    match value {
        Value::Bytes(bytes) => format!("h'{}'", hex::encode(bytes)),
        _ => clean(&format!("{value:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generic_tables_and_interactions_render_real_values() {
        let table = OutputTable {
            title: "Inventory".into(),
            columns: vec!["Item".into()],
            rows: vec![vec!["actual".into()]],
        };
        assert!(output_table(&table).contains("actual"));
        let request = InteractionRequest {
            kind: "challenge".into(),
            title: "Continue".into(),
            content: "Open device settings".into(),
            account: None,
            secret: true,
        };
        assert!(interaction(&request).contains("Open device settings"));
    }
    #[test]
    fn terminal_control_sequences_cannot_escape_rendering() {
        assert_eq!(clean("ok\x1b[2J\x07\nnext"), "ok[2J\nnext");
        assert!(!clean("\x1b]52;c;c2VjcmV0\x07\u{009b}2J").contains(|c: char| c.is_control()));
        let rendered = value(&Value::Text("\u{009b}2J".into()));
        assert!(!rendered.contains('\u{009b}'));
    }
}
