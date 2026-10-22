use super::*;
use atri_core::{from_value, to_value};
use tempfile::TempDir;

fn map(entries: &[(&str, Value)]) -> Value {
    Value::Map(
        entries
            .iter()
            .map(|(key, value)| (Value::Text((*key).into()), value.clone()))
            .collect(),
    )
}
fn integer(value: i64) -> Value {
    Value::Integer(value.into())
}
fn load(defaults: Option<Value>) -> ConfigLoad {
    ConfigLoad {
        defaults,
        format: None,
    }
}
fn store(value: Value, revision: Option<String>) -> ConfigStore {
    ConfigStore {
        value,
        expected_revision: revision,
        format: None,
    }
}
fn descriptor(id: &str) -> ExtensionMetadata {
    ExtensionMetadata::new(id, id, "1.0.0")
}
async fn prepared(id: &str) -> (TempDir, Storage, ExtensionMetadata, ExtensionPaths) {
    let directory = tempfile::tempdir().unwrap();
    let storage = Storage::open(directory.path().join("bot")).unwrap();
    let descriptor = descriptor(id);
    let paths = storage.prepare(&descriptor).await.unwrap();
    (directory, storage, descriptor, paths)
}
fn config_dir(paths: &ExtensionPaths) -> PathBuf {
    Path::new(&paths.root).join("config")
}
fn root_open_error(path: &Path) -> ApiError {
    match Storage::open(path.to_owned()) {
        Ok(_) => panic!("storage unexpectedly opened"),
        Err(error) => error,
    }
}

#[tokio::test]
async fn defaults_are_lazy_isolated_and_created_once_under_contention() {
    let (_directory, storage, descriptor, paths) = prepared("io.test.defaults").await;
    assert!(!config_dir(&paths).join("config.toml").exists());
    let missing = storage
        .config_document(&descriptor.id, &descriptor.configuration)
        .await
        .unwrap();
    assert_eq!(missing.revision, MISSING);
    assert!(missing.bytes.is_empty());
    assert!(!config_dir(&paths).join("config.toml").exists());
    let first = map(&[("winner", integer(1))]);
    let second = map(&[("winner", integer(2))]);
    let (left, right) = tokio::join!(
        storage.config_load(
            &descriptor.id,
            &descriptor.configuration,
            load(Some(first.clone()))
        ),
        storage.config_load(
            &descriptor.id,
            &descriptor.configuration,
            load(Some(second.clone()))
        )
    );
    let left = left.unwrap();
    let right = right.unwrap();
    assert_eq!(left.revision, right.revision);
    assert_eq!(left.value, right.value);
    assert!(left.value == first || left.value == second);
    let other = self::descriptor("io.test.other");
    let other_paths = storage.prepare(&other).await.unwrap();
    assert_ne!(paths.root, other_paths.root);
    let value = storage
        .config_load(
            &other.id,
            &other.configuration,
            load(Some(map(&[("winner", integer(3))]))),
        )
        .await
        .unwrap();
    assert_eq!(value.value, map(&[("winner", integer(3))]));
    assert_eq!(
        storage
            .config_load(&descriptor.id, &descriptor.configuration, load(None))
            .await
            .unwrap()
            .value,
        left.value
    );
}

