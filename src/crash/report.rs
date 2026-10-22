//! Out-of-process, local-only minidump processing. Nothing here runs in a fatal handler.

use std::{
    collections::HashMap,
    fmt::Write as _,
    io,
    path::{Path, PathBuf},
};

use atri_core::ApiError;
use minidump::{
    Minidump, MinidumpException, MinidumpModule, MinidumpSystemInfo, Module,
    system_info::{Os, PointerWidth},
};
use minidump_unwind::symbols::{Symbolizer, simple_symbol_supplier};
use serde::Serialize;
use serde_json::{Value, json};
use wholesym::{
    LibraryInfo, LookupAddress, MultiArchDisambiguator, SymbolManager, SymbolManagerConfig,
    SymbolMap,
};

use super::model::{CrashSnapshot, LinuxSignalAddressKind, LinuxSignalContext};

const MAX_THREADS: usize = 1024;
const MAX_FRAMES: usize = 512;
const MAX_INLINES: usize = 32;
const MAX_MODULES: usize = 4096;
const MAX_JSON_BYTES: usize = 32 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 8 * 1024 * 1024;
const MAX_STRING_BYTES: usize = 8192;

/// Save private text and structured reports beside the retained raw dump.
/// The supervisor, not this writer, prints the returned text file exactly once.
/// Parsing/symbol failures are diagnostic data, not reasons to discard the dump.
pub async fn write_report(
    dump_path: &Path,
    snapshot: &CrashSnapshot,
    signal_context: Option<&LinuxSignalContext>,
) -> Result<PathBuf, ApiError> {
    let mut diagnostics = Vec::new();
    let mut native_exception = Value::Null;
    let mut processor = Value::Null;
    match Minidump::read_path(dump_path) {
        Ok(dump) => {
            if let Ok(exception) = dump.get_stream::<MinidumpException>() {
                let system = dump.get_stream::<MinidumpSystemInfo>().ok();
                native_exception = native_exception_metadata(&exception, system.as_ref());
            }
            // No HTTP supplier, environment symbol servers, or external debugger.
            // The processor uses its native cross-platform frame-pointer/scan fallback.
            let symbols = Symbolizer::new(simple_symbol_supplier(Vec::new()));
            match minidump_processor::process_minidump(&dump, &symbols).await {
                Ok(mut state) => {
                    if let Some(index) =
                        state.requesting_thread.filter(|&i| i < state.threads.len())
                    {
                        state.threads.swap(0, index);
                        state.requesting_thread = Some(0);
                    }
                    if state.threads.len() > MAX_THREADS {
                        diagnostics.push(format!("TRUNCATED native threads: {} available, {MAX_THREADS} retained (crashing thread first)", state.threads.len()));
                        state.threads.truncate(MAX_THREADS);
                    }
                    for thread in &mut state.threads {
                        if thread.frames.len() > MAX_FRAMES {
                            diagnostics.push(format!(
                                "TRUNCATED thread {}: {} frames available, {MAX_FRAMES} retained",
                                thread.thread_id,
                                thread.frames.len()
                            ));
                            thread.frames.truncate(MAX_FRAMES);
                        }
                        if thread.frames.is_empty() {
                            diagnostics.push(format!(
                                "Thread {} has no recoverable native frames ({:?})",
                                thread.thread_id, thread.info
                            ));
                        }
                    }
                    let modules: Vec<_> = state.modules.iter().take(MAX_MODULES).cloned().collect();
                    if state.modules.iter().count() > MAX_MODULES {
                        diagnostics.push(format!(
                            "TRUNCATED local symbol module lookup to {MAX_MODULES} modules"
                        ));
                    }
                    let mut bytes = LimitedBytes::new(MAX_JSON_BYTES);
                    match state
                        .print_json(&mut bytes, false)
                        .and_then(|()| serde_json::from_slice(&bytes.bytes))
                    {
                        Ok(value) => processor = value,
                        Err(error) => diagnostics.push(format!(
                            "Processor JSON unavailable (possibly size limit): {error}"
                        )),
                    }
                    if !processor.is_null() {
                        annotate_thread_ids(&mut processor, &mut diagnostics);
                        enrich_symbols(&mut processor, &modules, &mut diagnostics).await;
                    }
                }
                Err(error) => diagnostics.push(format!(
                    "Minidump processing failed; partial context only: {error}"
                )),
            }
        }
        Err(error) => diagnostics.push(format!(
            "Minidump parsing failed; native stack unavailable: {error}"
        )),
    }
    if processor.is_null() {
        diagnostics.push("No native stack was reconstructed. The original dump is retained for offline analysis.".into());
    }
    let addresses = fault_addresses(&native_exception, &processor, signal_context);
    let mut report = json!({
        "atri_crash_report_version": 2,
        "processor_version": "minidump-processor 0.26",
        "symbol_policy": "local ELF/DWARF, Mach-O/dSYM and PE/PDB only; no symbol downloads or uploads",
        "raw_dump": dump_path.to_string_lossy(),
        "native_exception": native_exception,
        "linux_signal_context": signal_context,
        "fault_addresses": addresses,
        "snapshot": snapshot,
        "processor": processor,
        "diagnostics": diagnostics,
    });
    bound_strings(&mut report);
    let text = render_report(&report);
    let text_path = dump_path.with_extension("txt");
    let json_path = dump_path.with_extension("json");
    // Save readable diagnostics first, so even a JSON size/write failure leaves useful evidence.
    crate::storage::write_private_file(&text_path, text.as_bytes())?;
    let mut bytes = LimitedBytes::new(MAX_JSON_BYTES);
    if let Err(error) = serde_json::to_writer_pretty(&mut bytes, &report) {
        let notice = format!(
            "\nDIAGNOSTIC: structured report could not be serialized within {MAX_JSON_BYTES} bytes: {error}\nRaw dump retained: {}\n",
            dump_path.display()
        );
        crate::storage::write_private_file(&text_path, format!("{text}{notice}").as_bytes())?;
        // Keep valid JSON, never a silently truncated JSON document.
        let partial = json!({
            "atri_crash_report_version": 2, "raw_dump": dump_path.to_string_lossy(),
            "diagnostics": [notice], "processor": null, "native_exception": native_exception,
            "linux_signal_context": signal_context,
            "fault_addresses": fault_addresses(&native_exception, &Value::Null, signal_context),
        });
        let bytes = serde_json::to_vec_pretty(&partial).map_err(ApiError::internal)?;
        crate::storage::write_private_file(&json_path, &bytes)?;
    } else if let Err(error) = crate::storage::write_private_file(&json_path, &bytes.bytes) {
        let notice = format!("\nDIAGNOSTIC: structured report write failed: {error}\n");
        crate::storage::write_private_file(&text_path, format!("{text}{notice}").as_bytes())?;
    }
    Ok(text_path)
}

