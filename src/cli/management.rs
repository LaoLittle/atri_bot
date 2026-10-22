use super::{
    CliResult,
    args::{Command, ConfigCommand, PluginCommand, ServiceCommand, TaskCommand},
    render,
};
use atri_bot::{Runtime, control::ControlClient};
use atri_core::{
    ApiError, ConfigDocument, ConfigDocumentStore, ConfigSnapshot, ConfigStore, ExtensionInfo,
    Request, RuntimeSnapshot, ServiceCall, TaskRecord, Value, from_value,
};
use serde::Serialize;
use std::{io::Write, path::Path};

pub enum Backend {
    Remote(ControlClient),
    Local(Runtime),
}
impl Backend {
    pub async fn connect(path: &Path) -> Result<Self, ApiError> {
        match ControlClient::connect(path).await? {
            Some(client) => Ok(Self::Remote(client)),
            None => Ok(Self::Local(Runtime::open(path.to_owned()).await?)),
        }
    }
    pub async fn request(&self, method: &str, params: impl Serialize) -> Result<Value, ApiError> {
        let request = Request::new(method, params)?;
        match self {
            Self::Remote(client) => client.request(request).await,
            Self::Local(runtime) => runtime.control(request).await,
        }
    }
    pub async fn close(&self) -> Result<(), ApiError> {
        match self {
            Self::Local(runtime) => runtime.shutdown().await,
            Self::Remote(_) => Ok(()),
        }
    }
}

/// Execute the same runtime management commands for both stdout CLI and the console.
/// Output is returned, never printed while the console owns the terminal.
pub async fn dispatch(backend: &Backend, command: Command) -> CliResult<String> {
    match command {
        Command::Run(_) => Err(ApiError::new(
            "already_running",
            "The runtime is already running; use management commands or quit",
        )
        .into()),
        Command::Plugins { command } => plugin_command(backend, command).await,
        Command::Config { command } => config(backend, command).await,
        Command::Tasks { command } => tasks(backend, command).await,
        Command::Services {
            command: ServiceCommand::List,
        } => list_services(backend).await,
        Command::Services {
            command:
                ServiceCommand::Call {
                    service,
                    method,
                    account,
                    json,
                },
        } => {
            let args = service_args(&json)?;
            let response = backend
                .request(
                    "services.call",
                    ServiceCall {
                        service,
                        method,
                        account,
                        args: atri_core::to_value(&args)?,
                        caller: String::new(),
                        caller_kind: atri_core::CallerKind::Unknown,
                    },
                )
                .await?;
            Ok(format!("{}\n", render::value(&response)))
        }
        Command::Status => {
            let snapshot = from_value(&backend.request("runtime.status", ()).await?)?;
            Ok(render_status(&snapshot))
        }
        Command::Stop => {
            backend.request("runtime.stop", ()).await?;
            Ok("Runtime stop requested.\n".into())
        }
    }
}

pub fn service_args(json: &str) -> Result<serde_json::Value, ApiError> {
    serde_json::from_str(json).map_err(|error| {
        ApiError::new(
            "invalid_value",
            format!("--json must be valid JSON: {error}"),
        )
    })
}

async fn list_services(backend: &Backend) -> CliResult<String> {
    let extensions: Vec<ExtensionInfo> =
        from_value(&backend.request("extensions.list", ()).await?)?;
    let mut table = comfy_table::Table::new();
    table
        .load_preset(comfy_table::presets::ASCII_FULL)
        .set_header(["Provider", "State", "Service", "Version", "Methods"]);
    let mut count = 0;
    for extension in extensions {
        for service in extension.metadata.services {
            table.add_row([
                render::clean(&extension.metadata.id),
                render::state(&extension.state).into(),
                render::clean(&service.name),
                service.version.to_string(),
                service
                    .methods
                    .iter()
                    .map(|method| render::clean(method))
                    .collect::<Vec<_>>()
                    .join(", "),
            ]);
            count += 1;
        }
    }
    let mut output = format!("{table}\n");
    if count == 0 {
        output.push_str("No services declared by deployed extensions.\n");
    }
    Ok(output)
}

