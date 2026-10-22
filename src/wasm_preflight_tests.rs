use super::*;
use std::time::Duration;

const CORE: &str = r#"(module
    (memory (export "memory") 1)
    (func (export "atri_abi_version") (result i32) i32.const 1)
    (func (export "atri_alloc") (param i32) (result i32) i32.const 0)
    (func (export "atri_free") (param i32 i32))
    (func (export "atri_invoke") (param i32 i32 i32) (result i64) i64.const 0))"#;

fn binary(wat: &str) -> Vec<u8> {
    wat::parse_str(wat).unwrap()
}
fn check(wat: &str) -> Result<Backend, ApiError> {
    inspect(&binary(wat))
}
fn leb(mut value: usize, bytes: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        bytes.push(byte | if value == 0 { 0 } else { 0x80 });
        if value == 0 {
            break;
        }
    }
}
fn custom(bytes: &mut Vec<u8>, name: &str, data: &[u8]) {
    let mut section = Vec::new();
    leb(name.len(), &mut section);
    section.extend_from_slice(name.as_bytes());
    section.extend_from_slice(data);
    bytes.push(0);
    leb(section.len(), bytes);
    bytes.extend_from_slice(&section);
}
fn component(body: &str) -> String {
    format!(
        r#"(component {body} (instance $plugin) (export "atri:extension/plugin@1.0.0" (instance $plugin)))"#
    )
}

#[test]
fn explicit_core_abi_accepts_all_supported_imports_and_imported_function_indices() {
    let imports = r#"
        (import "atri_v1" "call" (func (param i32 i32) (result i32)))
        (import "atri_v1" "response_len" (func (param i32) (result i32)))
        (import "atri_v1" "response_read" (func (param i32 i32 i32) (result i32)))
        (import "atri_v1" "response_drop" (func (param i32)))"#;
    assert_eq!(check(CORE).unwrap(), Backend::Core);
    assert_eq!(
        check(&CORE.replacen("(module", &format!("(module {imports}"), 1)).unwrap(),
        Backend::Core
    );
    // Optional custom metadata does not override a complete explicit core ABI.
    let mut bytes = binary(CORE);
    custom(&mut bytes, "component-type:unrelated", b"metadata");
    assert_eq!(inspect(&bytes).unwrap(), Backend::Core);
}

#[test]
fn every_mandatory_core_export_is_checked_before_compilation() {
    for name in [
        "memory",
        "atri_abi_version",
        "atri_alloc",
        "atri_free",
        "atri_invoke",
    ] {
        let missing = CORE.replace(&format!("(export \"{name}\")"), "");
        assert_eq!(check(&missing).unwrap_err().code, "wasm_core_abi", "{name}");
        let wrong_kind = format!(
            "{} (global (export \"{name}\") i32 (i32.const 0)))",
            missing.strip_suffix(')').unwrap()
        );
        assert_eq!(
            check(&wrong_kind).unwrap_err().code,
            "wasm_core_abi",
            "{name}"
        );
    }
}

#[test]
fn every_core_function_signature_and_memory_model_is_checked() {
    for (old, new) in [
        ("(result i32) i32.const 1", "(result i64) i64.const 1"),
        (
            "(param i32) (result i32) i32.const 0",
            "(param i64) (result i32) i32.const 0",
        ),
        ("(param i32 i32))", "(param i32))"),
        (
            "(param i32 i32 i32) (result i64) i64.const 0",
            "(param i32 i32 i32) (result i32) i32.const 0",
        ),
        (
            "(memory (export \"memory\") 1)",
            "(memory (export \"memory\") i64 1)",
        ),
        (
            "(memory (export \"memory\") 1)",
            "(memory (export \"memory\") 1 2 shared)",
        ),
    ] {
        assert_eq!(
            check(&CORE.replace(old, new)).unwrap_err().code,
            "wasm_core_abi"
        );
    }
}