fn native_exception_metadata(
    exception: &MinidumpException<'_>,
    system: Option<&MinidumpSystemInfo>,
) -> Value {
    let raw = &exception.raw.exception_record;
    let (platform, meaning) = match system.map(|system| system.os) {
        Some(Os::Linux) => (
            "linux",
            "unreliable signalfd ssi_addr from crash-handler 0.8.1's byte-copied siginfo_t; not an instruction pointer",
        ),
        Some(Os::MacOs | Os::Ios) if raw.exception_code == 1 => (
            "mach",
            "Mach EXC_BAD_ACCESS subcode when present; otherwise writer PC fallback (not a known memory address)",
        ),
        Some(Os::MacOs | Os::Ios) => (
            "mach",
            "Mach writer instruction pointer or unavailable; not a data-fault address",
        ),
        Some(Os::Windows) => (
            "windows",
            "SEH exception instruction address; accessed memory is ExceptionInformation[1] for access/in-page faults",
        ),
        _ => (
            "unknown",
            "platform-specific exception address; not classified as a memory address or PC",
        ),
    };
    let instruction_pointer = system
        .and_then(|system| exception.context(system, None))
        .map(|context| format!("{:#x}", context.get_instruction_pointer()));
    let pointer_width_bits = system.and_then(|system| match system.cpu.pointer_width() {
        PointerWidth::Bits32 => Some(32u32),
        PointerWidth::Bits64 => Some(64),
        PointerWidth::Unknown => None,
    });
    json!({
        "os_thread_id": os_thread_id(exception.thread_id),
        "platform": platform,
        "pointer_width_bits": pointer_width_bits,
        "exception_code": format!("{:#x}", raw.exception_code),
        "exception_flags": format!("{:#x}", raw.exception_flags),
        "exception_address": format!("{:#x}", raw.exception_address),
        "exception_address_meaning": meaning,
        "number_parameters": raw.number_parameters,
        // Preserve every fixed raw slot plus the declared count, including malformed
        // counts; interpreting an unknown slot is different from retaining its bytes.
        "exception_information": raw.exception_information.map(|value| format!("{value:#x}")),
        "instruction_pointer": instruction_pointer,
    })
}

