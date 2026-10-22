//! Cheap binary interface inspection, before creating an engine or scheduling JIT.
//! Function bodies are deliberately left to Wasmtime's validator/compiler.
use atri_core::{ApiError, wasm_abi};
use wasmparser::{
    ComponentExternalKind, ComponentTypeRef, CompositeInnerType, Encoding, Export, ExternalKind,
    FuncType, MemoryType, Parser, Payload, TypeRef, ValType,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Backend {
    Core,
    Component,
}

const PLUGIN: &str = "atri:extension/plugin@1.0.0";
const HOST: &str = "atri:extension/host@1.0.0";
const CLOCK: &str = "wasi:clocks/monotonic-clock@0.3.0";
const CLOCK_TYPES: &str = "wasi:clocks/types@0.3.0";
const PACKAGE_HELP: &str = "Package the SDK's intermediate core file as a P3 component: cargo run --manifest-path atri_bot/Cargo.toml -p atri_extensions --features component-tool --bin atri-component -- <input.wasm> <output-component.wasm>; deploy only <output-component.wasm>. For an intentional standalone core extension, rebuild with the SDK's explicit core-module feature instead.";
const CORE_EXPORTS: [(&str, &[ValType], &[ValType]); 4] = [
    ("atri_abi_version", &[], &[ValType::I32]),
    ("atri_alloc", &[ValType::I32], &[ValType::I32]),
    ("atri_free", &[ValType::I32, ValType::I32], &[]),
    (
        "atri_invoke",
        &[ValType::I32, ValType::I32, ValType::I32],
        &[ValType::I64],
    ),
];

fn malformed(error: impl std::fmt::Display) -> ApiError {
    ApiError::new("wasm_format", error.to_string())
}
fn core_abi(message: impl std::fmt::Display) -> ApiError {
    ApiError::new("wasm_core_abi", format!("{message}. {PACKAGE_HELP}"))
}
fn wasi(name: &str) -> ApiError {
    ApiError::new(
        "wasm_wasi_unsupported",
        format!(
            "unsupported WASI import {name:?}; WASI is available only in P3 components, and this host links only {CLOCK} (no Preview 1/2, filesystem, or networking)"
        ),
    )
}
fn is_wasi(name: &str) -> bool {
    name.starts_with("wasi")
}
fn signature<'a>(
    types: &'a [Option<FuncType>],
    functions: &[u32],
    index: u32,
) -> Result<&'a FuncType, ApiError> {
    functions
        .get(index as usize)
        .and_then(|index| types.get(*index as usize))
        .and_then(Option::as_ref)
        .ok_or_else(|| {
            malformed(format!(
                "function {index} has an invalid function type index"
            ))
        })
}
fn matches_signature(ty: &FuncType, params: &[ValType], results: &[ValType]) -> bool {
    ty.params() == params && ty.results() == results
}

