//! Runtime-owned configuration and private extension state.
//!
//! Native extensions are trusted machine code. These APIs enforce an ownership
//! and lifecycle contract; they do not sandbox native code that opens files itself.
use atri_core::{
    ApiError, ConfigDocument, ConfigDocumentStore, ConfigLoad, ConfigSnapshot, ConfigSpec,
    ConfigStore, ExtensionMetadata, ExtensionPaths, StorageRead, StorageWrite, Value,
};
use fs4::fs_std::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};

const REGISTRY: &str = ".atri-registry.cbor";
const OWNER: &str = ".atri-owner.cbor";
const CONTROL: &str = ".atri-control.cbor";
const TRANSACTION: &str = ".atri-transaction.cbor";
const MISSING: &str = "missing";
const MAX_BYTES: u64 = atri_core::MAX_CALL_BYTES as u64;
const DATETIME: &str = "$__toml_private_datetime";

type Result<T> = std::result::Result<T, ApiError>;

#[derive(Clone)]
pub struct Storage(Arc<Inner>);
struct Inner {
    root: PathBuf,
    // Every Storage clone retains the writer lease, including in-flight blocking work.
    _process_lock: ProcessLock,
    registry: Mutex<Registry>,
    plugins: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    control: Mutex<()>,
}
struct ProcessLock {
    file: File,
    owner: u32,
}
impl Drop for ProcessLock {
    fn drop(&mut self) {
        // flock belongs to the open-file description, not just this descriptor.
        // A concurrent fork can inherit it until exec despite CLOEXEC. Explicitly
        // release our lease rather than making shutdown wait for that child's close.
        // A forked child must never release its still-running parent's lease.
        if self.owner == std::process::id() {
            let _ = FileExt::unlock(&self.file);
        }
    }
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    version: u32,
    owners: BTreeMap<String, String>,
    disabled: BTreeSet<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    id: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigTransaction {
    previous_format: Option<String>,
    previous_revision: String,
    format: String,
    #[serde(with = "serde_bytes")]
    bytes: Vec<u8>,
}
struct Document {
    format: String,
    bytes: Vec<u8>,
    revision: String,
}

impl Storage {
    pub fn open(bot_dir: PathBuf) -> Result<Self> {
        let root = absolute_root(&bot_dir)?;
        ensure_directory_tree(&root, true)?;
        let lock_path = root.join(".atri.lock");
        check_regular_or_missing(&lock_path)?;
        let lock = private_options()
            .read(true)
            .write(true)
            .create(true)
            .open(&lock_path)
            .map_err(io_error)?;
        make_private(&lock_path, false)?;
        if !lock.try_lock_exclusive().map_err(io_error)? {
            return Err(ApiError::new(
                "runtime_locked",
                "another process owns this bot directory",
            ));
        }
        let lock = ProcessLock {
            file: lock,
            owner: std::process::id(),
        };
        cleanup_staging(&root)?;
        let storage_root = root.join("storage");
        ensure_directory_tree(&storage_root, true)?;
        let mut registry = match read_optional(&root.join(REGISTRY))? {
            Some(bytes) => decode_metadata::<Registry>(&bytes, "registry")?,
            None => Registry {
                version: 1,
                ..Registry::default()
            },
        };
        validate_registry(&registry)?;
        recover_owners(&root, &mut registry)?;
        // There is exactly one ownership/enable-state commit, never two metadata
        // files whose independent renames could silently lose a disabled entry.
        if !root.join(REGISTRY).exists() {
            save_registry(&root, &registry)?;
        }
        Ok(Self(Arc::new(Inner {
            root,
            _process_lock: lock,
            registry: Mutex::new(registry),
            plugins: Mutex::new(HashMap::new()),
            control: Mutex::new(()),
        })))
    }

    pub fn root(&self) -> &Path {
        &self.0.root
    }

    pub async fn prepare(&self, descriptor: &ExtensionMetadata) -> Result<ExtensionPaths> {
        self.with_plugin(&descriptor.id, move |inner, id| {
            let mut registry = guarded(&inner.registry)?;
            let directory = match registry.owners.get(id) {
                Some(existing) => existing.clone(),
                None => allocate_directory(id, &registry)?,
            };
            match registry.owners.get(id) {
                Some(_) => {}
                None => {
                    if registry.owners.values().any(|value| value == &directory) {
                        return Err(ownership_error(id));
                    }
                    let path = inner.root.join("storage").join(&directory);
                    if path.try_exists().map_err(io_error)? {
                        return Err(ApiError::new(
                            "orphaned_storage",
                            "unregistered storage directory must not be claimed",
                        ));
                    }
                    registry.owners.insert(id.to_owned(), directory.clone());
                    if let Err(error) = save_registry(&inner.root, &registry) {
                        registry.owners.remove(id);
                        return Err(error);
                    }
                }
            }
            let root = inner.root.join("storage").join(directory);
            ensure_owned_directory(&root, id)?;
            drop(registry);
            for area in ["config", "data", "workspace"] {
                ensure_directory_tree(&root.join(area), true)?;
            }
            recover_config(&root.join("config"))?;
            Ok(ExtensionPaths {
                root: path_string(&root)?,
                data: path_string(&root.join("data"))?,
                workspace: path_string(&root.join("workspace"))?,
            })
        })
        .await
    }