#[derive(Serialize)]
struct AddressEvidence {
    address: String,
    provenance: &'static str,
    certainty: &'static str,
    normalization: Option<&'static str>,
}

#[derive(Serialize)]
struct InferredMemoryAccess<'a> {
    address: Option<String>,
    provenance: &'static str,
    certainty: &'static str,
    // Keep the decoder's complete access metadata (size, type, guard heuristic).
    access: &'a Value,
}

#[derive(Serialize)]
struct FaultAddresses<'a> {
    raw_memory_address: Option<AddressEvidence>,
    raw_missing_reason: Option<&'static str>,
    instruction_memory_accesses: Vec<InferredMemoryAccess<'a>>,
}

fn fault_addresses<'a>(
    native: &Value,
    processor: &'a Value,
    signal: Option<&LinuxSignalContext>,
) -> FaultAddresses<'a> {
    let mut raw = None;
    let mut reason = "Raw memory-fault address unavailable; exception platform/type is unknown or does not define a memory address.";
    let mut set_raw = |address: u64, provenance| {
        let is_32_bit = native["pointer_width_bits"].as_u64() == Some(32);
        let address = if is_32_bit {
            address as u32 as u64
        } else {
            address
        };
        raw = Some(AddressEvidence {
            address: format!("{address:#x}"), provenance, certainty: "raw",
            normalization: is_32_bit.then_some("zero-extended 32-bit target pointer; original numeric metadata retained separately"),
        });
    };
    match native["platform"].as_str() {
        Some("linux") | None if signal.is_some() => {
            let context = signal.unwrap();
            match context.address {
                Some(address) if address.kind == LinuxSignalAddressKind::MemoryAccess => {
                    set_raw(
                        address.address,
                        "original Linux siginfo_t.si_addr captured in fatal callback",
                    );
                }
                Some(_) => {
                    reason = "Original Linux si_addr identifies an instruction, not a memory-access fault."
                }
                None => {
                    reason = "Linux signal/code does not provide a known memory-fault address (for example SI_USER/SI_TKILL/SI_KERNEL or an unsupported code); zero is not evidence of a null fault."
                }
            }
        }
        Some("linux") => {
            reason = "Original Linux siginfo_t address was not captured. Pinned crash-handler 0.8.1 copies siginfo_t into signalfd_siginfo without conversion; the retained dump/processor address is unreliable, including when zero.";
        }
        Some("mach") => {
            let parameters = native["number_parameters"].as_u64();
            if hex_value(&native["exception_code"]) == Some(1)
                && parameters.is_some_and(|count| (3..=15).contains(&count))
            {
                if let Some(address) = hex_value(&native["exception_information"][2]) {
                    set_raw(
                        address,
                        "Mach EXC_BAD_ACCESS subcode retained in minidump ExceptionInformation[2]",
                    );
                }
            }
            reason = "No retained Mach EXC_BAD_ACCESS subcode; exception_address may be a writer PC fallback, not a known memory-fault address.";
        }
        Some("windows") => {
            let parameters = native["number_parameters"].as_u64();
            if matches!(
                hex_value(&native["exception_code"]),
                Some(0xc0000005 | 0xc0000006)
            ) && parameters.is_some_and(|count| (2..=15).contains(&count))
            {
                if let Some(address) = hex_value(&native["exception_information"][1]) {
                    set_raw(
                        address,
                        "SEH access/in-page fault ExceptionInformation[1]; ExceptionAddress is the instruction address",
                    );
                }
            }
            reason = "No valid SEH access/in-page-fault memory address; ExceptionAddress is not a data-fault address.";
        }
        _ => {}
    }
    let accesses = processor
        .pointer("/crash_info/memory_accesses")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|access| InferredMemoryAccess {
            address: hex_value(&access["address"]).map(|address| format!("{address:#x}")),
            provenance: "minidump-processor instruction decoding and captured registers",
            certainty: "inferred memory access; not proof that this access caused the fault",
            access,
        })
        .collect();
    FaultAddresses {
        raw_missing_reason: raw.is_none().then_some(reason),
        raw_memory_address: raw,
        instruction_memory_accesses: accesses,
    }
}