#[tokio::test]
async fn compare_and_swap_has_one_winner_and_missing_is_a_real_revision() {
    let (_directory, storage, descriptor, _paths) = prepared("io.test.cas").await;
    let initial = storage
        .config_store(
            &descriptor.id,
            &descriptor.configuration,
            store(map(&[("value", integer(0))]), Some(MISSING.into())),
        )
        .await
        .unwrap();
    let (left, right) = tokio::join!(
        storage.config_store(
            &descriptor.id,
            &descriptor.configuration,
            store(
                map(&[("value", integer(1))]),
                Some(initial.revision.clone())
            )
        ),
        storage.config_store(
            &descriptor.id,
            &descriptor.configuration,
            store(
                map(&[("value", integer(2))]),
                Some(initial.revision.clone())
            )
        )
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    let (winner, loser) = match (left, right) {
        (Ok(winner), Err(loser)) | (Err(loser), Ok(winner)) => (winner, loser),
        _ => panic!("exactly one CAS write must succeed"),
    };
    assert_eq!(loser.code, "revision_conflict");
    let actual = storage
        .config_load(&descriptor.id, &descriptor.configuration, load(None))
        .await
        .unwrap();
    assert_eq!(actual.revision, winner.revision);
    assert_eq!(actual.value, winner.value);
    let error = storage
        .config_store(
            &descriptor.id,
            &descriptor.configuration,
            store(map(&[]), Some(MISSING.into())),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "revision_conflict");
}

#[tokio::test]
async fn toml_omits_only_map_none_and_preserves_integer_float_and_datetime() {
    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Typed {
        maximum: i64,
        fraction: f64,
        optional: Option<String>,
        nested: BTreeMap<String, Option<i64>>,
        when: toml::value::Datetime,
    }
    let (_directory, storage, descriptor, paths) = prepared("io.test.types").await;
    let expected = Typed {
        maximum: i64::MAX,
        fraction: 1.2345678901234567,
        optional: None,
        nested: BTreeMap::from([("present".into(), Some(i64::MIN)), ("absent".into(), None)]),
        when: "2026-10-20T12:34:56.123456789+08:00".parse().unwrap(),
    };
    let snapshot = storage
        .config_load(
            &descriptor.id,
            &descriptor.configuration,
            load(Some(to_value(&expected).unwrap())),
        )
        .await
        .unwrap();
    let mut canonical = expected;
    canonical.nested.remove("absent");
    assert_eq!(from_value::<Typed>(&snapshot.value).unwrap(), canonical);
    let bytes = fs::read(config_dir(&paths).join("config.toml")).unwrap();
    let parsed: toml::Table = toml::from_str(std::str::from_utf8(&bytes).unwrap()).unwrap();
    assert_eq!(parsed["maximum"].as_integer(), Some(i64::MAX));
    assert_eq!(parsed["fraction"].as_float(), Some(canonical.fraction));
    assert!(parsed["when"].is_datetime());
    assert!(!parsed.contains_key("optional"));
    assert!(!parsed["nested"].as_table().unwrap().contains_key("absent"));
    let previous = bytes;
    let error = storage
        .config_store(
            &descriptor.id,
            &descriptor.configuration,
            store(map(&[("array", Value::Array(vec![Value::Null]))]), None),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "invalid_config");
    assert_eq!(
        fs::read(config_dir(&paths).join("config.toml")).unwrap(),
        previous
    );
}

#[tokio::test]
async fn malformed_files_are_preserved_until_an_explicit_cas_repair() {
    let (_directory, storage, descriptor, paths) = prepared("io.test.malformed").await;
    let file = config_dir(&paths).join("config.toml");
    let malformed = b"[broken\nsecret = '\xff'";
    fs::write(&file, malformed).unwrap();
    let error = storage
        .config_load(
            &descriptor.id,
            &descriptor.configuration,
            load(Some(map(&[("default", integer(7))]))),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "invalid_config");
    let error = storage
        .config_store(
            &descriptor.id,
            &descriptor.configuration,
            store(map(&[]), None),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "invalid_config");
    assert_eq!(fs::read(&file).unwrap(), malformed);
    let original = storage
        .config_document(&descriptor.id, &descriptor.configuration)
        .await
        .unwrap();
    assert_eq!(original.bytes, malformed);
    let error = storage
        .config_document_store(
            &descriptor.id,
            &descriptor.configuration,
            ConfigDocumentStore {
                bytes: b"healthy = true\n".to_vec(),
                format: "toml".into(),
                expected_revision: None,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "invalid_config");
    assert_eq!(fs::read(&file).unwrap(), malformed);
    storage
        .config_document_store(
            &descriptor.id,
            &descriptor.configuration,
            ConfigDocumentStore {
                bytes: b"healthy = true\n".to_vec(),
                format: "toml".into(),
                expected_revision: Some(original.revision),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        storage
            .config_load(&descriptor.id, &descriptor.configuration, load(None))
            .await
            .unwrap()
            .value,
        map(&[("healthy", Value::Bool(true))])
    );
}

#[tokio::test]
async fn custom_codecs_are_opaque_and_never_receive_automatic_defaults() {
    let (_directory, storage, mut descriptor, paths) = prepared("io.test.codec").await;
    descriptor.configuration.format = "application_yaml".into();
    let error = storage
        .config_load(&descriptor.id, &descriptor.configuration, load(None))
        .await
        .unwrap_err();
    assert_eq!(error.code, "codec_required");
    assert_eq!(fs::read_dir(config_dir(&paths)).unwrap().count(), 0);
    let original = vec![0, 255, 1, 0, b'\n'];
    let saved = storage
        .config_document_store(
            &descriptor.id,
            &descriptor.configuration,
            ConfigDocumentStore {
                bytes: original.clone(),
                format: descriptor.configuration.format.clone(),
                expected_revision: Some(MISSING.into()),
            },
        )
        .await
        .unwrap();
    let loaded = storage
        .config_document(&descriptor.id, &descriptor.configuration)
        .await
        .unwrap();
    assert_eq!(loaded.bytes, original);
    assert_eq!(loaded.revision, saved.revision);
    let error = storage
        .config_document_store(
            &descriptor.id,
            &descriptor.configuration,
            ConfigDocumentStore {
                bytes: b"stale".to_vec(),
                format: descriptor.configuration.format.clone(),
                expected_revision: Some(MISSING.into()),
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "revision_conflict");
    assert_eq!(
        storage
            .config_document(&descriptor.id, &descriptor.configuration)
            .await
            .unwrap()
            .bytes,
        original
    );
    assert_eq!(
        fs::read(config_dir(&paths).join("config.application_yaml")).unwrap(),
        original
    );
}

#[tokio::test]
async fn json_and_cbor_preserve_values_and_reject_lossy_conversion() {
    for format in ["json", "cbor"] {
        let (_directory, storage, mut descriptor, paths) = prepared("io.test.alternative").await;
        descriptor.configuration.format = format.into();
        let value = map(&[
            ("large", integer(i64::MAX)),
            ("fraction", Value::Float(1.0000000000000002)),
            ("none", Value::Null),
        ]);
        let saved = storage
            .config_store(
                &descriptor.id,
                &descriptor.configuration,
                store(value.clone(), None),
            )
            .await
            .unwrap();
        assert_eq!(saved.value, value);
        assert!(
            config_dir(&paths)
                .join(format!("config.{format}"))
                .is_file()
        );
        let duplicate = Value::Map(vec![
            (Value::Text("a".into()), integer(1)),
            (Value::Text("a".into()), integer(2)),
        ]);
        let error = storage
            .config_store(
                &descriptor.id,
                &descriptor.configuration,
                store(duplicate, None),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, "invalid_config");
        assert_eq!(
            storage
                .config_load(&descriptor.id, &descriptor.configuration, load(None))
                .await
                .unwrap()
                .value,
            value
        );
        if format == "json" {
            let error = storage
                .config_store(
                    &descriptor.id,
                    &descriptor.configuration,
                    store(map(&[("nan", Value::Float(f64::NAN))]), None),
                )
                .await
                .unwrap_err();
            assert_eq!(error.code, "invalid_config");
        } else {
            let bytes = map(&[("bytes", Value::Bytes(vec![0, 1, 255]))]);
            assert_eq!(
                storage
                    .config_store(
                        &descriptor.id,
                        &descriptor.configuration,
                        store(bytes.clone(), None)
                    )
                    .await
                    .unwrap()
                    .value,
                bytes
            );
        }
    }
}

#[tokio::test]
async fn dotted_mutation_and_import_are_atomic_and_do_not_discard_scalars() {
    let (_directory, storage, descriptor, _paths) = prepared("io.test.keys").await;
    let first = storage
        .config_store(
            &descriptor.id,
            &descriptor.configuration,
            store(map(&[("keep", integer(5)), ("scalar", integer(1))]), None),
        )
        .await
        .unwrap();
    let changed = storage
        .config_set_key(
            &descriptor.id,
            &descriptor.configuration,
            "account.\"literal.key\"",
            integer(i64::MAX),
            Some(first.revision),
        )
        .await
        .unwrap();
    assert_eq!(
        changed.value,
        map(&[
            ("account", map(&[("literal.key", integer(i64::MAX))])),
            ("keep", integer(5)),
            ("scalar", integer(1))
        ])
    );
    let error = storage
        .config_set_key(
            &descriptor.id,
            &descriptor.configuration,
            "scalar.child",
            integer(2),
            Some(changed.revision.clone()),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "invalid_config_key");
    assert_eq!(
        storage
            .config_load(&descriptor.id, &descriptor.configuration, load(None))
            .await
            .unwrap()
            .revision,
        changed.revision
    );
    let imported = storage
        .config_import(
            &descriptor.id,
            &descriptor.configuration,
            ConfigDocumentStore {
                bytes: b"{\"whole\":9223372036854775807,\"fraction\":1.2345678901234567}".to_vec(),
                format: "json".into(),
                expected_revision: Some(changed.revision),
            },
        )
        .await
        .unwrap();
    assert_eq!(imported.format, "toml");
    assert_eq!(
        imported.value,
        map(&[
            ("fraction", Value::Float(1.2345678901234567)),
            ("whole", integer(i64::MAX))
        ])
    );
}

#[test]
fn dotted_key_parser_rejects_ambiguous_and_unterminated_paths() {
    assert_eq!(
        parse_key(r#"alpha."literal.dot".escaped\.dot"#).unwrap(),
        vec!["alpha", "literal.dot", "escaped.dot"]
    );
    for invalid in [
        "",
        ".a",
        "a.",
        "a..b",
        "a.\"\"",
        "\"unfinished",
        "a\\",
        "\"a\"suffix",
        "a\n.b",
    ] {
        assert_eq!(
            parse_key(invalid).unwrap_err().code,
            "invalid_config_key",
            "{invalid:?}"
        );
    }
}

#[tokio::test]
async fn storage_paths_are_contained_portable_and_configuration_is_inaccessible() {
    let (_directory, storage, descriptor, paths) = prepared("io.test.paths").await;
    for path in [
        "",
        "/absolute",
        "../outside",
        "a/../outside",
        "a//b",
        "./a",
        "a\\b",
        "C:stream",
        "con",
        "NUL.txt",
        "COM1.log",
        "trailing.",
        "trailing ",
        ".atri-owner.cbor",
        "a/\0b",
    ] {
        let error = storage
            .write(
                &descriptor.id,
                StorageWrite {
                    area: "data".into(),
                    path: path.into(),
                    bytes: vec![1],
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, "invalid_path", "{path:?}");
    }
    let error = storage
        .write(
            &descriptor.id,
            StorageWrite {
                area: "config".into(),
                path: "config.toml".into(),
                bytes: vec![],
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "invalid_storage_area");
    storage
        .write(
            &descriptor.id,
            StorageWrite {
                area: "workspace".into(),
                path: "nested/file.bin".into(),
                bytes: vec![0, 255],
            },
        )
        .await
        .unwrap();
    assert_eq!(
        storage
            .read(
                &descriptor.id,
                StorageRead {
                    area: "workspace".into(),
                    path: "nested/file.bin".into()
                }
            )
            .await
            .unwrap(),
        vec![0, 255]
    );
    assert_eq!(
        fs::read(Path::new(&paths.workspace).join("nested/file.bin")).unwrap(),
        vec![0, 255]
    );
    let error = storage
        .read(
            &descriptor.id,
            StorageRead {
                area: "data".into(),
                path: "missing/child".into(),
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "not_found");
    assert!(Path::new(&paths.workspace).is_absolute());
}

#[tokio::test]
async fn arbitrary_ascii_identities_have_distinct_portable_persistent_paths() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("bot");
    let storage = Storage::open(root.clone()).unwrap();
    let mut ids: Vec<String> = [
        "abc",
        "io.test.case",
        "Io.Test.Case",
        "CON.txt",
        ".",
        "..",
        "../outside",
        "a/b",
        "a\\b",
        "x:stream",
        "trailing.",
        "white space",
        ".runtime",
        ".id-0000000000000000",
        "9f84870e-a135-4d1e-bbb6-934395abfe00",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    ids.push((0u8..=127).map(char::from).collect());
    ids.push("x".repeat(4096));
    let mut paths = BTreeMap::new();
    let mut folded = BTreeSet::new();
    for id in &ids {
        let paths_for_id = storage.prepare(&descriptor(id)).await.unwrap();
        assert!(
            folded.insert(paths_for_id.root.to_ascii_lowercase()),
            "{id:?}"
        );
        let path = Path::new(&paths_for_id.root);
        assert_eq!(path.parent().unwrap(), root.join("storage"));
        let component = path.file_name().unwrap().to_str().unwrap();
        assert!(component.len() <= 160);
        assert!(!windows_reserved(component));
        assert!(!component.ends_with('.'));
        assert!(component.bytes().all(|byte| byte.is_ascii_lowercase()
            || byte.is_ascii_digit()
            || b"._-".contains(&byte)));
        let owner: Owner = atri_core::decode(&fs::read(path.join(OWNER)).unwrap()).unwrap();
        assert_eq!(&owner.id, id);
        paths.insert(id.clone(), paths_for_id.root);
    }
    for id in ["", "é", "emoji-🦀"] {
        assert_eq!(
            storage.prepare(&descriptor(id)).await.unwrap_err().code,
            "invalid_extension_id",
            "{id:?}"
        );
    }
    storage.set_enabled("../outside", &false).await.unwrap();
    drop(storage);
    let reopened = Storage::open(root).unwrap();
    assert!(reopened.disabled().await.unwrap().contains("../outside"));
    for id in ids {
        assert_eq!(
            reopened.prepare(&descriptor(&id)).await.unwrap().root,
            paths[&id]
        );
    }
}

#[tokio::test]
async fn legacy_hashed_ownership_is_preserved_without_claiming_another_identity() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("bot");
    let storage = Storage::open(root.clone()).unwrap();
    let id = "Io.Test.Legacy";
    let legacy = legacy_filesystem_id(id).unwrap();
    {
        let mut registry = guarded(&storage.0.registry).unwrap();
        registry.owners.insert(id.into(), legacy.clone());
        save_registry(&root, &registry).unwrap();
    }
    let existing = root.join("storage").join(&legacy);
    ensure_owned_directory(&existing, id).unwrap();
    fs::write(existing.join("machine_id"), b"existing-device").unwrap();
    fs::write(existing.join("keystore"), b"existing-secret").unwrap();
    drop(storage);
    let reopened = Storage::open(root).unwrap();
    let old = reopened.prepare(&descriptor(id)).await.unwrap();
    assert_eq!(Path::new(&old.root), existing);
    assert_eq!(
        fs::read(existing.join("machine_id")).unwrap(),
        b"existing-device"
    );
    assert_eq!(
        fs::read(existing.join("keystore")).unwrap(),
        b"existing-secret"
    );
    // The old hash is itself a legal, distinct ID. It must not alias its owner.
    let collision = reopened.prepare(&descriptor(&legacy)).await.unwrap();
    assert_ne!(
        old.root.to_ascii_lowercase(),
        collision.root.to_ascii_lowercase()
    );
    assert!(allocated_directory(
        Path::new(&collision.root)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn private_permissions_symlinks_and_hardlinks_are_enforced() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let (directory, storage, descriptor, paths) = prepared("io.test.permissions").await;
    storage
        .config_load(&descriptor.id, &descriptor.configuration, load(None))
        .await
        .unwrap();
    storage
        .write(
            &descriptor.id,
            StorageWrite {
                area: "data".into(),
                path: "nested/private".into(),
                bytes: b"inside".to_vec(),
            },
        )
        .await
        .unwrap();
    for path in [
        storage.root().to_owned(),
        PathBuf::from(&paths.root),
        config_dir(&paths),
        PathBuf::from(&paths.data),
        Path::new(&paths.data).join("nested"),
        PathBuf::from(&paths.workspace),
    ] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    for path in [
        storage.root().join(REGISTRY),
        storage.root().join(".atri.lock"),
        Path::new(&paths.root).join(OWNER),
        config_dir(&paths).join("config.toml"),
        Path::new(&paths.data).join("nested/private"),
    ] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let outside = directory.path().join("outside");
    fs::write(&outside, b"outside").unwrap();
    let link = Path::new(&paths.data).join("link");
    symlink(&outside, &link).unwrap();
    assert_eq!(
        storage
            .read(
                &descriptor.id,
                StorageRead {
                    area: "data".into(),
                    path: "link".into()
                }
            )
            .await
            .unwrap_err()
            .code,
        "unsafe_path"
    );
    assert_eq!(
        storage
            .write(
                &descriptor.id,
                StorageWrite {
                    area: "data".into(),
                    path: "link".into(),
                    bytes: b"changed".to_vec()
                }
            )
            .await
            .unwrap_err()
            .code,
        "unsafe_path"
    );
    let hardlink = Path::new(&paths.data).join("hardlink");
    fs::hard_link(&outside, &hardlink).unwrap();
    assert_eq!(
        storage
            .read(
                &descriptor.id,
                StorageRead {
                    area: "data".into(),
                    path: "hardlink".into()
                }
            )
            .await
            .unwrap_err()
            .code,
        "unsafe_path"
    );
    let outside_dir = directory.path().join("outside_dir");
    fs::create_dir(&outside_dir).unwrap();
    symlink(&outside_dir, Path::new(&paths.workspace).join("linked")).unwrap();
    assert_eq!(
        storage
            .write(
                &descriptor.id,
                StorageWrite {
                    area: "workspace".into(),
                    path: "linked/escape".into(),
                    bytes: vec![1]
                }
            )
            .await
            .unwrap_err()
            .code,
        "unsafe_path"
    );
    assert_eq!(fs::read(&outside).unwrap(), b"outside");
    assert!(!outside_dir.join("escape").exists());
    let bot_link = directory.path().join("bot_link");
    symlink(storage.root(), &bot_link).unwrap();
    assert_eq!(root_open_error(&bot_link).code, "unsafe_path");
    let config = config_dir(&paths).join("config.toml");
    fs::remove_file(&config).unwrap();
    symlink(&outside, &config).unwrap();
    assert_eq!(
        storage
            .config_load(&descriptor.id, &descriptor.configuration, load(None))
            .await
            .unwrap_err()
            .code,
        "unsafe_path"
    );
    assert_eq!(
        storage
            .config_store(
                &descriptor.id,
                &descriptor.configuration,
                store(map(&[]), None)
            )
            .await
            .unwrap_err()
            .code,
        "unsafe_path"
    );
    assert_eq!(fs::read(&outside).unwrap(), b"outside");
}

#[tokio::test]
async fn root_lock_is_held_by_every_clone_and_enable_state_survives_restart() {
    let (_directory, storage, descriptor, _paths) = prepared("io.test.registry").await;
    let root = storage.root().to_owned();
    let clone = storage.clone();
    assert_eq!(root_open_error(&root).code, "runtime_locked");
    let (first, second) = tokio::join!(
        storage.set_enabled(&descriptor.id, &false),
        storage.set_enabled("io.test.other", &false)
    );
    first.unwrap();
    second.unwrap();
    assert_eq!(
        storage.disabled().await.unwrap(),
        BTreeSet::from([descriptor.id.clone(), "io.test.other".into()])
    );
    drop(storage);
    assert_eq!(root_open_error(&root).code, "runtime_locked");
    drop(clone);
    let reopened = Storage::open(root).unwrap();
    assert_eq!(
        reopened.disabled().await.unwrap(),
        BTreeSet::from([descriptor.id.clone(), "io.test.other".into()])
    );
    reopened.set_enabled(&descriptor.id, &true).await.unwrap();
    assert_eq!(
        reopened.disabled().await.unwrap(),
        BTreeSet::from(["io.test.other".into()])
    );
}

#[test]
fn duplicated_descriptors_do_not_extend_the_storage_writer_lease() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("bot");
    let storage = Storage::open(root.clone()).unwrap();
    let clone = storage.clone();
    // dup and fork inherit the same Unix open-file description. Keep that extra
    // reference alive without requiring a fork from the multithreaded test runner.
    let inherited = storage.0._process_lock.file.try_clone().unwrap();
    drop(storage);
    assert_eq!(root_open_error(&root).code, "runtime_locked");
    drop(clone);
    let reopened = Storage::open(root.clone()).unwrap();
    assert_eq!(root_open_error(&root).code, "runtime_locked");
    drop(inherited);
    // Closing the old descriptor must not release the newly acquired writer.
    assert_eq!(root_open_error(&root).code, "runtime_locked");
    drop(reopened);
    Storage::open(root).unwrap();
}

#[tokio::test]
async fn malformed_registry_and_unowned_directories_are_not_overwritten() {
    let (_directory, storage, descriptor, paths) = prepared("io.test.recovery").await;
    let root = storage.root().to_owned();
    drop(storage);
    let registry_path = root.join(REGISTRY);
    let valid_registry = fs::read(&registry_path).unwrap();
    fs::write(&registry_path, b"not cbor").unwrap();
    assert_eq!(root_open_error(&root).code, "invalid_metadata");
    assert_eq!(fs::read(&registry_path).unwrap(), b"not cbor");
    fs::write(&registry_path, valid_registry).unwrap();
    fs::create_dir(root.join("storage").join("unknown")).unwrap();
    assert_eq!(root_open_error(&root).code, "orphaned_storage");
    fs::remove_dir(root.join("storage").join("unknown")).unwrap();
    fs::write(
        Path::new(&paths.root).join(OWNER),
        atri_core::encode(&Owner {
            id: "io.test.somebody-else".into(),
        })
        .unwrap(),
    )
    .unwrap();
    assert_eq!(root_open_error(&root).code, "ownership_mismatch");
    assert_eq!(descriptor.id, "io.test.recovery");
}

#[tokio::test]
async fn missing_registry_recovers_proven_owners_disabled_not_unowned_data() {
    let (_directory, storage, descriptor, paths) = prepared("io.test.lost-registry").await;
    let root = storage.root().to_owned();
    storage
        .write(
            &descriptor.id,
            StorageWrite {
                area: "data".into(),
                path: "state".into(),
                bytes: vec![9],
            },
        )
        .await
        .unwrap();
    drop(storage);
    fs::remove_file(root.join(REGISTRY)).unwrap();
    let storage = Storage::open(root).unwrap();
    assert!(storage.disabled().await.unwrap().contains(&descriptor.id));
    assert_eq!(storage.prepare(&descriptor).await.unwrap().root, paths.root);
    assert_eq!(
        storage
            .read(
                &descriptor.id,
                StorageRead {
                    area: "data".into(),
                    path: "state".into()
                }
            )
            .await
            .unwrap(),
        vec![9]
    );
}

#[tokio::test]
async fn interrupted_ownership_intent_and_marker_staging_recover() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("bot");
    let storage = Storage::open(root.clone()).unwrap();
    let descriptor = descriptor("io.test.intent");
    let dirname = legacy_filesystem_id(&descriptor.id).unwrap();
    {
        let mut registry = guarded(&storage.0.registry).unwrap();
        registry
            .owners
            .insert(descriptor.id.clone(), dirname.clone());
        save_registry(&root, &registry).unwrap();
    }
    let plugin_root = root.join("storage").join(dirname);
    fs::create_dir(&plugin_root).unwrap();
    fs::write(
        plugin_root.join(format!(".atri-tmp-{}", uuid::Uuid::new_v4())),
        b"partial owner marker",
    )
    .unwrap();
    drop(storage);
    let reopened = Storage::open(root).unwrap();
    let paths = reopened.prepare(&descriptor).await.unwrap();
    assert_eq!(Path::new(&paths.root), plugin_root);
    let owner: Owner = atri_core::decode(&fs::read(plugin_root.join(OWNER)).unwrap()).unwrap();
    assert_eq!(owner.id, descriptor.id);
    assert!(Path::new(&paths.data).is_dir());
    assert_eq!(fs::read_dir(plugin_root).unwrap().count(), 4);
}

#[tokio::test]
async fn codec_switch_recovers_every_durable_transaction_boundary() {
    for stage in 0..3 {
        let (_directory, storage, descriptor, paths) = prepared("io.test.transaction").await;
        let old = storage
            .config_load(
                &descriptor.id,
                &descriptor.configuration,
                load(Some(map(&[("old", Value::Bool(true))]))),
            )
            .await
            .unwrap();
        storage.set_enabled(&descriptor.id, &false).await.unwrap();
        let directory = config_dir(&paths);
        let new_bytes = b"{\"new\":9223372036854775807}".to_vec();
        let transaction = ConfigTransaction {
            previous_format: Some("toml".into()),
            previous_revision: old.revision,
            format: "json".into(),
            bytes: new_bytes.clone(),
        };
        atomic_write(
            &directory.join(TRANSACTION),
            &atri_core::encode(&transaction).unwrap(),
        )
        .unwrap();
        if stage >= 1 {
            atomic_write(&directory.join("config.json"), &new_bytes).unwrap();
        }
        if stage >= 2 {
            remove_private(&directory.join("config.toml")).unwrap();
        }
        let root = storage.root().to_owned();
        drop(storage);
        let storage = Storage::open(root).unwrap();
        storage.prepare(&descriptor).await.unwrap();
        let document = storage
            .config_document(&descriptor.id, &descriptor.configuration)
            .await
            .unwrap();
        assert_eq!(document.format, "json");
        assert_eq!(document.bytes, new_bytes);
        assert!(!directory.join("config.toml").exists());
        assert!(!directory.join(TRANSACTION).exists());
        assert!(storage.disabled().await.unwrap().contains(&descriptor.id));
        assert_eq!(
            storage
                .config_load(&descriptor.id, &descriptor.configuration, load(None))
                .await
                .unwrap()
                .value,
            map(&[("new", integer(i64::MAX))])
        );
    }
}

#[tokio::test]
async fn conflicting_interrupted_metadata_preserves_both_documents_and_journal() {
    let (_directory, storage, descriptor, paths) = prepared("io.test.conflict").await;
    let old = storage
        .config_load(&descriptor.id, &descriptor.configuration, load(None))
        .await
        .unwrap();
    let directory = config_dir(&paths);
    let transaction = ConfigTransaction {
        previous_format: Some("toml".into()),
        previous_revision: old.revision,
        format: "json".into(),
        bytes: b"{}".to_vec(),
    };
    let journal = atri_core::encode(&transaction).unwrap();
    atomic_write(&directory.join(TRANSACTION), &journal).unwrap();
    atomic_write(&directory.join("config.json"), b"{\"external\":true}").unwrap();
    let error = storage
        .config_document(&descriptor.id, &descriptor.configuration)
        .await
        .unwrap_err();
    assert_eq!(error.code, "config_conflict");
    assert_eq!(fs::read(directory.join(TRANSACTION)).unwrap(), journal);
    assert_eq!(
        fs::read(directory.join("config.json")).unwrap(),
        b"{\"external\":true}"
    );
    assert!(directory.join("config.toml").exists());
}

#[tokio::test]
async fn endpoint_metadata_is_private_atomic_and_discoverable_while_locked() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("bot");
    assert!(read_control_metadata(&root).await.unwrap().is_none());
    let storage = Storage::open(root.clone()).unwrap();
    let endpoint = map(&[
        ("address", Value::Text("127.0.0.1:12345".into())),
        ("token", Value::Text("private-token".into())),
        ("session", Value::Text("private-session".into())),
    ]);
    storage
        .set_control_metadata(Some(endpoint.clone()))
        .await
        .unwrap();
    assert_eq!(
        read_control_metadata(&root).await.unwrap(),
        Some(endpoint.clone())
    );
    assert_eq!(storage.control_metadata().await.unwrap(), Some(endpoint));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(root.join(CONTROL))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    storage.set_control_metadata(None).await.unwrap();
    assert!(read_control_metadata(&root).await.unwrap().is_none());
    fs::write(root.join(CONTROL), b"broken endpoint").unwrap();
    assert_eq!(
        read_control_metadata(&root).await.unwrap_err().code,
        "invalid_metadata"
    );
    assert_eq!(fs::read(root.join(CONTROL)).unwrap(), b"broken endpoint");
}

#[tokio::test]
async fn concurrent_private_file_writes_never_interleave_and_key_edits_do_not_lose_updates() {
    let (_directory, storage, descriptor, _paths) = prepared("io.test.concurrent").await;
    let left = vec![0x55; 128 * 1024];
    let right = vec![0xaa; 128 * 1024];
    let (first, second) = tokio::join!(
        storage.write(
            &descriptor.id,
            StorageWrite {
                area: "data".into(),
                path: "state.bin".into(),
                bytes: left.clone()
            }
        ),
        storage.write(
            &descriptor.id,
            StorageWrite {
                area: "data".into(),
                path: "state.bin".into(),
                bytes: right.clone()
            }
        )
    );
    first.unwrap();
    second.unwrap();
    let actual = storage
        .read(
            &descriptor.id,
            StorageRead {
                area: "data".into(),
                path: "state.bin".into(),
            },
        )
        .await
        .unwrap();
    assert!(actual == left || actual == right);
    let (first, second) = tokio::join!(
        storage.config_set_key(
            &descriptor.id,
            &descriptor.configuration,
            "left",
            integer(1),
            None
        ),
        storage.config_set_key(
            &descriptor.id,
            &descriptor.configuration,
            "right",
            integer(2),
            None
        )
    );
    first.unwrap();
    second.unwrap();
    let actual = storage
        .config_load(&descriptor.id, &descriptor.configuration, load(None))
        .await
        .unwrap();
    assert_eq!(
        actual.value,
        map(&[("left", integer(1)), ("right", integer(2))])
    );
}

#[tokio::test]
async fn crash_namespace_is_private_and_never_claimed_as_an_extension() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("bot");
    let crashes = crash_directory(&root).unwrap();
    assert_eq!(crashes, root.join("storage/.runtime/crashes"));
    let report = crashes.join("report.txt");
    let mut file = create_private_file(&report).unwrap();
    file.write_all(b"first report").unwrap();
    file.sync_all().unwrap();
    drop(file);
    assert!(create_private_file(&report).is_err());
    assert_eq!(fs::read(&report).unwrap(), b"first report");
    write_private_file(&report, b"complete replacement").unwrap();
    assert_eq!(fs::read(&report).unwrap(), b"complete replacement");
    let storage = Storage::open(root.clone()).unwrap();
    assert!(guarded(&storage.0.registry).unwrap().owners.is_empty());
    assert_eq!(crash_directory(&root).unwrap(), crashes);
    let descriptor = descriptor("io.test.after-crash");
    storage.prepare(&descriptor).await.unwrap();
    assert_eq!(guarded(&storage.0.registry).unwrap().owners.len(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        for path in [root.join("storage/.runtime"), crashes.clone()] {
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        assert_eq!(
            fs::metadata(&report).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let outside = directory.path().join("outside");
        fs::write(&outside, b"not a report").unwrap();
        let linked = crashes.join("linked.txt");
        symlink(&outside, &linked).unwrap();
        assert_eq!(
            write_private_file(&linked, b"bad").unwrap_err().code,
            "unsafe_path"
        );
        assert!(create_private_file(&linked).is_err());
        assert_eq!(fs::read(outside).unwrap(), b"not a report");
    }
    drop(storage);
    let storage = Storage::open(root).unwrap();
    assert_eq!(guarded(&storage.0.registry).unwrap().owners.len(), 1);
}