async fn plugin_command(backend: &Backend, command: PluginCommand) -> CliResult<String> {
    let output = match command {
        PluginCommand::List => {
            let plugins: Vec<ExtensionInfo> =
                from_value(&backend.request("extensions.list", ()).await?)?;
            render::extensions(&plugins)
        }
        PluginCommand::Enable { id } => {
            backend
                .request("extensions.enable", serde_json::json!({"id":id}))
                .await?;
            format!("{}: enabled\n", render::clean(&id))
        }
        PluginCommand::Disable { id } => {
            backend
                .request("extensions.disable", serde_json::json!({"id":id}))
                .await?;
            format!("{}: disabled\n", render::clean(&id))
        }
        PluginCommand::Info { id } => {
            let info = from_value(
                &backend
                    .request("extensions.info", serde_json::json!({"id":id}))
                    .await?,
            )?;
            render::extension_info(&info)
        }
        PluginCommand::Install { path } => {
            let path = path.canonicalize().map_err(ApiError::internal)?;
            let info: ExtensionInfo = from_value(
                &backend
                    .request("extensions.install", serde_json::json!({"path":path}))
                    .await?,
            )?;
            format!(
                "Installed {} {}\n",
                render::clean(&info.metadata.id),
                render::clean(&info.metadata.version)
            )
        }
        PluginCommand::Remove { id } => {
            backend
                .request("extensions.remove", serde_json::json!({"id":id}))
                .await?;
            format!(
                "Removed {}; extension storage retained.\n",
                render::clean(&id)
            )
        }
    };
    Ok(output)
}

async fn tasks(backend: &Backend, command: TaskCommand) -> CliResult<String> {
    match command {
        TaskCommand::List => {
            let tasks: Vec<TaskRecord> = from_value(&backend.request("tasks.list", ()).await?)?;
            Ok(render::tasks(&tasks))
        }
        TaskCommand::Cancel { id } => {
            backend
                .request("tasks.cancel", serde_json::json!({"id":id}))
                .await?;
            Ok(format!("Cancelled task {id}\n"))
        }
    }
}

fn render_status(snapshot: &RuntimeSnapshot) -> String {
    format!(
        "Runtime: {}\n{}",
        if snapshot.running {
            "running"
        } else {
            "offline"
        },
        render::extensions(&snapshot.extensions)
    )
}

/// Read-only status deliberately never opens a local runtime or acquires its writer lock.
pub async fn status(path: &Path) -> CliResult<String> {
    if let Some(client) = ControlClient::connect(path).await? {
        let snapshot = from_value(&client.request(Request::new("runtime.status", ())?).await?)?;
        Ok(render_status(&snapshot))
    } else {
        let artifacts = Runtime::deployed_artifacts(path)?;
        let mut output = String::from("Runtime: offline\n");
        if artifacts.is_empty() {
            output.push_str("No deployed extensions found.\n");
        } else {
            output.push_str("Deployed artifacts (not loaded):\n");
            for artifact in artifacts {
                output.push_str(&format!(
                    "  {}\n",
                    render::clean(&artifact.display().to_string())
                ));
            }
        }
        Ok(output)
    }
}

#[derive(Serialize)]
struct Store<'a, T> {
    id: &'a str,
    #[serde(flatten)]
    store: T,
}

pub async fn document(backend: &Backend, id: &str) -> Result<ConfigDocument, ApiError> {
    from_value(
        &backend
            .request("config.document.get", serde_json::json!({"id":id}))
            .await?,
    )
}

pub async fn commit_document(
    backend: &Backend,
    id: &str,
    original: &ConfigDocument,
    bytes: Vec<u8>,
    format: String,
) -> Result<ConfigDocument, ApiError> {
    from_value(
        &backend
            .request(
                "config.document.set",
                Store {
                    id,
                    store: ConfigDocumentStore {
                        bytes,
                        format,
                        expected_revision: Some(original.revision.clone()),
                    },
                },
            )
            .await?,
    )
}