fn local_symbol_config(modules: &[MinidumpModule]) -> SymbolManagerConfig {
    // Start from an empty config, explicitly disable every environment/network source.
    let mut config = SymbolManagerConfig::new()
        .respect_nt_symbol_path(false)
        .use_debuginfod(false)
        .use_spotlight(false);
    for module in modules {
        if let Some(parent) = Path::new(module.code_file().as_ref()).parent() {
            if !parent.as_os_str().is_empty() {
                config = config.extra_symbols_directory(parent);
            }
        }
    }
    config
}

async fn load_local_map(
    manager: &mut SymbolManager,
    module: &dyn Module,
) -> Result<SymbolMap, String> {
    let path = module.code_file().into_owned();
    let debug_path = module.debug_file().map(|name| name.into_owned());
    let debug_name = debug_path.as_deref().map(portable_basename);
    let debug_id = module.debug_identifier();
    manager.add_known_library(LibraryInfo {
        path: Some(path.clone()),
        name: Some(portable_basename(&path).to_owned()),
        debug_path: debug_path.clone(),
        debug_name: debug_name.map(str::to_owned),
        debug_id,
        ..LibraryInfo::default()
    });
    if let (Some(name), Some(id)) = (debug_name, debug_id) {
        // This searches local build-id/debuglink/dSYM/PDB locations and validates identity.
        if let Ok(map) = manager.load_symbol_map(name, id).await {
            return Ok(map);
        }
    }
    let map = manager
        .load_symbol_map_for_binary_at_path(
            Path::new(&path),
            debug_id.map(MultiArchDisambiguator::DebugId),
        )
        .await
        .map_err(|error| error.to_string())?;
    if debug_id.is_some_and(|expected| expected != map.debug_id()) {
        return Err(
            "local symbol build ID does not match dump; refusing misleading symbols".into(),
        );
    }
    Ok(map)
}

