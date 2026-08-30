use std::path::PathBuf;

use wasmtime::{Engine, Instance, Linker, Module, Store};
use xtools_protocol::*;

use crate::error::RuntimeError;
use crate::host_env::{HostContext, register_host_functions};

pub struct PluginInstance {
    store: Store<HostContext>,
    instance: Instance,
    manifest: PluginManifest,
}

impl PluginInstance {
    /// Load and instantiate a WASM plugin from binary bytes.
    pub fn load(
        engine: &Engine,
        wasm_bytes: &[u8],
        storage_root: Option<PathBuf>,
    ) -> Result<Self, RuntimeError> {
        let module = Module::from_binary(engine, wasm_bytes)?;
        let mut linker = Linker::new(engine);
        register_host_functions(&mut linker)?;

        let temp_id = "loading".to_string();
        let storage_root = storage_root.unwrap_or_else(|| std::env::temp_dir().join("xtools-storage"));

        let mut store = Store::new(engine, HostContext::new(temp_id, storage_root.clone()));
        let instance = linker.instantiate(&mut store, &module)?;

        // Read manifest
        let manifest_func = instance
            .get_func(&mut store, EXPORT_MANIFEST)
            .ok_or(RuntimeError::ExportNotFound(EXPORT_MANIFEST))?;
        let manifest_typed = manifest_func.typed::<(), u64>(&store)?;
        let packed = manifest_typed.call(&mut store, ())?;

        let (ptr, len) = unpack_ptr_len(packed);
        let manifest: PluginManifest = Self::read_and_dealloc(&mut store, &instance, ptr, len)?;

        store.data_mut().plugin_id = manifest.id.clone();

        Ok(Self {
            store,
            instance,
            manifest,
        })
    }

    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    /// Initialize the plugin instance inside WASM
    pub fn init(&mut self) -> Result<(), RuntimeError> {
        let init_func = self
            .instance
            .get_func(&mut self.store, EXPORT_INIT)
            .ok_or(RuntimeError::ExportNotFound(EXPORT_INIT))?;
        let init_typed = init_func.typed::<(), u64>(&self.store)?;
        let packed = init_typed.call(&mut self.store, ())?;

        let (ptr, len) = unpack_ptr_len(packed);
        let res: Result<(), String> =
            Self::read_and_dealloc(&mut self.store, &self.instance, ptr, len)?;

        res.map_err(RuntimeError::InitFailed)
    }

    /// Render the current UI tree from the plugin
    pub fn render(&mut self) -> Result<UiView, RuntimeError> {
        let render_func = self
            .instance
            .get_func(&mut self.store, EXPORT_RENDER)
            .ok_or(RuntimeError::ExportNotFound(EXPORT_RENDER))?;
        let render_typed = render_func.typed::<(), u64>(&self.store)?;
        let packed = render_typed.call(&mut self.store, ())?;

        let (ptr, len) = unpack_ptr_len(packed);
        Self::read_and_dealloc(&mut self.store, &self.instance, ptr, len)
    }

    /// Dispatch a UI event to the plugin and get the response
    pub fn handle_event(&mut self, event: &UiEvent) -> Result<UiResponse, RuntimeError> {
        let event_bytes = serde_json::to_vec(event)?;

        // Allocate memory in WASM for input event
        let alloc_func = self
            .instance
            .get_func(&mut self.store, EXPORT_ALLOC)
            .ok_or(RuntimeError::ExportNotFound(EXPORT_ALLOC))?;
        let alloc_typed = alloc_func.typed::<u32, u32>(&self.store)?;
        let in_len = event_bytes.len() as u32;
        let in_ptr = alloc_typed.call(&mut self.store, in_len)?;

        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .ok_or(RuntimeError::MemoryNotFound)?;
        memory.write(&mut self.store, in_ptr as usize, &event_bytes)?;

        // Call event handler
        let handle_func = self
            .instance
            .get_func(&mut self.store, EXPORT_HANDLE_EVENT)
            .ok_or(RuntimeError::ExportNotFound(EXPORT_HANDLE_EVENT))?;
        let handle_typed = handle_func.typed::<(u32, u32), u64>(&self.store)?;
        let packed = handle_typed.call(&mut self.store, (in_ptr, in_len))?;

        // Deallocate input event buffer
        if let Some(dealloc_func) = self.instance.get_func(&mut self.store, EXPORT_DEALLOC) {
            if let Ok(dealloc_typed) = dealloc_func.typed::<(u32, u32), ()>(&self.store) {
                let _ = dealloc_typed.call(&mut self.store, (in_ptr, in_len));
            }
        }

        let (out_ptr, out_len) = unpack_ptr_len(packed);
        Self::read_and_dealloc(&mut self.store, &self.instance, out_ptr, out_len)
    }

    /// Internal helper to read JSON data from WASM memory and trigger deallocation
    fn read_and_dealloc<T: serde::de::DeserializeOwned>(
        store: &mut Store<HostContext>,
        instance: &Instance,
        ptr: u32,
        len: u32,
    ) -> Result<T, RuntimeError> {
        if ptr == 0 || len == 0 {
            return Err(RuntimeError::InvalidMemory(ptr, len));
        }

        let memory = instance
            .get_memory(&mut *store, "memory")
            .ok_or(RuntimeError::MemoryNotFound)?;

        let mut buf = vec![0u8; len as usize];
        memory.read(&*store, ptr as usize, &mut buf)?;

        // Deallocate the return buffer inside WASM
        if let Some(dealloc_func) = instance.get_func(&mut *store, EXPORT_DEALLOC) {
            if let Ok(dealloc_typed) = dealloc_func.typed::<(u32, u32), ()>(&*store) {
                let _ = dealloc_typed.call(&mut *store, (ptr, len));
            }
        }
        let obj = serde_json::from_slice(&buf)?;
        Ok(obj)
    }
}
