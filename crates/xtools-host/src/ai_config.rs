//! AI 服务配置（~/.config/xtools/plugins/xtools.ai/config.json）的宿主侧读写。
//! 与 WASM 插件（xtools-plugin-ai）共享同一文件，结构保持一致：
//! 多服务商、多模型，外加当前选中的服务商与模型。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// AI 服务商（OpenAI 兼容接口）：名称 + 地址 + 密钥 + 模型列表
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AiProviderEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub models: Vec<String>,
}

/// AI 问答插件存储。base_url/api_key/model 为旧版单服务商字段，
/// 仅用于读取旧配置迁移，不再写入。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AiConfigFile {
    #[serde(default)]
    pub providers: Vec<AiProviderEntry>,
    #[serde(default)]
    pub selected_provider_id: String,
    #[serde(default)]
    pub selected_model: String,
    #[serde(default, skip_serializing)]
    pub base_url: String,
    #[serde(default, skip_serializing)]
    pub api_key: String,
    #[serde(default, skip_serializing)]
    pub model: String,
}

pub(crate) fn new_provider_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let n = millis
        .wrapping_mul(1000)
        .wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed));
    format!("p{n}")
}

impl AiConfigFile {
    /// 迁移旧版单服务商配置并校正失效的选中项。返回 true 表示结构变化（可选择写回）。
    pub fn normalize(&mut self) -> bool {
        let mut changed = false;

        if self.providers.is_empty()
            && (!self.base_url.trim().is_empty()
                || !self.api_key.trim().is_empty()
                || !self.model.trim().is_empty())
        {
            let id = new_provider_id();
            let model = self.model.trim().to_string();
            self.providers.push(AiProviderEntry {
                id: id.clone(),
                name: "默认".to_string(),
                base_url: self.base_url.trim().to_string(),
                api_key: self.api_key.trim().to_string(),
                models: if model.is_empty() {
                    Vec::new()
                } else {
                    vec![model.clone()]
                },
            });
            self.selected_provider_id = id;
            self.selected_model = model;
            self.base_url.clear();
            self.api_key.clear();
            self.model.clear();
            changed = true;
        }

        if self.selected_provider_id.is_empty()
            || !self
                .providers
                .iter()
                .any(|p| p.id == self.selected_provider_id)
        {
            self.selected_provider_id = self
                .providers
                .first()
                .map(|p| p.id.clone())
                .unwrap_or_default();
            self.selected_model.clear();
            changed = true;
        }

        if let Some(provider) = self.selected_provider() {
            if self.selected_model.is_empty()
                || !provider.models.iter().any(|m| *m == self.selected_model)
            {
                self.selected_model = provider.models.first().cloned().unwrap_or_default();
                changed = true;
            }
        }

        changed
    }

    pub fn selected_provider(&self) -> Option<&AiProviderEntry> {
        self.providers
            .iter()
            .find(|p| p.id == self.selected_provider_id)
    }
}

pub fn plugins_root() -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join("xtools").join("plugins"))
        .unwrap_or_else(|| PathBuf::from("storage"))
}

pub fn baidu_config_path() -> PathBuf {
    plugins_root().join("xtools.trans").join("config.json")
}

pub fn ai_config_path() -> PathBuf {
    plugins_root().join("xtools.ai").join("config.json")
}

fn load_json<T: for<'de> Deserialize<'de> + Default>(path: &PathBuf) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub(crate) fn save_json<T: Serialize>(path: &PathBuf, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败: {e}"))?;
    }
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| format!("序列化配置失败: {e}"))?;
    std::fs::write(path, bytes).map_err(|e| format!("写入配置失败: {e}"))
}

#[cfg(test)]
pub(crate) fn load_json_test_helper<T: for<'de> Deserialize<'de> + Default>(path: &PathBuf) -> T {
    load_json(path)
}

#[cfg(test)]
pub(crate) fn save_json_test_helper<T: Serialize>(path: &PathBuf, value: &T) -> Result<(), String> {
    save_json(path, value)
}

/// 智能翻译插件存储（宿主设置窗口使用）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransConfigFile {
    #[serde(default)]
    pub engine_index: usize,
    #[serde(default)]
    pub baidu_appid: String,
    #[serde(default)]
    pub baidu_key: String,
}

/// 读取百度翻译配置（engine_index 保留插件内的选择）
pub fn load_baidu_config() -> TransConfigFile {
    load_json(&baidu_config_path())
}

/// 保存百度翻译 AppID / 密钥，保留现有引擎选择
pub fn save_baidu_config(appid: &str, key: &str) -> Result<(), String> {
    let mut config: TransConfigFile = load_json(&baidu_config_path());
    config.baidu_appid = appid.trim().to_string();
    config.baidu_key = key.trim().to_string();
    save_json(&baidu_config_path(), &config)
}

/// 读取 AI 多服务商配置（含旧格式迁移）
pub fn load_ai_config() -> AiConfigFile {
    let mut config: AiConfigFile = load_json(&ai_config_path());
    config.normalize();
    config
}

/// 保存 AI 多服务商配置
pub fn save_ai_config(config: &AiConfigFile) -> Result<(), String> {
    save_json(&ai_config_path(), config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legacy_ai_config_migration() {
        let raw = r#"{"base_url": " https://api.old.com/v1 ", "api_key": " sk-old ", "model": "gpt-4o-mini"}"#;
        let mut config: AiConfigFile = serde_json::from_str(raw).unwrap();
        assert!(config.normalize(), "首次迁移应发生变化");
        assert_eq!(config.providers.len(), 1);
        assert_eq!(config.providers[0].name, "默认");
        assert_eq!(config.providers[0].models, vec!["gpt-4o-mini"]);
        assert_eq!(config.selected_model, "gpt-4o-mini");
        assert!(!config.normalize(), "重复迁移应无变化");
    }

    #[test]
    fn test_provider_id_unique() {
        assert_ne!(new_provider_id(), new_provider_id());
    }
}
