use std::path::{Path, PathBuf};

use wasmtime::Engine;
use xtools_protocol::PluginManifest;

use crate::error::RuntimeError;
use crate::instance::PluginInstance;

#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    pub path: PathBuf,
    pub manifest: PluginManifest,
}

pub struct PluginLoader {
    engine: Engine,
    storage_root: Option<PathBuf>,
}

impl PluginLoader {
    pub fn new() -> Self {
        let engine = Engine::default();
        Self {
            engine,
            storage_root: None,
        }
    }

    pub fn with_storage_root(mut self, root: PathBuf) -> Self {
        self.storage_root = Some(root);
        self
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Fast inspect a .wasm file by loading it, extracting its manifest, and discarding the instance.
    pub fn inspect_file(&self, path: impl AsRef<Path>) -> Result<PluginManifest, RuntimeError> {
        let bytes = std::fs::read(path.as_ref())?;
        let instance = PluginInstance::load(&self.engine, &bytes, self.storage_root.clone())?;
        Ok(instance.manifest().clone())
    }

    /// Scan a directory for all .wasm files and read their manifests.
    pub fn scan_dir(&self, dir: impl AsRef<Path>) -> Vec<DiscoveredPlugin> {
        let mut plugins = Vec::new();
        let read_dir = match std::fs::read_dir(dir.as_ref()) {
            Ok(rd) => rd,
            Err(_) => return plugins,
        };

        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "wasm") {
                match self.inspect_file(&path) {
                    Ok(manifest) => {
                        log::info!("Discovered WASM plugin: {} ({}) at {:?}", manifest.name, manifest.id, path);
                        plugins.push(DiscoveredPlugin { path, manifest });
                    }
                    Err(e) => {
                        log::warn!("Failed to load WASM plugin at {:?}: {e}", path);
                    }
                }
            }
        }

        // Sort by ID or name for stable order
        plugins.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
        plugins
    }

    /// Load a full runnable plugin instance from file.
    pub fn load_instance(&self, path: impl AsRef<Path>) -> Result<PluginInstance, RuntimeError> {
        let bytes = std::fs::read(path.as_ref())?;
        let mut instance = PluginInstance::load(&self.engine, &bytes, self.storage_root.clone())?;
        instance.init()?;
        Ok(instance)
    }
}

impl Default for PluginLoader {
    fn default() -> Self {
        Self::new()
    }
}
