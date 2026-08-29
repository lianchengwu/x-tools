use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("Wasmtime execution error: {0}")]
    Wasmtime(#[from] wasmtime::Error),

    #[error("Serialization / JSON error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Required WASM export function '{0}' not found in module")]
    ExportNotFound(&'static str),

    #[error("WASM linear memory export 'memory' not found")]
    MemoryNotFound,

    #[error("Plugin initialization failed: {0}")]
    InitFailed(String),

    #[error("Memory access error: {0}")]
    MemoryAccess(#[from] wasmtime::MemoryAccessError),

    #[error("Plugin error: {0}")]
    PluginError(String),
    #[error("Permission denied for capability: {0}")]
    PermissionDenied(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid memory pointer or length returned from WASM: ptr={0:#x}, len={1}")]
    InvalidMemory(u32, u32),
}