async fn config(backend: &Backend, command: ConfigCommand) -> CliResult<String> {
    let output = match command {
        ConfigCommand::Show { id } => {
            let doc = document(backend, &id).await?;
            let mut output = match std::str::from_utf8(&doc.bytes) {
                Ok(text) => render::clean(text),
                Err(_) => hex::encode(&doc.bytes),
            };
            if !output.ends_with('\n') {
                output.push('\n');
            }
            output
        }
        ConfigCommand::Get { id, key } => {
            let snapshot: ConfigSnapshot = from_value(
                &backend
                    .request("config.get", serde_json::json!({"id":id}))
                    .await?,
            )?;
            let value = match key {
                Some(key) => pointer(&snapshot.value, &key)?,
                None => &snapshot.value,
            };
            format!("{}\n", render::value(value))
        }
        ConfigCommand::Set { id, key, value } => {
            let mut snapshot: ConfigSnapshot = from_value(
                &backend
                    .request("config.get", serde_json::json!({"id":id}))
                    .await?,
            )?;
            let replacement: serde_json::Value = serde_json::from_str(&value).map_err(|error| {
                ApiError::new(
                    "invalid_value",
                    format!("VALUE must be valid JSON: {error}"),
                )
            })?;
            replace_pointer(
                &mut snapshot.value,
                &key,
                atri_core::to_value(&replacement)?,
            )?;
            backend
                .request(
                    "config.set",
                    Store {
                        id: &id,
                        store: ConfigStore {
                            value: snapshot.value,
                            expected_revision: Some(snapshot.revision),
                            format: Some(snapshot.format),
                        },
                    },
                )
                .await?;
            format!("Updated {} {}\n", render::clean(&id), render::clean(&key))
        }
        ConfigCommand::Edit { id } => edit(backend, &id).await?,
        ConfigCommand::Import { id, path, format } => {
            let original = document(backend, &id).await?;
            let bytes = tokio::fs::read(&path).await.map_err(ApiError::internal)?;
            let format = format
                .or_else(|| {
                    path.extension()
                        .and_then(|part| part.to_str())
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| original.format.clone());
            commit_document(backend, &id, &original, bytes, format).await?;
            format!("Imported configuration for {}\n", render::clean(&id))
        }
    };
    Ok(output)
}

async fn edit(backend: &Backend, id: &str) -> Result<String, ApiError> {
    let original = document(backend, id).await?;
    let editor = std::env::var("VISUAL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            std::env::var("EDITOR")
                .ok()
                .filter(|v| !v.trim().is_empty())
        })
        .ok_or_else(|| {
            ApiError::new(
                "editor_missing",
                "Set VISUAL or EDITOR before editing configuration",
            )
        })?;
    let bytes = edit_copy(original.bytes.clone(), &original.format, &editor).await?;
    if bytes == original.bytes {
        return Ok("Configuration unchanged.\n".into());
    }
    commit_document(backend, id, &original, bytes, original.format.clone()).await?;
    Ok(format!("Updated configuration for {}\n", render::clean(id)))
}