    pub async fn disabled(&self) -> Result<BTreeSet<String>> {
        Ok(guarded(&self.0.registry)?.disabled.clone())
    }

    pub async fn set_enabled(&self, id: &str, enabled: &bool) -> Result<()> {
        validate_id(id)?;
        let this = self.clone();
        let id = id.to_owned();
        let enabled = *enabled;
        blocking(move || {
            let mut registry = guarded(&this.0.registry)?;
            let previous = registry.disabled.clone();
            if enabled {
                registry.disabled.remove(&id);
            } else {
                registry.disabled.insert(id);
            }
            if let Err(error) = save_registry(&this.0.root, &registry) {
                registry.disabled = previous;
                return Err(error);
            }
            Ok(())
        })
        .await
    }

    pub async fn config_load(
        &self,
        id: &str,
        spec: &ConfigSpec,
        request: ConfigLoad,
    ) -> Result<ConfigSnapshot> {
        let default_format = spec.format.clone();
        let defaults = request.defaults.unwrap_or_else(|| spec.defaults.clone());
        self.with_plugin(id, move |inner, id| {
            let directory = config_directory(inner, id)?;
            let existing = current_document(&directory)?;
            let format = select_format(
                existing.as_ref(),
                request.format.as_deref(),
                &default_format,
            )?;
            require_builtin(&format)?;
            if let Some(document) = existing {
                return snapshot(document);
            }
            let bytes = encode_config(&format, &defaults)?;
            let document = commit_document(&directory, None, format, bytes)?;
            snapshot(document)
        })
        .await
    }

    pub async fn config_store(
        &self,
        id: &str,
        spec: &ConfigSpec,
        request: ConfigStore,
    ) -> Result<ConfigSnapshot> {
        let default_format = spec.format.clone();
        self.with_plugin(id, move |inner, id| {
            let directory = config_directory(inner, id)?;
            let existing = current_document(&directory)?;
            check_revision(existing.as_ref(), request.expected_revision.as_deref())?;
            protect_malformed(existing.as_ref(), request.expected_revision.as_deref())?;
            let format = select_format(
                existing.as_ref(),
                request.format.as_deref(),
                &default_format,
            )?;
            let bytes = encode_config(&format, &request.value)?;
            snapshot(commit_document(
                &directory,
                existing.as_ref(),
                format,
                bytes,
            )?)
        })
        .await
    }

    pub async fn config_document(&self, id: &str, spec: &ConfigSpec) -> Result<ConfigDocument> {
        let default_format = spec.format.clone();
        self.with_plugin(id, move |inner, id| {
            let directory = config_directory(inner, id)?;
            match current_document(&directory)? {
                Some(document) => Ok(public_document(document)),
                None => Ok(ConfigDocument {
                    bytes: Vec::new(),
                    format: normalized_format(&default_format)?,
                    revision: MISSING.into(),
                }),
            }
        })
        .await
    }

    pub async fn config_document_store(
        &self,
        id: &str,
        _spec: &ConfigSpec,
        request: ConfigDocumentStore,
    ) -> Result<ConfigDocument> {
        self.with_plugin(id, move |inner, id| {
            let directory = config_directory(inner, id)?;
            let existing = current_document(&directory)?;
            check_revision(existing.as_ref(), request.expected_revision.as_deref())?;
            protect_malformed(existing.as_ref(), request.expected_revision.as_deref())?;
            let format = normalized_format(&request.format)?;
            check_size(&request.bytes)?;
            if is_builtin(&format) {
                decode_config(&format, &request.bytes)?;
            }
            Ok(public_document(commit_document(
                &directory,
                existing.as_ref(),
                format,
                request.bytes,
            )?))
        })
        .await
    }

    /// Atomically mutate one unambiguous dotted key. Intermediate scalar values
    /// are errors, not discarded subtrees. Quoted/escaped segments name literal dots.
    pub async fn config_set_key(
        &self,
        id: &str,
        spec: &ConfigSpec,
        key: &str,
        value: Value,
        expected_revision: Option<String>,
    ) -> Result<ConfigSnapshot> {
        let segments = parse_key(key)?;
        let default_format = spec.format.clone();
        let defaults = spec.defaults.clone();
        self.with_plugin(id, move |inner, id| {
            let directory = config_directory(inner, id)?;
            let existing = current_document(&directory)?;
            check_revision(existing.as_ref(), expected_revision.as_deref())?;
            let format = select_format(existing.as_ref(), None, &default_format)?;
            let mut document = match &existing {
                Some(document) => decode_config(&format, &document.bytes)?,
                None => defaults,
            };
            mutate_key(&mut document, &segments, value)?;
            let bytes = encode_config(&format, &document)?;
            snapshot(commit_document(
                &directory,
                existing.as_ref(),
                format,
                bytes,
            )?)
        })
        .await
    }