#[test]
fn core_import_names_kinds_and_signatures_are_not_deferred_to_linking() {
    for (import, code) in [
        (
            r#"(import "env" "call" (func (param i32 i32) (result i32)))"#,
            "wasm_imports",
        ),
        (r#"(import "atri_v1" "other" (func))"#, "wasm_imports"),
        (r#"(import "atri_v1" "call" (memory 1))"#, "wasm_core_abi"),
        (
            r#"(import "atri_v1" "call" (func (param i32 i64) (result i32)))"#,
            "wasm_core_abi",
        ),
        (
            r#"(import "atri_v1" "response_len" (func (param i32) (result i64)))"#,
            "wasm_core_abi",
        ),
        (
            r#"(import "atri_v1" "response_read" (func (param i32 i32) (result i32)))"#,
            "wasm_core_abi",
        ),
        (
            r#"(import "atri_v1" "response_drop" (func (param i32) (result i32)))"#,
            "wasm_core_abi",
        ),
    ] {
        let source = CORE.replacen("(module", &format!("(module {import}"), 1);
        assert_eq!(check(&source).unwrap_err().code, code);
    }
}

#[test]
fn wasi_imports_are_rejected_in_core_even_with_a_complete_core_abi() {
    for namespace in [
        "wasi_snapshot_preview1",
        "wasi_unstable",
        "wasi:clocks/monotonic-clock@0.2.0",
        CLOCK,
    ] {
        let source = CORE.replacen(
            "(module",
            &format!(r#"(module (import "{namespace}" "clock" (func))"#),
            1,
        );
        assert_eq!(check(&source).unwrap_err().code, "wasm_wasi_unsupported");
    }
}

#[test]
fn raw_component_markers_are_packaging_errors_not_core_or_wasi_guesses() {
    let mut bytes = binary("(module)");
    custom(&mut bytes, "component-type:atri", b"encoded WIT metadata");
    assert_eq!(inspect(&bytes).unwrap_err().code, "wasm_component_required");
    for source in [
        r#"(module (func (export "cabi_realloc")))"#,
        r#"(module (func (export "atri:extension/plugin@1.0.0#describe")))"#,
        r#"(module (import "atri:extension/host@1.0.0" "invoke" (func)))"#,
    ] {
        assert_eq!(check(source).unwrap_err().code, "wasm_component_required");
    }
    let mut unrelated = binary("(module)");
    custom(&mut unrelated, "debug-info", b"not component type metadata");
    assert_eq!(inspect(&unrelated).unwrap_err().code, "wasm_core_abi");
}

#[test]
fn component_preflight_checks_only_root_interfaces_and_supported_wasi_version() {
    let nested = r#"(core module
        (import "wasi_snapshot_preview1" "unused" (func))
        (func (export "cabi_realloc")))"#;
    assert_eq!(check(&component(nested)).unwrap(), Backend::Component);
    for name in [HOST, CLOCK, CLOCK_TYPES] {
        let body = format!(r#"(type $t (instance)) (import "{name}" (instance (type $t)))"#);
        assert_eq!(check(&component(&body)).unwrap(), Backend::Component);
    }
    for name in [
        "wasi:clocks/monotonic-clock@0.2.0",
        "wasi:clocks/types@0.2.0",
        "wasi:filesystem/types@0.3.0",
    ] {
        let body = format!(r#"(type $t (instance)) (import "{name}" (instance (type $t)))"#);
        assert_eq!(
            check(&component(&body)).unwrap_err().code,
            "wasm_wasi_unsupported"
        );
    }
    let wrong_import = format!(r#"(type $t (func)) (import "{HOST}" (func (type $t)))"#);
    assert_eq!(
        check(&component(&wrong_import)).unwrap_err().code,
        "wasm_component_abi"
    );
    assert_eq!(check("(component)").unwrap_err().code, "wasm_component_abi");
    let wrong_export =
        r#"(component (type $t (instance)) (export "atri:extension/plugin@1.0.0" (type $t)))"#;
    assert_eq!(check(wrong_export).unwrap_err().code, "wasm_component_abi");
}

#[test]
fn malformed_headers_sections_and_export_indices_have_stable_format_errors() {
    for bytes in [
        b"not wasm".as_slice(),
        b"\0asm\x02\0\0\0",
        b"\0asm\x01\0\0\0\x01\x01\x01",
    ] {
        assert_eq!(inspect(bytes).unwrap_err().code, "wasm_format");
    }
    let source = CORE.replace("(export \"atri_invoke\")", "");
    let source = format!(
        "{} (export \"atri_invoke\" (func 9999)))",
        source.strip_suffix(')').unwrap()
    );
    assert_eq!(check(&source).unwrap_err().code, "wasm_format");
}

#[tokio::test]
async fn runtime_open_propagates_raw_component_failure_with_deployed_path() {
    let root = tempfile::TempDir::new().unwrap();
    let directory = root.path().join("extensions");
    tokio::fs::create_dir(&directory).await.unwrap();
    let path = directory.join("arbitrary-extension.wasm");
    let mut bytes = binary("(module)");
    custom(&mut bytes, "component-type", b"WIT");
    tokio::fs::write(&path, bytes).await.unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        crate::runtime::Runtime::open(root.path().into()),
    )
    .await
    .expect("binary rejection must not wait for a guest or JIT job");
    let error = match result {
        Ok(_) => panic!("raw component must fail startup"),
        Err(error) => error,
    };
    assert_eq!(error.code, "wasm_component_required");
    assert!(error.message.contains(&path.display().to_string()));
}

#[tokio::test]
async fn malformed_function_body_is_reported_with_compile_stage_and_path() {
    let mut bytes = binary(CORE);
    let opcode = Parser::new(0)
        .parse_all(&bytes)
        .find_map(|payload| match payload.unwrap() {
            Payload::CodeSectionEntry(body) => {
                Some(body.get_operators_reader().unwrap().original_position() as usize)
            }
            _ => None,
        })
        .unwrap();
    bytes[opcode] = 0xff;
    // Interface inspection does not duplicate the compiler's body validation.
    assert_eq!(inspect(&bytes).unwrap(), Backend::Core);
    let root = tempfile::TempDir::new().unwrap();
    let path = root.path().join("malformed-body.wasm");
    tokio::fs::write(&path, bytes).await.unwrap();
    let error = super::super::WasmModule::load(&path).await.err().unwrap();
    assert_eq!(error.code, "wasm_compile");
    assert!(error.message.contains(&path.display().to_string()));
}

#[tokio::test]
async fn structurally_valid_core_still_checks_the_executed_abi_version() {
    let bytes = binary(&CORE.replace("(result i32) i32.const 1", "(result i32) i32.const 2"));
    assert_eq!(inspect(&bytes).unwrap(), Backend::Core);
    let root = tempfile::TempDir::new().unwrap();
    let path = root.path().join("different-abi.wasm");
    tokio::fs::write(&path, bytes).await.unwrap();
    let error = super::super::WasmModule::load(&path).await.err().unwrap();
    assert_eq!(error.code, "extension_abi_mismatch");
    assert!(error.message.contains(&path.display().to_string()));
}