pub(super) fn inspect(source: &[u8]) -> Result<Backend, ApiError> {
    let mut backend = None;
    let mut depth = 0_u32;
    let mut types = Vec::new();
    let mut functions = Vec::new();
    let mut memories: Vec<MemoryType> = Vec::new();
    let mut imports = Vec::new();
    let mut exports: Vec<Export<'_>> = Vec::new();
    let mut component_ready = false;
    let mut plugin = false;
    for payload in Parser::new(0).parse_all(source) {
        let payload = payload.map_err(malformed)?;
        match payload {
            Payload::Version { encoding, .. } => {
                if depth == 0 {
                    backend = Some(match encoding {
                        Encoding::Module => Backend::Core,
                        Encoding::Component => Backend::Component,
                    });
                }
                depth += 1;
                continue;
            }
            Payload::End(_) => {
                depth -= 1;
                continue;
            }
            _ => {}
        }
        // Embedded core modules are implementation details of a component,
        // not standalone extensions and not direct host imports.
        if depth != 1 {
            continue;
        }
        match (backend, payload) {
            (Some(Backend::Core), Payload::TypeSection(section)) => {
                for group in section {
                    for ty in group.map_err(malformed)?.into_types() {
                        types.push(match ty.composite_type.inner {
                            CompositeInnerType::Func(ty) => Some(ty),
                            _ => None,
                        });
                    }
                }
            }
            (Some(Backend::Core), Payload::ImportSection(section)) => {
                for import in section.into_imports() {
                    let import = import.map_err(malformed)?;
                    match import.ty {
                        TypeRef::Func(index) | TypeRef::FuncExact(index) => functions.push(index),
                        TypeRef::Memory(ty) => memories.push(ty),
                        _ => {}
                    }
                    component_ready |= import.module.starts_with("atri:extension/");
                    imports.push(import);
                }
            }
            (Some(Backend::Core), Payload::FunctionSection(section)) => {
                for index in section {
                    functions.push(index.map_err(malformed)?);
                }
            }
            (Some(Backend::Core), Payload::MemorySection(section)) => {
                for memory in section {
                    memories.push(memory.map_err(malformed)?);
                }
            }
            (Some(Backend::Core), Payload::ExportSection(section)) => {
                for export in section {
                    let export = export.map_err(malformed)?;
                    component_ready |= export.name.starts_with("cabi_")
                        || export.name.contains("atri:extension/plugin@");
                    exports.push(export);
                }
            }
            (Some(Backend::Core), Payload::CustomSection(section)) => {
                component_ready |= section.name().starts_with("component-type");
            }
            (Some(Backend::Component), Payload::ComponentImportSection(section)) => {
                for import in section {
                    let import = import.map_err(malformed)?;
                    let name = import.name.full_name();
                    if is_wasi(&name) && name != CLOCK && name != CLOCK_TYPES {
                        return Err(wasi(&name));
                    }
                    if name != HOST && name != CLOCK && name != CLOCK_TYPES {
                        return Err(ApiError::new(
                            "wasm_imports",
                            format!(
                                "unsupported component import {name:?}; supported imports are {HOST}, {CLOCK} and {CLOCK_TYPES}"
                            ),
                        ));
                    }
                    if !matches!(import.ty, ComponentTypeRef::Instance(_)) {
                        return Err(ApiError::new(
                            "wasm_component_abi",
                            format!("component import {name:?} must be an interface instance"),
                        ));
                    }
                }
            }
            (Some(Backend::Component), Payload::ComponentExportSection(section)) => {
                for export in section {
                    let export = export.map_err(malformed)?;
                    if export.name.full_name() == PLUGIN {
                        if export.kind != ComponentExternalKind::Instance {
                            return Err(ApiError::new(
                                "wasm_component_abi",
                                format!("component export {PLUGIN} must be an interface instance"),
                            ));
                        }
                        plugin = true;
                    }
                }
            }
            (_, Payload::UnknownSection { id, .. }) => {
                return Err(malformed(format!("unknown WebAssembly section {id}")));
            }
            _ => {}
        }
    }
    let backend = backend.ok_or_else(|| malformed("missing WebAssembly header"))?;
    if backend == Backend::Component {
        if !plugin {
            return Err(ApiError::new(
                "wasm_component_abi",
                format!("component must export the {PLUGIN} interface instance"),
            ));
        }
        return Ok(backend);
    }
    let complete_core = exports.iter().any(|e| e.name == "memory")
        && CORE_EXPORTS
            .iter()
            .all(|(name, _, _)| exports.iter().any(|e| e.name == *name));
    if component_ready && !complete_core {
        return Err(ApiError::new(
            "wasm_component_required",
            format!(
                "component-ready core module is not a standalone Atri core-ABI extension. {PACKAGE_HELP}"
            ),
        ));
    }
    for import in imports {
        if is_wasi(import.module) {
            return Err(wasi(import.module));
        }
        let expected: Option<(&[ValType], &[ValType])> = match import.name {
            "call" => Some((&[ValType::I32, ValType::I32], &[ValType::I32])),
            "response_len" => Some((&[ValType::I32], &[ValType::I32])),
            "response_read" => Some((&[ValType::I32, ValType::I32, ValType::I32], &[ValType::I32])),
            "response_drop" => Some((&[ValType::I32], &[])),
            _ => None,
        };
        let Some((params, results)) = expected.filter(|_| import.module == wasm_abi::IMPORT_MODULE)
        else {
            return Err(ApiError::new(
                "wasm_imports",
                format!(
                    "unsupported core import {}.{}; only atri_v1.call/response_len/response_read/response_drop are available",
                    import.module, import.name
                ),
            ));
        };
        let TypeRef::Func(index) = import.ty else {
            return Err(core_abi(format!(
                "core import atri_v1.{} must be a function",
                import.name
            )));
        };
        let ty = types
            .get(index as usize)
            .and_then(Option::as_ref)
            .ok_or_else(|| malformed("core import has an invalid function type index"))?;
        if !matches_signature(ty, params, results) {
            return Err(core_abi(format!(
                "core import atri_v1.{} has type {ty}, expected {params:?} -> {results:?}",
                import.name
            )));
        }
    }
    for (name, params, results) in CORE_EXPORTS {
        let export = exports
            .iter()
            .find(|e| e.name == name)
            .ok_or_else(|| core_abi(format!("missing core export {name}")))?;
        if export.kind != ExternalKind::Func {
            return Err(core_abi(format!("core export {name} must be a function")));
        }
        let ty = signature(&types, &functions, export.index)?;
        if !matches_signature(ty, params, results) {
            return Err(core_abi(format!(
                "core export {name} has type {ty}, expected {params:?} -> {results:?}"
            )));
        }
    }
    let export = exports
        .iter()
        .find(|e| e.name == "memory")
        .ok_or_else(|| core_abi("missing core export memory"))?;
    if export.kind != ExternalKind::Memory {
        return Err(core_abi("core export memory must be a memory"));
    }
    let memory = memories
        .get(export.index as usize)
        .ok_or_else(|| malformed("core export memory has an invalid memory index"))?;
    if memory.memory64 || memory.shared {
        return Err(core_abi(
            "core export memory must be unshared 32-bit linear memory",
        ));
    }
    Ok(backend)
}

#[cfg(test)]
#[path = "wasm_preflight_tests.rs"]
mod tests;