async fn enrich_symbols(
    processor: &mut Value,
    modules: &[MinidumpModule],
    diagnostics: &mut Vec<String>,
) {
    let mut manager = SymbolManager::with_config(local_symbol_config(modules));
    let mut maps: HashMap<u64, Option<SymbolMap>> = HashMap::new();
    let Some(threads) = processor.get_mut("threads").and_then(Value::as_array_mut) else {
        diagnostics.push("Processor supplied no thread array".into());
        return;
    };
    let mut missing = 0usize;
    let mut resolved = 0usize;
    for thread in threads.iter_mut() {
        let Some(frames) = thread.get_mut("frames").and_then(Value::as_array_mut) else {
            continue;
        };
        for frame in frames {
            let Some(address) = hex_value(&frame["offset"]) else {
                missing += 1;
                continue;
            };
            let Some(module) = modules.iter().find(|module| {
                address >= module.base_address() && address - module.base_address() < module.size()
            }) else {
                missing += 1;
                continue;
            };
            let base = module.base_address();
            // `offset` is the processor's call-site instruction, already adjusted from
            // return addresses for each CPU. Subtract the loaded image base once, not
            // again by one byte, nor by ELF p_vaddr / Mach-O __TEXT / PE image base.
            let Some(relative) = address
                .checked_sub(base)
                .and_then(|value| u32::try_from(value).ok())
            else {
                diagnostics.push(format!(
                    "Address {address:#x} exceeds native symbol relative-address range"
                ));
                missing += 1;
                continue;
            };
            if !maps.contains_key(&base) {
                let map = match load_local_map(&mut manager, module).await {
                    Ok(map) => Some(map),
                    Err(error) => {
                        diagnostics.push(format!(
                            "Local symbols unavailable for {}: {error}",
                            module.code_file()
                        ));
                        None
                    }
                };
                maps.insert(base, map);
            }
            let Some(map) = maps.get(&base).and_then(Option::as_ref) else {
                missing += 1;
                continue;
            };
            let Some(info) = map.lookup(LookupAddress::Relative(relative)).await else {
                missing += 1;
                continue;
            };
            frame["function"] = json!(info.symbol.name);
            frame["missing_symbols"] = json!(false);
            frame["symbol_source"] = json!("local native debug symbols");
            if let Some(mut source_frames) = info.frames {
                if let Some(outer) = source_frames.pop() {
                    apply_source(frame, &outer);
                }
                if source_frames.len() > MAX_INLINES {
                    diagnostics.push(format!(
                        "TRUNCATED inlines at {address:#x}: {} available, {MAX_INLINES} retained",
                        source_frames.len()
                    ));
                }
                frame["inlines"] = Value::Array(
                    source_frames
                        .iter()
                        .take(MAX_INLINES)
                        .map(|source| {
                            let mut inline = json!({});
                            apply_source(&mut inline, source);
                            inline
                        })
                        .collect(),
                );
            }
            resolved += 1;
        }
    }
    // This redundant stable-schema copy must contain the same enriched symbols.
    let crash_index = processor
        .pointer("/crashing_thread/threads_index")
        .and_then(Value::as_u64)
        .or_else(|| {
            processor
                .pointer("/crash_info/crashing_thread")
                .and_then(Value::as_u64)
        });
    if let Some(index) = crash_index.and_then(|index| usize::try_from(index).ok()) {
        if let Some(mut thread) = processor["threads"].get(index).cloned() {
            thread["threads_index"] = json!(index);
            processor["crashing_thread"] = thread;
        }
    }
    diagnostics.push(format!("Local symbolization: {resolved} native frames resolved; {missing} frames retain raw addresses/module offsets. Missing file/line means local source debug information was unavailable."));
    diagnostics.push("Native physical thread stacks are shown; managed task states are not reconstructed suspended async call stacks. Scan-derived frames are heuristic, not verified calls.".into());
}

fn apply_source(target: &mut Value, source: &wholesym::FrameDebugInfo) {
    if let Some(function) = &source.function {
        target["function"] = json!(function);
    }
    if let Some(file) = &source.file_path {
        target["file"] = json!(file.raw_path());
    }
    if let Some(line) = source.line_number {
        target["line"] = json!(line);
    }
}

fn portable_basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn hex_value(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        let value = value.as_str()?;
        u64::from_str_radix(value.strip_prefix("0x").unwrap_or(value), 16).ok()
    })
}

fn bound_strings(value: &mut Value) {
    match value {
        Value::String(text) if text.len() > MAX_STRING_BYTES => {
            let mut end = MAX_STRING_BYTES;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
            text.push_str(" [TRUNCATED string]");
        }
        Value::Array(values) => values.iter_mut().for_each(bound_strings),
        Value::Object(values) => values.values_mut().for_each(bound_strings),
        _ => {}
    }
}

/// Escape untrusted module/task/symbol strings before printing to a terminal.
fn display(value: &Value) -> String {
    match value {
        Value::Null => "unknown".into(),
        Value::String(text) => text.chars().flat_map(char::escape_default).collect(),
        other => other.to_string(),
    }
}