async fn edit_copy(bytes: Vec<u8>, format: &str, editor: &str) -> Result<Vec<u8>, ApiError> {
    let command = shell_words::split(editor)
        .map_err(|error| ApiError::new("invalid_editor", error.to_string()))?;
    if command.is_empty() {
        return Err(ApiError::new("invalid_editor", "Editor command is empty"));
    }
    let suffix = format!(
        ".{}",
        if format.chars().all(|c| c.is_ascii_alphanumeric()) {
            format
        } else {
            "config"
        }
    );
    let (_directory, copy) = tokio::task::spawn_blocking(move || {
        // NamedTempFile is private (0600 on Unix); editors never see the live path.
        let directory = tempfile::Builder::new()
            .prefix("atri-edit-")
            .tempdir()
            .map_err(ApiError::internal)?;
        atri_bot::storage::private_directory(directory.path())?;
        let mut copy = tempfile::Builder::new()
            .prefix("config-")
            .suffix(&suffix)
            .tempfile_in(directory.path())
            .map_err(ApiError::internal)?;
        copy.write_all(&bytes).map_err(ApiError::internal)?;
        copy.flush().map_err(ApiError::internal)?;
        Ok::<_, ApiError>((directory, copy))
    })
    .await
    .map_err(ApiError::internal)??;
    let mut child = tokio::process::Command::new(&command[0])
        .args(&command[1..])
        .arg(copy.path())
        .kill_on_drop(true)
        .spawn()
        .map_err(ApiError::internal)?;
    let status = child.wait().await.map_err(ApiError::internal)?;
    if !status.success() {
        return Err(ApiError::new(
            "editor_failed",
            format!("Editor exited with {status}; configuration was not committed"),
        ));
    }
    tokio::fs::read(copy.path())
        .await
        .map_err(ApiError::internal)
}