    /// Import is a typed conversion into the currently selected runtime format,
    /// not a file copy and not a lossy trip through a JSON intermediate value.
    pub async fn config_import(
        &self,
        id: &str,
        spec: &ConfigSpec,
        request: ConfigDocumentStore,
    ) -> Result<ConfigSnapshot> {
        let default_format = spec.format.clone();
        self.with_plugin(id, move |inner, id| {
            let directory = config_directory(inner, id)?;
            let existing = current_document(&directory)?;
            check_revision(existing.as_ref(), request.expected_revision.as_deref())?;
            protect_malformed(existing.as_ref(), request.expected_revision.as_deref())?;
            let imported = decode_config(&normalized_format(&request.format)?, &request.bytes)?;
            let format = select_format(existing.as_ref(), None, &default_format)?;
            let bytes = encode_config(&format, &imported)?;
            snapshot(commit_document(
                &directory,
                existing.as_ref(),
                format,
                bytes,
            )?)
        })
        .await
    }

    pub async fn read(&self, id: &str, request: StorageRead) -> Result<Vec<u8>> {
        self.with_plugin(id, move |inner, id| {
            let root = area_directory(inner, id, &request.area)?;
            let path = private_relative_path(&root, &request.path, false)?;
            read_optional(&path)?
                .ok_or_else(|| ApiError::new("not_found", "storage file does not exist"))
        })
        .await
    }

    pub async fn write(&self, id: &str, request: StorageWrite) -> Result<()> {
        self.with_plugin(id, move |inner, id| {
            check_size(&request.bytes)?;
            let root = area_directory(inner, id, &request.area)?;
            let path = private_relative_path(&root, &request.path, true)?;
            atomic_write(&path, &request.bytes)
        })
        .await
    }

    pub async fn control_metadata(&self) -> Result<Option<Value>> {
        let this = self.clone();
        blocking(move || {
            let _guard = guarded(&this.0.control)?;
            read_control_at(&this.0.root)
        })
        .await
    }

    pub async fn set_control_metadata(&self, metadata: Option<Value>) -> Result<()> {
        let this = self.clone();
        blocking(move || {
            let _guard = guarded(&this.0.control)?;
            let path = this.0.root.join(CONTROL);
            match metadata {
                Some(value) => atomic_write(&path, &atri_core::encode(&value)?),
                None => remove_private(&path),
            }
        })
        .await
    }

    async fn with_plugin<T, F>(&self, id: &str, operation: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Inner, &str) -> Result<T> + Send + 'static,
    {
        validate_id(id)?;
        let this = self.clone();
        let id = id.to_owned();
        let lock = {
            let mut locks = guarded(&this.0.plugins)?;
            locks
                .entry(id.clone())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        blocking(move || {
            let _guard = guarded(&lock)?;
            operation(&this.0, &id)
        })
        .await
    }
}

/// Framework-only crash artifacts have a reserved namespace, never an extension
/// identity. The supervisor may use it while its child holds the root lock.
pub(crate) fn crash_directory(bot_dir: &Path) -> Result<PathBuf> {
    let root = absolute_root(bot_dir)?;
    private_directory(&root)?;
    let storage = root.join("storage");
    private_directory(&storage)?;
    let runtime = storage.join(".runtime");
    private_directory(&runtime)?;
    let crashes = runtime.join("crashes");
    private_directory(&crashes)?;
    Ok(crashes)
}

pub fn private_directory(path: &Path) -> Result<()> {
    ensure_directory_tree(&absolute_root(path)?, true)
}

/// Create, but never truncate or adopt, a private framework file.
pub(crate) fn create_private_file(path: &Path) -> Result<File> {
    let path = absolute_root(path)?;
    check_regular_or_missing(&path)?;
    let file = private_options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(io_error)?;
    if let Err(error) = make_private(&path, false) {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(error);
    }
    Ok(file)
}

pub(crate) fn write_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    atomic_write(&absolute_root(path)?, bytes)
}

/// Endpoint discovery intentionally does not acquire the active runtime's lock.
pub async fn read_control_metadata(bot_dir: &Path) -> Result<Option<Value>> {
    let root = absolute_root(bot_dir)?;
    blocking(move || {
        match fs::symlink_metadata(&root) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error(error)),
            Ok(metadata) if is_link(&metadata) || !metadata.is_dir() => return Err(unsafe_path()),
            Ok(_) => {}
        }
        check_ancestors(&root)?;
        read_control_at(&root)
    })
    .await
}

fn read_control_at(root: &Path) -> Result<Option<Value>> {
    read_optional(&root.join(CONTROL))?
        .map(|bytes| decode_metadata(&bytes, "control endpoint"))
        .transpose()
}

async fn blocking<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(ApiError::internal)?
}
fn guarded<T>(mutex: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>> {
    mutex
        .lock()
        .map_err(|_| ApiError::new("storage_poisoned", "a previous storage operation panicked"))
}
fn io_error(error: std::io::Error) -> ApiError {
    ApiError::new(
        if error.kind() == std::io::ErrorKind::NotFound {
            "not_found"
        } else {
            "io"
        },
        error.to_string(),
    )
}
fn unsafe_path() -> ApiError {
    ApiError::new(
        "unsafe_path",
        "symbolic links, reparse points and non-regular storage files are forbidden",
    )
}
fn ownership_error(id: &str) -> ApiError {
    ApiError::new(
        "ownership_mismatch",
        format!("private storage ownership does not match extension {id}"),
    )
}
fn invalid_config(error: impl std::fmt::Display) -> ApiError {
    ApiError::new("invalid_config", error.to_string())
}
fn check_size(bytes: &[u8]) -> Result<()> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(ApiError::new(
            "payload_too_large",
            "private document exceeds 64 MiB",
        ));
    }
    Ok(())
}
fn decode_metadata<T: serde::de::DeserializeOwned>(bytes: &[u8], name: &str) -> Result<T> {
    atri_core::decode(bytes).map_err(|error| {
        ApiError::new(
            "invalid_metadata",
            format!("malformed {name}; original file was preserved: {error}"),
        )
    })
}

