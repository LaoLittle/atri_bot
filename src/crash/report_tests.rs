use super::super::model::{LinuxSignalAddress, TaskSnapshot, ThreadSnapshot};
use super::*;

struct ReportDirectory(PathBuf);
impl ReportDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("atri-report-{}", uuid::Uuid::new_v4()));
        crate::storage::private_directory(&path).unwrap();
        Self(path)
    }
}
impl Drop for ReportDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn snapshot() -> CrashSnapshot {
    CrashSnapshot {
        version: 1,
        pid: 123,
        executable: "atri".into(),
        threads: vec![ThreadSnapshot {
            os_thread_id: 70001,
            extension: "io.atri.test".into(),
            operation: 5,
            task_id: u64::MAX - 1,
            consistent: true,
        }],
        tasks: vec![
            TaskSnapshot {
                extension: "io.atri.test".into(),
                id: u64::MAX - 1,
                name: "background worker".into(),
                state: "running".into(),
            },
            TaskSnapshot {
                extension: "io.atri.other".into(),
                id: 3,
                name: "pending worker".into(),
                state: "cancelled".into(),
            },
        ],
    }
}

#[tokio::test]
async fn corrupt_dump_is_preserved_and_partial_reports_are_private() {
    let directory = ReportDirectory::new();
    let dump_path = directory.0.join("corrupt.dmp");
    let corrupt = b"MDMP\0truncated crash data";
    crate::storage::write_private_file(&dump_path, corrupt).unwrap();
    let text_path = write_report(&dump_path, &snapshot(), None).await.unwrap();
    assert_eq!(std::fs::read(&dump_path).unwrap(), corrupt);
    let text = std::fs::read_to_string(&text_path).unwrap();
    assert!(text.contains("Minidump parsing failed"));
    assert!(text.contains("No native stack was reconstructed"));
    assert!(text.contains("18446744073709551614"));
    assert!(text.contains("state=cancelled"));
    assert!(!text.contains("#0"));
    let structured: Value =
        serde_json::from_slice(&std::fs::read(dump_path.with_extension("json")).unwrap()).unwrap();
    assert!(structured["processor"].is_null());
    assert_eq!(structured["snapshot"]["tasks"].as_array().unwrap().len(), 2);
    assert_eq!(
        structured["snapshot"]["tasks"][0]["id"].as_u64(),
        Some(u64::MAX - 1)
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in [&dump_path, &text_path, &dump_path.with_extension("json")] {
            assert_eq!(
                std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}

#[tokio::test]
async fn valid_header_without_required_streams_reports_processing_failure() {
    let directory = ReportDirectory::new();
    let dump_path = directory.0.join("partial.dmp");
    // A real little-endian MINIDUMP_HEADER with an empty stream directory.
    // This exercises the parser/processor distinction, not an invented call stack.
    let mut header = Vec::new();
    for value in [0x504d444du32, 0xa793, 0, 32, 0, 0] {
        header.extend_from_slice(&value.to_le_bytes());
    }
    header.extend_from_slice(&0u64.to_le_bytes());
    crate::storage::write_private_file(&dump_path, &header).unwrap();
    assert!(Minidump::read_path(&dump_path).is_ok());
    let text_path = write_report(&dump_path, &snapshot(), None).await.unwrap();
    let text = std::fs::read_to_string(text_path).unwrap();
    assert!(text.contains("Minidump processing failed"));
    assert!(text.contains("Native threads unavailable"));
    assert_eq!(std::fs::read(dump_path).unwrap(), header);
}

#[test]
fn attribution_uses_os_tid_not_thread_index_or_mach_port_and_preserves_task_ids() {
    let snapshot = serde_json::to_value(snapshot()).unwrap();
    let thread = json!({"thread_id": 99, "os_thread_id": 70001});
    let annotation = thread_context(&thread, &snapshot);
    assert!(annotation.contains("extension=io.atri.test"));
    assert!(annotation.contains("task=18446744073709551614"));
    assert!(annotation.contains("state=running"));
    assert!(
        thread_context(&json!({"thread_id":70001,"os_thread_id":99}), &snapshot)
            .contains("no matching")
    );
    assert!(thread_context(&json!({"thread_id":70001}), &snapshot).contains("OS TID unavailable"));
}

#[test]
fn interrupted_activity_and_reused_tids_are_not_falsely_attributed() {
    let mut snapshot = serde_json::to_value(snapshot()).unwrap();
    let thread = json!({"os_thread_id":70001});
    snapshot["threads"][0]["consistent"] = json!(false);
    assert!(thread_context(&thread, &snapshot).contains("update interrupted"));
    snapshot["threads"][0]["consistent"] = json!(true);
    let duplicate = snapshot["threads"][0].clone();
    snapshot["threads"].as_array_mut().unwrap().push(duplicate);
    assert!(thread_context(&thread, &snapshot).contains("ambiguous/stale"));
    snapshot["threads"][1]["operation"] = json!(255);
    assert!(thread_context(&thread, &snapshot).contains("state=running"));
}

#[test]
fn shared_host_thread_prefers_active_extension_over_restored_idle_slots() {
    let mut snapshot = serde_json::to_value(snapshot()).unwrap();
    let thread = json!({"os_thread_id":70001});
    let mut idle = snapshot["threads"][0].clone();
    idle["extension"] = json!("io.atri.previously-active");
    idle["operation"] = json!(0);
    idle["task_id"] = json!(0);
    snapshot["threads"].as_array_mut().unwrap().insert(0, idle);
    let annotation = thread_context(&thread, &snapshot);
    assert!(annotation.contains("extension=io.atri.test"));
    assert!(annotation.contains("state=running"));
    snapshot["threads"][1]["operation"] = json!(255);
    assert!(thread_context(&thread, &snapshot).contains("extension=io.atri.previously-active"));
    snapshot["threads"][0]["operation"] = json!(255);
    snapshot["threads"].as_array_mut().unwrap().pop();
    let annotation = thread_context(&thread, &snapshot);
    assert!(annotation.contains("exited registration"));
    assert!(!annotation.contains("state=running"));
}

#[tokio::test]
async fn absent_modules_leave_raw_processor_addresses_and_honest_diagnostics() {
    // Stable-schema input for the enrichment stage; not a simulated native crash.
    let mut processor = json!({"crash_info":{"crashing_thread":1},"threads":[
        {"thread_id":7,"os_thread_id":7,"frames":[]},
        {"thread_id":70001,"os_thread_id":70001,"frames":[{"offset":"0x7fff12345678","module":"unavailable.dll","module_offset":"0x5678","trust":"context","missing_symbols":true}]}
    ]});
    let mut diagnostics = Vec::new();
    enrich_symbols(&mut processor, &[], &mut diagnostics).await;
    assert_eq!(
        processor["threads"][1]["frames"][0]["offset"],
        "0x7fff12345678"
    );
    assert!(processor["threads"][1]["frames"][0]["function"].is_null());
    assert_eq!(
        processor["crashing_thread"]["frames"][0]["module_offset"],
        "0x5678"
    );
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("1 frames retain raw addresses"))
    );
    let report = json!({"processor":processor,"snapshot":snapshot(),"raw_dump":"crash.dmp","diagnostics":diagnostics});
    let text = render_report(&report);
    assert!(text.find("thread 70001 [CRASHED]").unwrap() < text.find("thread 7 ").unwrap());
    assert!(text.contains("unavailable.dll+0x5678 [context]"));
    assert!(text.contains("no frames invented"));
    assert!(text.contains("state=cancelled"));
}

#[test]
fn omitted_schema_fields_do_not_panic_or_invent_frames() {
    let text = render_report(&json!({"processor":{},"snapshot":{},"raw_dump":"missing.dmp"}));
    assert!(text.contains("OS TID=unknown"));
    assert!(text.contains("Native threads unavailable"));
    assert!(!text.contains("#0"));
}

#[test]
fn terminal_control_sequences_and_string_truncation_are_visible() {
    assert_eq!(
        display(&json!("worker\u{1b}[31m\nsecret")),
        "worker\\u{1b}[31m\\nsecret"
    );
    let mut value = json!({"name":"é".repeat(MAX_STRING_BYTES)});
    bound_strings(&mut value);
    assert!(
        value["name"]
            .as_str()
            .unwrap()
            .ends_with("[TRUNCATED string]")
    );
    assert!(value["name"].as_str().unwrap().len() < MAX_STRING_BYTES + 64);
}

#[test]
fn serialization_limit_never_produces_a_partial_success() {
    let mut bytes = LimitedBytes::new(16);
    assert!(serde_json::to_writer(&mut bytes, &json!({"large":"a".repeat(32)})).is_err());
    assert!(bytes.bytes.len() <= 16);
}

#[inline(never)]
fn report_symbol_regression_anchor(value: usize) -> usize {
    std::hint::black_box(value).wrapping_add(1)
}

#[tokio::test]
async fn native_local_symbols_resolve_function_source_and_relative_image_addresses() {
    // Use the actual test binary's native debug format on each OS, not an ELF-only
    // fixture or a copied implementation. Release/test artifacts must keep debuginfo.
    assert_eq!(
        report_symbol_regression_anchor(std::hint::black_box(41)),
        42
    );
    let manager = SymbolManager::with_config(
        SymbolManagerConfig::new()
            .respect_nt_symbol_path(false)
            .use_debuginfod(false)
            .use_spotlight(false),
    );
    let executable = std::env::current_exe().unwrap();
    let map = manager
        .load_symbol_map_for_binary_at_path(&executable, None)
        .await
        .unwrap();
    let (relative, _) = map
        .iter_symbols()
        .find(|(_, name)| name.contains("report_symbol_regression_anchor"))
        .expect("native test function symbol");
    let info = map
        .lookup(LookupAddress::Relative(relative))
        .await
        .expect("relative native symbol address");
    assert!(info.symbol.name.contains("report_symbol_regression_anchor"));
    let frames = info
        .frames
        .expect("native source debug information retained by artifact profile");
    assert!(frames.iter().any(|frame| {
        frame
            .file_path
            .as_ref()
            .is_some_and(|file| file.raw_path().ends_with("report_tests.rs"))
            && frame.line_number.is_some()
    }));
    let directory = ReportDirectory::new();
    let corrupt_path = directory.0.join("corrupt-native-symbols");
    crate::storage::write_private_file(&corrupt_path, b"not an ELF, Mach-O or PE file").unwrap();
    let missing_path = directory.0.join("missing-native-symbols");
    for path in [&corrupt_path, &missing_path] {
        // Module's existing tuple implementation supplies a real debug ID while
        // directing our production loader at absent/corrupt native symbol files.
        let module = (path.to_str().unwrap(), map.debug_id());
        let mut isolated_manager = SymbolManager::with_config(
            SymbolManagerConfig::new()
                .respect_nt_symbol_path(false)
                .use_debuginfod(false)
                .use_spotlight(false),
        );
        assert!(
            load_local_map(&mut isolated_manager, &module)
                .await
                .is_err()
        );
    }
    assert_eq!(
        std::fs::read(corrupt_path).unwrap(),
        b"not an ELF, Mach-O or PE file"
    );
}

fn address_report(native: Value, processor: Value, signal: Option<&LinuxSignalContext>) -> Value {
    json!({
        "fault_addresses": fault_addresses(&native, &processor, signal),
        "native_exception": native, "processor": processor,
        "linux_signal_context": signal, "snapshot": snapshot(), "raw_dump": "crash.dmp",
    })
}

#[test]
fn protected_page_write_keeps_raw_and_inferred_addresses_separate() {
    // Decoder metadata from the actual non-null protected-page write. These are
    // regression inputs to report construction, not a simulated native crash.
    let native = json!({
        "platform":"linux", "exception_code":"0xb", "exception_address":"0x0",
        "exception_address_meaning":"unreliable copied signalfd field",
        "instruction_pointer":"0x7f4a33001000",
    });
    let processor = json!({"crash_info":{
        "address":"0x0000000000000000", "type":"SIGSEGV / SEGV_ACCERR",
        "instruction":"mov byte [rdi], 0x7f", "memory_accesses":[{
            "address":"0x00007f4a33def000", "access_type":"write", "size":1, "is_likely_guard_page":true,
        }],
    }});
    let signal = LinuxSignalContext {
        signal: 11,
        code: 2,
        errno: 0,
        address: Some(LinuxSignalAddress {
            address: 0x7f4a33def000,
            kind: LinuxSignalAddressKind::MemoryAccess,
        }),
    };
    let report = address_report(native.clone(), processor.clone(), Some(&signal));
    assert_eq!(report["native_exception"], native);
    assert_eq!(report["processor"], processor);
    let addresses = &report["fault_addresses"];
    assert_eq!(addresses["raw_memory_address"]["address"], "0x7f4a33def000");
    assert_eq!(addresses["raw_memory_address"]["certainty"], "raw");
    assert!(
        addresses["raw_memory_address"]["provenance"]
            .as_str()
            .unwrap()
            .contains("siginfo_t.si_addr")
    );
    assert!(addresses["raw_missing_reason"].is_null());
    assert_eq!(
        addresses["instruction_memory_accesses"][0]["address"],
        "0x7f4a33def000"
    );
    assert!(
        addresses["instruction_memory_accesses"][0]["certainty"]
            .as_str()
            .unwrap()
            .starts_with("inferred")
    );
    let text = render_report(&report);
    assert!(text.contains("memory fault address=0x7f4a33def000 [raw] pc=0x7f4a33001000"));
    assert!(!text.contains("\nmemory fault address=0x0 "));
    assert!(text.contains("Raw processor crash_info.address=0x0000000000000000"));
    assert!(text.contains("Inferred instruction memory access address=0x7f4a33def000 type=write"));

    // An old dump cannot recover the original siginfo_t address: even one decoded
    // guarded write remains an inference, never a replacement "known" faultaddr.
    let report = address_report(native, processor, None);
    assert!(report["fault_addresses"]["raw_memory_address"].is_null());
    let text = render_report(&report);
    assert!(text.contains("memory fault address=unknown [raw unavailable]"));
    assert!(text.contains("Original Linux siginfo_t address was not captured"));
    assert!(text.contains("Inferred instruction memory access address=0x7f4a33def000"));
}

#[test]
fn genuine_raw_null_and_unknown_linux_signal_address_are_distinct() {
    let mut signal = LinuxSignalContext {
        signal: 11,
        code: 1,
        errno: 0,
        address: Some(LinuxSignalAddress {
            address: 0,
            kind: LinuxSignalAddressKind::MemoryAccess,
        }),
    };
    let native =
        json!({"platform":"linux","exception_address":"0x0","instruction_pointer":"0x1234"});
    let report = address_report(native.clone(), json!({}), Some(&signal));
    assert_eq!(
        report["fault_addresses"]["raw_memory_address"]["address"],
        "0x0"
    );
    assert!(render_report(&report).contains("memory fault address=0x0 [raw] pc=0x1234"));
    signal.code = 128; // SI_KERNEL: a zero union is not a known null memory address.
    signal.address = None;
    let report = address_report(native.clone(), json!({}), Some(&signal));
    assert!(report["fault_addresses"]["raw_memory_address"].is_null());
    assert!(render_report(&report).contains("zero is not evidence of a null fault"));
    signal.code = 1;
    signal.address = Some(LinuxSignalAddress {
        address: 0x1234,
        kind: LinuxSignalAddressKind::Instruction,
    });
    let report = address_report(native, json!({}), Some(&signal));
    assert!(report["fault_addresses"]["raw_memory_address"].is_null());
    assert!(
        render_report(&report).contains("identifies an instruction, not a memory-access fault")
    );
}

#[test]
fn mach_subcodes_and_seh_access_parameters_are_not_exception_pcs() {
    let native = json!({
        "platform":"mach","exception_code":"0x1","exception_address":"0x6000",
        "number_parameters":3,"exception_information":["0x1","0x2","0x6000"],
        "instruction_pointer":"0x1000",
    });
    let report = address_report(native.clone(), json!({}), None);
    assert_eq!(
        report["fault_addresses"]["raw_memory_address"]["address"],
        "0x6000"
    );
    assert!(
        report["fault_addresses"]["raw_memory_address"]["provenance"]
            .as_str()
            .unwrap()
            .starts_with("Mach")
    );
    assert!(render_report(&report).contains("pc=0x1000"));
    let mut missing_subcode = native;
    missing_subcode["number_parameters"] = json!(2);
    let report = address_report(missing_subcode, json!({}), None);
    assert!(report["fault_addresses"]["raw_memory_address"].is_null());
    assert!(
        report["fault_addresses"]["raw_missing_reason"]
            .as_str()
            .unwrap()
            .contains("PC fallback")
    );

    for code in ["0xc0000005", "0xc0000006"] {
        let native = json!({
            "platform":"windows","exception_code":code,"exception_address":"0x1000",
            "number_parameters":2,"exception_information":["0x1","0x6000"],
            "instruction_pointer":"0x1000",
        });
        let report = address_report(native.clone(), json!({}), None);
        assert_eq!(
            report["fault_addresses"]["raw_memory_address"]["address"],
            "0x6000"
        );
        assert!(
            report["fault_addresses"]["raw_memory_address"]["provenance"]
                .as_str()
                .unwrap()
                .starts_with("SEH")
        );
        let mut insufficient = native;
        insufficient["number_parameters"] = json!(1);
        assert!(
            address_report(insufficient, json!({}), None)["fault_addresses"]["raw_memory_address"]
                .is_null()
        );
    }
    let report = address_report(
        json!({
            "platform":"windows","exception_code":"0xc000001d","exception_address":"0x1000",
            "number_parameters":2,"exception_information":["0x1","0x6000"],
        }),
        json!({}),
        None,
    );
    assert!(report["fault_addresses"]["raw_memory_address"].is_null());
    assert!(render_report(&report).contains("pc=unknown"));
}

#[test]
fn malformed_addresses_and_parameter_counts_remain_visible_without_invented_values() {
    let native = json!({
        "platform":"windows","exception_code":"0xc0000005","exception_address":"0x1000",
        "number_parameters":16,"exception_information":["0x1","not-an-address"],
    });
    let processor = json!({"crash_info":{"address":"bad","memory_accesses":[{"address":"invalid","access_type":"write"}]}});
    let report = address_report(native.clone(), processor.clone(), None);
    assert_eq!(report["native_exception"], native);
    assert_eq!(report["processor"], processor);
    assert!(report["fault_addresses"]["raw_memory_address"].is_null());
    assert!(report["fault_addresses"]["instruction_memory_accesses"][0]["address"].is_null());
    assert_eq!(
        report["fault_addresses"]["instruction_memory_accesses"][0]["access"]["address"],
        "invalid"
    );
    let text = render_report(&report);
    assert!(text.contains("Inferred instruction memory access address=unknown"));
    assert!(text.contains("pc=unknown"));
}

#[test]
fn seh_32_bit_pointer_representation_does_not_overwrite_raw_parameters() {
    let native = json!({
        "platform":"windows","pointer_width_bits":32,
        "exception_code":"0xc0000005","exception_address":"0x1000",
        "number_parameters":2,"exception_information":["0x1","0xffffffff80006000"],
    });
    let report = address_report(native.clone(), json!({}), None);
    assert_eq!(report["native_exception"], native);
    assert_eq!(
        report["fault_addresses"]["raw_memory_address"]["address"],
        "0x80006000"
    );
    assert!(
        report["fault_addresses"]["raw_memory_address"]["normalization"]
            .as_str()
            .unwrap()
            .contains("32-bit target pointer")
    );
    assert!(render_report(&report).contains("Raw address representation: zero-extended 32-bit"));
}