fn operation(value: &Value) -> String {
    match value.as_u64() {
        Some(0) => "idle".into(),
        Some(1) => "start".into(),
        Some(2) => "event".into(),
        Some(3) => "service".into(),
        Some(4) => "stop".into(),
        Some(5) => "managed-task poll".into(),
        Some(255) => "exited".into(),
        _ => format!("unknown({})", display(value)),
    }
}
// Darwin minidumps store the monitor's Mach thread port name, not the stable
// pthread_threadid_np ID used by framework snapshots. The monitor still owns the
// transferred send rights while this report is being generated.
#[cfg(target_os = "macos")]
fn os_thread_id(raw: u32) -> Option<u64> {
    #[repr(C)]
    struct ThreadIdentifierInfo {
        thread_id: u64,
        thread_handle: u64,
        dispatch_qaddr: u64,
    }
    unsafe extern "C" {
        fn thread_info(thread: u32, flavor: i32, info: *mut i32, count: *mut u32) -> i32;
    }
    let mut info = ThreadIdentifierInfo {
        thread_id: 0,
        thread_handle: 0,
        dispatch_qaddr: 0,
    };
    let mut count = 6;
    // THREAD_IDENTIFIER_INFO has three u64 fields, expressed as six integer_t's.
    let result = unsafe {
        thread_info(
            raw,
            4,
            (&mut info as *mut ThreadIdentifierInfo).cast(),
            &mut count,
        )
    };
    (result == 0 && count >= 6 && info.thread_id != 0).then_some(info.thread_id)
}

#[cfg(not(target_os = "macos"))]
fn os_thread_id(raw: u32) -> Option<u64> {
    Some(u64::from(raw))
}

fn annotate_thread_ids(processor: &mut Value, diagnostics: &mut Vec<String>) {
    if let Some(threads) = processor.get_mut("threads").and_then(Value::as_array_mut) {
        for thread in threads {
            let tid = thread["thread_id"]
                .as_u64()
                .and_then(|tid| u32::try_from(tid).ok())
                .and_then(os_thread_id);
            thread["os_thread_id"] = tid.map_or(Value::Null, |tid| json!(tid));
            if tid.is_none() {
                diagnostics.push(format!("OS thread ID unavailable for minidump thread {}; framework attribution omitted", thread["thread_id"]));
            }
        }
    }
}

fn thread_context(thread: &Value, snapshot: &Value) -> String {
    let Some(tid) = thread["os_thread_id"].as_u64() else {
        return "unattributed (OS TID unavailable)".into();
    };
    let priority = |context: &Value| match (
        context["consistent"].as_bool(),
        context["operation"].as_u64(),
    ) {
        (Some(true), Some(1..=5)) => 3,
        (Some(true), Some(0)) => 2,
        (_, Some(255)) => 0,
        _ => 1,
    };
    let mut selected: Option<&Value> = None;
    let mut ambiguous = false;
    for context in snapshot["threads"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|context| context["os_thread_id"].as_u64() == Some(tid))
    {
        match selected {
            Some(previous) if priority(context) < priority(previous) => {}
            Some(previous) if priority(context) == priority(previous) => ambiguous = true,
            _ => {
                selected = Some(context);
                ambiguous = false;
            }
        }
    }
    let Some(context) = selected else {
        return "unattributed (no matching framework OS TID)".into();
    };
    // Shared host threads have one slot per extension. Only the highest-priority
    // active slot may identify the callback; equally active slots cannot be guessed.
    if ambiguous {
        return "ambiguous/stale OS TID registrations; attribution unavailable".into();
    }
    if context["operation"].as_u64() == Some(255) {
        return format!(
            "extension={} [exited registration; live operation/task attribution unavailable]",
            display(&context["extension"])
        );
    }

    if context["consistent"].as_bool() != Some(true) {
        return format!(
            "extension={} [activity update interrupted; operation/task attribution unavailable]",
            display(&context["extension"])
        );
    }
    let task_id = context["task_id"].as_u64().unwrap_or(0);
    let task = snapshot["tasks"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|task| {
            task["extension"] == context["extension"] && task["id"].as_u64() == Some(task_id)
        });
    format!(
        "extension={} operation={} task={}{}",
        display(&context["extension"]),
        operation(&context["operation"]),
        task_id,
        task.map(|task| format!(
            " name={} state={}",
            display(&task["name"]),
            display(&task["state"])
        ))
        .unwrap_or_default()
    )
}