fn segments(pointer: &str) -> Result<Vec<String>, ApiError> {
    if pointer.is_empty() {
        return Ok(Vec::new());
    }
    let Some(path) = pointer.strip_prefix('/') else {
        return Ok(vec![pointer.to_owned()]);
    };
    path.split('/')
        .map(|part| {
            let mut result = String::new();
            let mut chars = part.chars();
            while let Some(c) = chars.next() {
                if c != '~' {
                    result.push(c);
                    continue;
                }
                match chars.next() {
                    Some('0') => result.push('~'),
                    Some('1') => result.push('/'),
                    _ => {
                        return Err(ApiError::new(
                            "invalid_key",
                            "JSON Pointer escape must be ~0 or ~1",
                        ));
                    }
                }
            }
            Ok(result)
        })
        .collect()
}
fn missing() -> ApiError {
    ApiError::new("key_not_found", "Configuration key does not exist")
}
fn child<'a>(value: &'a Value, key: &str) -> Result<&'a Value, ApiError> {
    match value {
        Value::Map(entries) => entries
            .iter()
            .find_map(|(k, v)| {
                if k.as_text() == Some(key) {
                    Some(v)
                } else {
                    None
                }
            })
            .ok_or_else(missing),
        Value::Array(items) => items
            .get(key.parse::<usize>().map_err(|_| missing())?)
            .ok_or_else(missing),
        _ => Err(missing()),
    }
}
pub fn pointer<'a>(value: &'a Value, path: &str) -> Result<&'a Value, ApiError> {
    let mut current = value;
    for part in segments(path)? {
        current = child(current, &part)?;
    }
    Ok(current)
}
pub fn replace_pointer(value: &mut Value, path: &str, replacement: Value) -> Result<(), ApiError> {
    let parts = segments(path)?;
    let mut current = value;
    for (position, part) in parts.iter().enumerate() {
        let last = position + 1 == parts.len();
        match current {
            Value::Map(entries) => {
                if last
                    && !entries
                        .iter()
                        .any(|(k, _)| k.as_text() == Some(part.as_str()))
                {
                    entries.push((Value::Text(part.clone()), replacement));
                    return Ok(());
                }
                current = entries
                    .iter_mut()
                    .find_map(|(k, v)| {
                        if k.as_text() == Some(part.as_str()) {
                            Some(v)
                        } else {
                            None
                        }
                    })
                    .ok_or_else(missing)?;
            }
            Value::Array(items) => {
                current = items
                    .get_mut(part.parse::<usize>().map_err(|_| missing())?)
                    .ok_or_else(missing)?
            }
            _ => return Err(missing()),
        }
    }
    *current = replacement;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn dispatcher_uses_real_runtime_and_returns_all_management_output() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = Runtime::open(directory.path().to_owned()).await.unwrap();
        runtime.start().await.unwrap();
        let backend = Backend::Local(runtime);
        let output = dispatch(&backend, Command::Status).await.unwrap();
        assert!(output.starts_with("Runtime: running\n"));
        assert!(
            dispatch(
                &backend,
                Command::Plugins {
                    command: PluginCommand::List
                }
            )
            .await
            .unwrap()
            .contains("No deployed extensions found.")
        );
        assert!(
            dispatch(
                &backend,
                Command::Tasks {
                    command: TaskCommand::List
                }
            )
            .await
            .unwrap()
            .contains("No managed tasks.")
        );
        assert!(
            dispatch(
                &backend,
                Command::Services {
                    command: ServiceCommand::List
                }
            )
            .await
            .unwrap()
            .contains("No services declared")
        );
        assert!(
            dispatch(
                &backend,
                Command::Config {
                    command: ConfigCommand::Get {
                        id: "absent".into(),
                        key: None
                    },
                }
            )
            .await
            .is_err()
        );
        let error = dispatch(&backend, Command::Run(Default::default()))
            .await
            .unwrap_err();
        assert_eq!(error.error.code, "already_running");
        assert_eq!(
            dispatch(&backend, Command::Stop).await.unwrap(),
            "Runtime stop requested.\n"
        );
        backend.close().await.unwrap();
    }

    #[tokio::test]
    async fn offline_status_never_creates_a_runtime_or_storage() {
        let directory = tempfile::tempdir().unwrap();
        let absent = directory.path().join("uncreated-deployment");
        assert!(
            status(&absent)
                .await
                .unwrap()
                .starts_with("Runtime: offline\n")
        );
        assert!(!absent.exists());
    }

    #[test]
    fn pointer_edits_preserve_siblings_and_escape_keys() {
        let mut value =
            atri_core::to_value(&serde_json::json!({"a/b":{"~key":1},"keep":[2,3]})).unwrap();
        replace_pointer(&mut value, "/a~1b/~0key", Value::Bool(true)).unwrap();
        assert_eq!(pointer(&value, "/a~1b/~0key").unwrap(), &Value::Bool(true));
        assert_eq!(
            pointer(&value, "/keep/1").unwrap(),
            &Value::Integer(3.into())
        );
        assert!(replace_pointer(&mut value, "/missing/child", Value::Null).is_err());
        assert!(pointer(&value, "/bad~2escape").is_err());
    }
    #[test]
    fn pointer_can_add_leaf_or_replace_document() {
        let mut value = atri_core::empty_map();
        replace_pointer(&mut value, "/enabled", Value::Bool(true)).unwrap();
        assert_eq!(pointer(&value, "/enabled").unwrap(), &Value::Bool(true));
        replace_pointer(&mut value, "", Value::Array(vec![])).unwrap();
        assert_eq!(value, Value::Array(vec![]));
    }
    #[test]
    fn launch_script_can_set_top_level_boolean_keys() {
        let mut value = atri_core::empty_map();
        replace_pointer(&mut value, "fresh_qr", Value::Bool(true)).unwrap();
        replace_pointer(
            &mut value,
            "randomize_signature_for_testing",
            Value::Bool(false),
        )
        .unwrap();
        assert_eq!(pointer(&value, "fresh_qr").unwrap(), &Value::Bool(true));
        assert_eq!(
            pointer(&value, "/randomize_signature_for_testing").unwrap(),
            &Value::Bool(false)
        );
    }
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn external_editor_changes_only_private_copy() {
        let original = b"enabled = false\n".to_vec();
        let changed = edit_copy(original.clone(), "toml", "sh -c 'test \"$(stat -c %a \"$(dirname \"$1\")\")\" = 700 && printf \"enabled = true\\n\" > \"$1\"' atri-editor").await.unwrap();
        assert_eq!(changed, b"enabled = true\n");
        assert_eq!(original, b"enabled = false\n");
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn failed_external_editor_does_not_produce_commit_bytes() {
        let error = edit_copy(
            b"original".to_vec(),
            "toml",
            "sh -c 'printf damaged > \"$1\"; exit 9' atri-editor",
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "editor_failed");
    }
}