fn validate_id(id: &str) -> Result<()> {
    if id.is_empty() || !id.is_ascii() {
        return Err(ApiError::new(
            "invalid_extension_id",
            "extension identity must be nonempty ASCII",
        ));
    }
    Ok(())
}
fn windows_reserved(component: &str) -> bool {
    let base = component
        .split('.')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    matches!(
        base.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$"
    ) || base
        .strip_prefix("COM")
        .or_else(|| base.strip_prefix("LPT"))
        .is_some_and(|suffix| {
            matches!(
                suffix,
                "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
}
// Compatibility mapping only, not an identity validation rule. Existing private
// extension directories retain their registered paths.
fn legacy_filesystem_id(id: &str) -> Option<String> {
    let mut bytes = id.bytes();
    if id.len() > 160
        || !bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        || id.ends_with('.')
    {
        return None;
    }
    Some(
        if id.bytes().any(|byte| byte.is_ascii_uppercase()) || windows_reserved(id) {
            format!("p-{}", hex::encode(Sha256::digest(id.as_bytes())))
        } else {
            id.to_owned()
        },
    )
}
fn allocated_directory(directory: &str) -> bool {
    directory.strip_prefix(".id-").is_some_and(|suffix| {
        suffix.len() == 16
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}
fn validate_directory(id: &str, directory: &str) -> Result<()> {
    validate_id(id)?;
    if !allocated_directory(directory) && legacy_filesystem_id(id).as_deref() != Some(directory) {
        return Err(ownership_error(id));
    }
    Ok(())
}
fn allocate_directory(id: &str, registry: &Registry) -> Result<String> {
    validate_id(id)?;
    // Preserve safe lowercase paths when free. All other identities use a
    // registry-assigned name in a disjoint namespace: no hashes, truncation,
    // case folding, or unbounded path components can alias distinct identities.
    if let Some(directory) = legacy_filesystem_id(id) {
        if directory == id && !registry.owners.values().any(|value| value == &directory) {
            return Ok(directory);
        }
    }
    let index = registry
        .owners
        .values()
        .filter(|directory| allocated_directory(directory))
        .filter_map(|directory| u64::from_str_radix(&directory[4..], 16).ok())
        .max()
        .map_or(Some(0), |index| index.checked_add(1))
        .ok_or_else(|| {
            ApiError::new(
                "storage_exhausted",
                "private storage directory identifiers exhausted",
            )
        })?;
    Ok(format!(".id-{index:016x}"))
}
fn validate_registry(registry: &Registry) -> Result<()> {
    if registry.version != 1 {
        return Err(ApiError::new(
            "invalid_metadata",
            "unsupported storage registry version",
        ));
    }
    let mut directories = BTreeSet::new();
    for (id, directory) in &registry.owners {
        validate_directory(id, directory)?;
        if !directories.insert(directory) {
            return Err(ownership_error(id));
        }
    }
    for id in &registry.disabled {
        validate_id(id)?;
    }
    Ok(())
}
fn save_registry(root: &Path, registry: &Registry) -> Result<()> {
    atomic_write(&root.join(REGISTRY), &atri_core::encode(registry)?)
}
fn recover_owners(root: &Path, registry: &mut Registry) -> Result<()> {
    let mut changed = false;
    for entry in fs::read_dir(root.join("storage")).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
        if is_link(&metadata) || !metadata.is_dir() {
            return Err(unsafe_path());
        }
        let directory = entry.file_name().into_string().map_err(|_| unsafe_path())?;
        if directory == ".runtime" {
            // Reserved host state is never assigned to an extension. Arbitrary
            // identities, including ".runtime", use the disjoint .id- namespace.
            make_private(&path, true)?;
            continue;
        }
        let owner = match read_optional(&path.join(OWNER))? {
            Some(bytes) => decode_metadata::<Owner>(&bytes, "storage owner")?,
            None => {
                let known = registry
                    .owners
                    .iter()
                    .find(|(_, value)| *value == &directory);
                if let Some((id, _)) = known {
                    ensure_owned_directory(&path, id)?;
                    continue;
                }
                return Err(ApiError::new(
                    "orphaned_storage",
                    format!("unowned directory {directory}; refusing to guess its owner"),
                ));
            }
        };
        validate_directory(&owner.id, &directory)?;
        match registry.owners.get(&owner.id) {
            Some(existing) if existing != &directory => return Err(ownership_error(&owner.id)),
            Some(_) => {}
            None => {
                // Lost registry entries have no trustworthy enable-state record.
                // Recover ownership conservatively, never start an unknown owner.
                registry.disabled.insert(owner.id.clone());
                registry.owners.insert(owner.id, directory);
                changed = true;
            }
        }
    }
    if changed {
        save_registry(root, registry)?;
    }
    Ok(())
}
fn ensure_owned_directory(root: &Path, id: &str) -> Result<()> {
    ensure_directory_tree(root, true)?;
    let owner_path = root.join(OWNER);
    match read_optional(&owner_path)? {
        Some(bytes) => {
            let owner: Owner = decode_metadata(&bytes, "storage owner")?;
            if owner.id != id {
                return Err(ownership_error(id));
            }
        }
        None => {
            cleanup_staging(root)?;
            // An ownership intent is committed to the registry before mkdir.
            // Only its still-empty directory is recoverable without a marker.
            if fs::read_dir(root).map_err(io_error)?.next().is_some() {
                return Err(ApiError::new(
                    "orphaned_storage",
                    "nonempty directory is missing its ownership marker",
                ));
            }
            atomic_write(&owner_path, &atri_core::encode(&Owner { id: id.into() })?)?;
        }
    }
    Ok(())
}
fn owned_directory(inner: &Inner, id: &str) -> Result<PathBuf> {
    let registry = guarded(&inner.registry)?;
    let directory = registry.owners.get(id).ok_or_else(|| {
        ApiError::new(
            "extension_not_found",
            "extension storage has not been prepared",
        )
    })?;
    let path = inner.root.join("storage").join(directory);
    ensure_owned_directory(&path, id)?;
    Ok(path)
}
fn config_directory(inner: &Inner, id: &str) -> Result<PathBuf> {
    let path = owned_directory(inner, id)?.join("config");
    ensure_directory_tree(&path, true)?;
    Ok(path)
}
fn area_directory(inner: &Inner, id: &str, area: &str) -> Result<PathBuf> {
    if !matches!(area, "data" | "workspace") {
        return Err(ApiError::new(
            "invalid_storage_area",
            "only data and workspace are extension-accessible",
        ));
    }
    let path = owned_directory(inner, id)?.join(area);
    ensure_directory_tree(&path, true)?;
    Ok(path)
}

fn normalized_format(format: &str) -> Result<String> {
    if format.is_empty()
        || format.len() > 64
        || !format.as_bytes()[0].is_ascii_lowercase()
        || !format
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte))
    {
        return Err(ApiError::new(
            "invalid_config_format",
            "codec name must be a lowercase portable identifier",
        ));
    }
    Ok(format.to_owned())
}
fn is_builtin(format: &str) -> bool {
    matches!(format, "toml" | "json" | "cbor")
}
fn require_builtin(format: &str) -> Result<()> {
    if !is_builtin(format) {
        return Err(ApiError::new(
            "codec_required",
            "custom configuration codecs use the opaque document API",
        ));
    }
    Ok(())
}
fn select_format(
    current: Option<&Document>,
    requested: Option<&str>,
    default_format: &str,
) -> Result<String> {
    let format = normalized_format(
        requested.unwrap_or_else(|| current.map_or(default_format, |document| &document.format)),
    )?;
    if current.is_some_and(|document| document.format != format) {
        return Err(ApiError::new(
            "config_format_mismatch",
            "use the document API to explicitly change the configuration codec",
        ));
    }
    Ok(format)
}
fn revision(format: &str, bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(format.as_bytes());
    hash.update([0]);
    hash.update(bytes);
    hex::encode(hash.finalize())
}
fn document_path(directory: &Path, format: &str) -> PathBuf {
    directory.join(format!("config.{format}"))
}
fn current_document(directory: &Path) -> Result<Option<Document>> {
    recover_config(directory)?;
    let mut result = None;
    for entry in fs::read_dir(directory).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let name = entry.file_name().into_string().map_err(|_| unsafe_path())?;
        if let Some(format) = name.strip_prefix("config.") {
            let format = normalized_format(format)?;
            if result.is_some() {
                return Err(ApiError::new(
                    "config_conflict",
                    "multiple configuration documents exist; none was overwritten",
                ));
            }
            let bytes = read_optional(&entry.path())?.ok_or_else(|| {
                ApiError::new("config_conflict", "configuration disappeared while reading")
            })?;
            result = Some(Document {
                revision: revision(&format, &bytes),
                format,
                bytes,
            });
        }
    }
    Ok(result)
}
fn check_revision(current: Option<&Document>, expected: Option<&str>) -> Result<()> {
    let actual = current.map_or(MISSING, |document| document.revision.as_str());
    if expected.is_some_and(|expected| expected != actual) {
        return Err(ApiError::new(
            "revision_conflict",
            "configuration changed; reload before committing your edit",
        ));
    }
    Ok(())
}
fn protect_malformed(current: Option<&Document>, expected: Option<&str>) -> Result<()> {
    if expected.is_none() {
        if let Some(document) = current {
            if is_builtin(&document.format) {
                decode_config(&document.format, &document.bytes)?;
            }
        }
    }
    Ok(())
}
fn public_document(document: Document) -> ConfigDocument {
    ConfigDocument {
        bytes: document.bytes,
        format: document.format,
        revision: document.revision,
    }
}
fn snapshot(document: Document) -> Result<ConfigSnapshot> {
    Ok(ConfigSnapshot {
        value: decode_config(&document.format, &document.bytes)?,
        format: document.format,
        revision: document.revision,
    })
}
fn commit_document(
    directory: &Path,
    current: Option<&Document>,
    format: String,
    bytes: Vec<u8>,
) -> Result<Document> {
    check_size(&bytes)?;
    let bytes = if current.is_some_and(|document| document.format != format) {
        let transaction = ConfigTransaction {
            previous_format: current.map(|document| document.format.clone()),
            previous_revision: current.map_or(MISSING.into(), |document| document.revision.clone()),
            format: format.clone(),
            bytes,
        };
        atomic_write(
            &directory.join(TRANSACTION),
            &atri_core::encode(&transaction)?,
        )?;
        recover_config(directory)?;
        transaction.bytes
    } else {
        atomic_write(&document_path(directory, &format), &bytes)?;
        bytes
    };
    Ok(Document {
        revision: revision(&format, &bytes),
        format,
        bytes,
    })
}
fn recover_config(directory: &Path) -> Result<()> {
    cleanup_staging(directory)?;
    let journal = directory.join(TRANSACTION);
    let Some(bytes) = read_optional(&journal)? else {
        return Ok(());
    };
    let transaction: ConfigTransaction = decode_metadata(&bytes, "configuration transaction")?;
    let format = normalized_format(&transaction.format)?;
    check_size(&transaction.bytes)?;
    if is_builtin(&format) {
        decode_config(&format, &transaction.bytes)?;
    }
    if let Some(previous) = &transaction.previous_format {
        normalized_format(previous)?;
        if previous == &format {
            return Err(ApiError::new(
                "invalid_metadata",
                "configuration transaction has identical source and destination",
            ));
        }
        if let Some(bytes) = read_optional(&document_path(directory, previous))? {
            if revision(previous, &bytes) != transaction.previous_revision {
                return Err(ApiError::new(
                    "config_conflict",
                    "configuration changed outside an interrupted transaction; all files preserved",
                ));
            }
        }
    } else if transaction.previous_revision != MISSING {
        return Err(ApiError::new(
            "invalid_metadata",
            "configuration transaction has no valid source revision",
        ));
    }
    let destination = document_path(directory, &format);
    if let Some(existing) = read_optional(&destination)? {
        if existing != transaction.bytes {
            return Err(ApiError::new(
                "config_conflict",
                "configuration transaction destination was independently modified",
            ));
        }
    } else {
        atomic_write(&destination, &transaction.bytes)?;
    }
    if let Some(previous) = &transaction.previous_format {
        remove_private(&document_path(directory, previous))?;
    }
    remove_private(&journal)
}

fn encode_config(format: &str, value: &Value) -> Result<Vec<u8>> {
    require_builtin(format)?;
    validate_maps(value)?;
    let bytes = match format {
        "toml" => toml::to_string_pretty(&to_toml(value)?)
            .map_err(invalid_config)?
            .into_bytes(),
        "json" => {
            validate_json(value)?;
            serde_json::to_vec_pretty(value).map_err(invalid_config)?
        }
        "cbor" => atri_core::encode(value).map_err(invalid_config)?,
        _ => unreachable!(),
    };
    check_size(&bytes)?;
    Ok(bytes)
}
fn decode_config(format: &str, bytes: &[u8]) -> Result<Value> {
    require_builtin(format)?;
    check_size(bytes)?;
    let value = match format {
        "toml" => {
            let text = std::str::from_utf8(bytes).map_err(invalid_config)?;
            let value: toml::Table = toml::from_str(text).map_err(invalid_config)?;
            from_toml(toml::Value::Table(value))?
        }
        "json" => serde_json::from_slice::<Value>(bytes).map_err(invalid_config)?,
        "cbor" => atri_core::decode::<Value>(bytes).map_err(invalid_config)?,
        _ => unreachable!(),
    };
    validate_maps(&value)?;
    Ok(value)
}
fn validate_maps(value: &Value) -> Result<()> {
    match value {
        Value::Map(entries) => {
            let mut text_keys = BTreeSet::new();
            for (index, (key, value)) in entries.iter().enumerate() {
                let duplicate = match key {
                    Value::Text(key) => !text_keys.insert(key.as_str()),
                    _ => entries[..index].iter().any(|(previous, _)| previous == key),
                };
                if duplicate {
                    return Err(invalid_config("duplicate configuration map key"));
                }
                validate_maps(key)?;
                validate_maps(value)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                validate_maps(value)?;
            }
        }
        Value::Tag(_, value) => validate_maps(value)?,
        _ => {}
    }
    Ok(())
}
fn validate_json(value: &Value) -> Result<()> {
    match value {
        Value::Float(value) if !value.is_finite() => {
            return Err(invalid_config("JSON cannot preserve a non-finite float"));
        }
        Value::Bytes(_) | Value::Tag(_, _) => {
            return Err(invalid_config("JSON cannot preserve CBOR bytes or tags"));
        }
        Value::Map(entries) => {
            for (key, value) in entries {
                if !matches!(key, Value::Text(_)) {
                    return Err(invalid_config("JSON object keys must be strings"));
                }
                validate_json(value)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                validate_json(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn to_toml(value: &Value) -> Result<toml::Value> {
    Ok(match value {
        Value::Text(value) => toml::Value::String(value.clone()),
        Value::Integer(value) => {
            toml::Value::Integer(i64::try_from(*value).map_err(invalid_config)?)
        }
        Value::Float(value) => toml::Value::Float(*value),
        Value::Bool(value) => toml::Value::Boolean(*value),
        Value::Array(values) => {
            toml::Value::Array(values.iter().map(to_toml).collect::<Result<_>>()?)
        }
        Value::Map(entries) => {
            if let [(Value::Text(key), Value::Text(value))] = entries.as_slice() {
                if key == DATETIME {
                    return value
                        .parse()
                        .map(toml::Value::Datetime)
                        .map_err(invalid_config);
                }
            }
            let mut table = toml::Table::new();
            for (key, value) in entries {
                let Value::Text(key) = key else {
                    return Err(invalid_config("TOML table keys must be strings"));
                };
                // serde None arrives as CBOR Null. TOML has no null: omission is
                // legal only inside maps, never an array element or root value.
                if matches!(value, Value::Null) {
                    continue;
                }
                table.insert(key.clone(), to_toml(value)?);
            }
            toml::Value::Table(table)
        }
        _ => {
            return Err(invalid_config(
                "value cannot be represented without loss in TOML",
            ));
        }
    })
}
fn from_toml(value: toml::Value) -> Result<Value> {
    Ok(match value {
        toml::Value::String(value) => Value::Text(value),
        toml::Value::Integer(value) => Value::Integer(value.into()),
        toml::Value::Float(value) => Value::Float(value),
        toml::Value::Boolean(value) => Value::Bool(value),
        toml::Value::Datetime(value) => atri_core::to_value(&value).map_err(invalid_config)?,
        toml::Value::Array(values) => {
            Value::Array(values.into_iter().map(from_toml).collect::<Result<_>>()?)
        }
        toml::Value::Table(table) => Value::Map(
            table
                .into_iter()
                .map(|(key, value)| Ok((Value::Text(key), from_toml(value)?)))
                .collect::<Result<_>>()?,
        ),
    })
}

/// Parse dotted keys without interpreting a segment as a filesystem path.
pub fn parse_key(key: &str) -> Result<Vec<String>> {
    let invalid = || {
        ApiError::new(
            "invalid_config_key",
            "expected nonempty dotted key segments, optionally quoted or backslash-escaped",
        )
    };
    if key.is_empty() || key.len() > 4096 {
        return Err(invalid());
    }
    let mut result = Vec::new();
    let mut segment = String::new();
    let mut quoted = false;
    let mut escaped = false;
    let mut closed = false;
    for character in key.chars() {
        if character.is_control() {
            return Err(invalid());
        }
        if escaped {
            segment.push(character);
            escaped = false;
            continue;
        }
        match character {
            '\\' if !closed => escaped = true,
            '"' if quoted => {
                quoted = false;
                closed = true;
            }
            '"' if segment.is_empty() && !closed => quoted = true,
            '.' if !quoted => {
                if segment.is_empty() || result.len() >= 63 {
                    return Err(invalid());
                }
                result.push(std::mem::take(&mut segment));
                closed = false;
            }
            _ if closed || (character == '"' && !quoted) => return Err(invalid()),
            _ => segment.push(character),
        }
    }
    if quoted || escaped || segment.is_empty() {
        return Err(invalid());
    }
    result.push(segment);
    Ok(result)
}
fn mutate_key(document: &mut Value, segments: &[String], value: Value) -> Result<()> {
    let Value::Map(entries) = document else {
        return Err(ApiError::new(
            "invalid_config_key",
            "dotted key crosses a non-table value",
        ));
    };
    let key = &segments[0];
    let index = entries
        .iter()
        .position(|(candidate, _)| matches!(candidate, Value::Text(candidate) if candidate == key));
    if segments.len() == 1 {
        if let Some(index) = index {
            entries[index].1 = value;
        } else {
            entries.push((Value::Text(key.clone()), value));
        }
        return Ok(());
    }
    let index = match index {
        Some(index) => index,
        None => {
            entries.push((Value::Text(key.clone()), Value::Map(Vec::new())));
            entries.len() - 1
        }
    };
    mutate_key(&mut entries[index].1, &segments[1..], value)
}

fn absolute_root(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir().map_err(io_error)?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::ParentDir => {
                return Err(ApiError::new(
                    "invalid_path",
                    "parent traversal is forbidden in a bot directory",
                ));
            }
            Component::CurDir => {}
            _ => normalized.push(component.as_os_str()),
        }
    }
    Ok(normalized)
}
fn path_string(path: &Path) -> Result<String> {
    path.to_str().map(str::to_owned).ok_or_else(|| {
        ApiError::new(
            "invalid_path",
            "extension paths require a UTF-8 bot directory",
        )
    })
}
fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
fn check_ancestors(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor).map_err(io_error)?;
        if is_link(&metadata) || !metadata.is_dir() {
            return Err(unsafe_path());
        }
    }
    Ok(())
}
fn ensure_directory_tree(path: &Path, private: bool) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if is_link(&metadata) || !metadata.is_dir() {
                return Err(unsafe_path());
            }
            check_ancestors(path)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent() {
                ensure_directory_tree(parent, false)?;
            }
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
                    if is_link(&metadata) || !metadata.is_dir() {
                        return Err(unsafe_path());
                    }
                }
                Err(error) => return Err(io_error(error)),
            }
        }
        Err(error) => return Err(io_error(error)),
    }
    if private {
        make_private(path, true)?;
    }
    Ok(())
}
fn check_regular_or_missing(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        check_ancestors(parent)?;
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_link(&metadata) || !metadata.is_file() => Err(unsafe_path()),
        #[cfg(unix)]
        Ok(metadata) => {
            use std::os::unix::fs::MetadataExt;
            if metadata.nlink() != 1 {
                return Err(unsafe_path());
            }
            Ok(())
        }
        #[cfg(not(unix))]
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(error)),
    }
}
fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options
}
fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    check_regular_or_missing(path)?;
    let mut file = match private_options().read(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error(error)),
    };
    let metadata = file.metadata().map_err(io_error)?;
    if !metadata.is_file() || is_link(&metadata) {
        return Err(unsafe_path());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.nlink() != 1 {
            return Err(unsafe_path());
        }
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(io_error)?;
    }
    if metadata.len() > MAX_BYTES {
        return Err(ApiError::new(
            "payload_too_large",
            "private file exceeds 64 MiB",
        ));
    }
    #[cfg(not(unix))]
    make_private(path, false)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    (&mut file)
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    check_size(&bytes)?;
    Ok(Some(bytes))
}
fn private_relative_path(root: &Path, path: &str, create: bool) -> Result<PathBuf> {
    if path.is_empty()
        || path.len() > 4096
        || path.contains('\\')
        || path.contains(':')
        || path.starts_with('/')
    {
        return Err(ApiError::new(
            "invalid_path",
            "storage paths must be nonempty portable relative paths",
        ));
    }
    let segments: Vec<_> = path.split('/').collect();
    let mut current = root.to_owned();
    for (index, segment) in segments.iter().enumerate() {
        if segment.is_empty()
            || matches!(*segment, "." | "..")
            || segment.ends_with(['.', ' '])
            || segment.starts_with(".atri-")
            || windows_reserved(segment)
            || segment
                .chars()
                .any(|character| character.is_control() || "<>\"|?*".contains(character))
        {
            return Err(ApiError::new(
                "invalid_path",
                "storage path contains an unsafe or reserved component",
            ));
        }
        current.push(segment);
        if index + 1 < segments.len() {
            if create {
                ensure_directory_tree(&current, true)?;
            } else {
                let metadata = fs::symlink_metadata(&current).map_err(io_error)?;
                if is_link(&metadata) || !metadata.is_dir() {
                    return Err(unsafe_path());
                }
            }
        }
    }
    check_regular_or_missing(&current)?;
    Ok(current)
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    check_size(bytes)?;
    check_regular_or_missing(path)?;
    let parent = path.parent().ok_or_else(unsafe_path)?;
    let temporary = Temporary(parent.join(format!(".atri-tmp-{}", uuid::Uuid::new_v4())));
    let mut file = private_options()
        .write(true)
        .create_new(true)
        .open(&temporary.0)
        .map_err(io_error)?;
    make_private(&temporary.0, false)?;
    file.write_all(bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    drop(file);
    check_regular_or_missing(path)?;
    replace_file(&temporary.0, path)?;
    sync_directory(parent)
}
#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> Result<()> {
    fs::rename(source, destination).map_err(io_error)
}
#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(io_error(std::io::Error::last_os_error()));
    }
    Ok(())
}
fn remove_private(path: &Path) -> Result<()> {
    check_regular_or_missing(path)?;
    match fs::remove_file(path) {
        Ok(()) => sync_directory(path.parent().ok_or_else(unsafe_path)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(error)),
    }
}
fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        File::open(path)
            .and_then(|file| file.sync_all())
            .map_err(io_error)?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}