fn render_frame(text: &mut String, index: usize, frame: &Value, location: &Value, inline: bool) {
    let _ = writeln!(
        text,
        "  #{index:<3} {} {}{}",
        display(&location["offset"]),
        display(&frame["function"]),
        if inline { " [inline]" } else { "" }
    );
    if !location["module"].is_null() {
        let _ = writeln!(
            text,
            "       {}+{} [{}]",
            display(&location["module"]),
            display(&location["module_offset"]),
            display(&location["trust"])
        );
    }
    if !frame["file"].is_null() {
        let _ = writeln!(
            text,
            "       {}:{}",
            display(&frame["file"]),
            display(&frame["line"])
        );
    }
}

fn render_report(report: &Value) -> String {
    let processor = &report["processor"];
    let snapshot = &report["snapshot"];
    let crash_index = processor
        .pointer("/crashing_thread/threads_index")
        .and_then(Value::as_u64)
        .or_else(|| {
            processor
                .pointer("/crash_info/crashing_thread")
                .and_then(Value::as_u64)
        });
    let threads = processor["threads"].as_array();
    let crash_thread = crash_index.and_then(|index| threads?.get(usize::try_from(index).ok()?));
    let tid = crash_thread
        .map(|thread| &thread["os_thread_id"])
        .filter(|tid| !tid.is_null())
        .unwrap_or(&report["native_exception"]["os_thread_id"]);
    let pc = report["native_exception"]
        .get("instruction_pointer")
        .filter(|pc| !pc.is_null())
        .or_else(|| {
            crash_thread
                .and_then(|thread| thread["frames"].as_array()?.first())
                .map(|frame| &frame["offset"])
        })
        .unwrap_or(&Value::Null);
    let addresses = &report["fault_addresses"];
    let raw_address = &addresses["raw_memory_address"];
    let mut text = String::new();
    let _ = writeln!(
        text,
        "fatal error: {}\nprocess {} ({})\nmemory fault address={} [{}] pc={} OS TID={}\nsystem={} {} cpu={}\n",
        display(&processor["crash_info"]["type"]),
        display(&snapshot["pid"]),
        display(&snapshot["executable"]),
        display(&raw_address["address"]),
        if raw_address.is_null() {
            "raw unavailable"
        } else {
            "raw"
        },
        display(pc),
        display(tid),
        display(&processor["system_info"]["os"]),
        display(&processor["system_info"]["os_ver"]),
        display(&processor["system_info"]["cpu_arch"])
    );
    if raw_address.is_null() {
        let _ = writeln!(
            text,
            "Raw memory-fault address unavailable: {}",
            display(&addresses["raw_missing_reason"])
        );
    } else {
        let _ = writeln!(
            text,
            "Raw memory-fault address provenance: {}",
            display(&raw_address["provenance"])
        );
        if !raw_address["normalization"].is_null() {
            let _ = writeln!(
                text,
                "Raw address representation: {}",
                display(&raw_address["normalization"])
            );
        }
    }
    if !report["native_exception"].is_null() {
        let native = &report["native_exception"];
        let _ = writeln!(
            text,
            "Raw minidump exception code={} flags={} exception_address={} [{}]",
            display(&native["exception_code"]),
            display(&native["exception_flags"]),
            display(&native["exception_address"]),
            display(&native["exception_address_meaning"])
        );
    }
    if !report["linux_signal_context"].is_null() {
        let signal = &report["linux_signal_context"];
        let _ = writeln!(
            text,
            "Original Linux signal={} code={} errno={} si_addr={} kind={}",
            display(&signal["signal"]),
            display(&signal["code"]),
            display(&signal["errno"]),
            display(&signal["address"]["address"]),
            display(&signal["address"]["kind"])
        );
    }
    if !processor["crash_info"]["address"].is_null() {
        let _ = writeln!(
            text,
            "Raw processor crash_info.address={} [platform-dependent decoder value; not authoritative memory-fault evidence]",
            display(&processor["crash_info"]["address"])
        );
    }
    if !processor["crash_info"]["instruction"].is_null() {
        let _ = writeln!(
            text,
            "Decoded instruction: {}",
            display(&processor["crash_info"]["instruction"])
        );
    }
    for inferred in addresses["instruction_memory_accesses"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let _ = writeln!(
            text,
            "Inferred instruction memory access address={} type={} size={} guard_page_heuristic={} [{}; {}]",
            display(&inferred["address"]),
            display(&inferred["access"]["access_type"]),
            display(&inferred["access"]["size"]),
            display(&inferred["access"]["is_likely_guard_page"]),
            display(&inferred["provenance"]),
            display(&inferred["certainty"])
        );
    }
    let _ = writeln!(
        text,
        "Private raw dump retained: {}\nText report: {}\nStructured report: {}\nWARNING: dump contains sensitive process/stack memory. No automatic upload.\n",
        display(&report["raw_dump"]),
        display(&json!(
            Path::new(report["raw_dump"].as_str().unwrap_or("")).with_extension("txt")
        )),
        display(&json!(
            Path::new(report["raw_dump"].as_str().unwrap_or("")).with_extension("json")
        ))
    );
    if let Some(threads) = threads {
        let order = crash_index
            .and_then(|index| usize::try_from(index).ok())
            .filter(|&index| index < threads.len())
            .into_iter()
            .chain((0..threads.len()).filter(|&index| Some(index as u64) != crash_index));
        for index in order {
            let thread = &threads[index];
            let _ = writeln!(
                text,
                "thread {}{} dump-thread={} name={} [{}]:",
                display(&thread["os_thread_id"]),
                if Some(index as u64) == crash_index {
                    " [CRASHED]"
                } else {
                    ""
                },
                display(&thread["thread_id"]),
                display(&thread["thread_name"]),
                thread_context(thread, snapshot)
            );
            if let Some(frames) = thread["frames"]
                .as_array()
                .filter(|frames| !frames.is_empty())
            {
                let mut number = 0;
                for frame in frames {
                    if let Some(inlines) = frame["inlines"].as_array() {
                        for inline in inlines {
                            render_frame(&mut text, number, inline, frame, true);
                            number += 1;
                        }
                    }
                    render_frame(&mut text, number, frame, frame, false);
                    number += 1;
                    if text.len() >= MAX_TEXT_BYTES {
                        break;
                    }
                }
            } else {
                text.push_str("  <native stack unavailable; no frames invented>\n");
            }
            text.push('\n');
            if text.len() >= MAX_TEXT_BYTES {
                text.push_str(
                    "TRUNCATED native stack text at 8 MiB; consult structured report/raw dump.\n",
                );
                break;
            }
        }
    } else {
        text.push_str("Native threads unavailable.\n\n");
    }
    text.push_str("Managed tasks (framework state only; no private arguments):\n");
    if let Some(tasks) = snapshot["tasks"].as_array() {
        for task in tasks {
            let _ = writeln!(
                text,
                "  task {} extension={} name={} state={}",
                display(&task["id"]),
                display(&task["extension"]),
                display(&task["name"]),
                display(&task["state"])
            );
        }
        if tasks.is_empty() {
            text.push_str("  <none recorded>\n");
        }
    } else {
        text.push_str("  <snapshot unavailable>\n");
    }
    if let Some(diagnostics) = report["diagnostics"].as_array() {
        for diagnostic in diagnostics {
            let _ = writeln!(text, "DIAGNOSTIC: {}", display(diagnostic));
        }
    }
    text
}

struct LimitedBytes {
    bytes: Vec<u8>,
    limit: usize,
}
impl LimitedBytes {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }
}
impl io::Write for LimitedBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("crash report byte limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