fn make_private(path: &Path, directory: bool) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
        )
        .map_err(io_error)?;
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::{
            Foundation::LocalFree,
            Security::{
                Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW,
                DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, SetFileSecurityW,
            },
        };
        // Owner Rights is evaluated against each object's owner, unlike a fixed
        // administrator/group grant. Protect the DACL from inherited broad ACLs.
        let sddl = if directory {
            "D:P(A;OICI;FA;;;OW)"
        } else {
            "D:P(A;;FA;;;OW)"
        };
        let sddl: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
        let mut descriptor = std::ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io_error(std::io::Error::last_os_error()));
        }
        let filename: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let result = unsafe {
            SetFileSecurityW(
                filename.as_ptr(),
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                descriptor,
            )
        };
        let error = if result == 0 {
            Some(std::io::Error::last_os_error())
        } else {
            None
        };
        unsafe {
            LocalFree(descriptor);
        }
        if let Some(error) = error {
            return Err(io_error(error));
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (path, directory);
    }
    Ok(())
}

// Atomic-write staging files are never authoritative. Remove only our exact
// reserved UUID names while holding the root or owning extension's lock.
fn cleanup_staging(directory: &Path) -> Result<()> {
    for entry in fs::read_dir(directory).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let filename = entry.file_name();
        let Some(suffix) = filename
            .to_str()
            .and_then(|name| name.strip_prefix(".atri-tmp-"))
        else {
            continue;
        };
        if uuid::Uuid::parse_str(suffix).is_err() {
            continue;
        }
        remove_private(&entry.path())?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
