//! 窗口外观偏好：~/.config/xtools/window.json
//! 目前包含工具窗口整体透明度，供设置窗口调节、各窗口启动时应用。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const MIN_OPACITY: f32 = 0.3;
pub const DEFAULT_OPACITY: f32 = 1.0;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WindowPrefs {
    /// 窗口整体不透明度（0.3–1.0）
    #[serde(default = "default_opacity")]
    pub opacity: f32,
}

fn default_opacity() -> f32 {
    DEFAULT_OPACITY
}

impl Default for WindowPrefs {
    fn default() -> Self {
        Self {
            opacity: DEFAULT_OPACITY,
        }
    }
}

impl WindowPrefs {
    pub fn normalized_opacity(&self) -> f32 {
        self.opacity.clamp(MIN_OPACITY, 1.0)
    }
}

fn prefs_path() -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join("xtools").join("window.json"))
        .unwrap_or_else(|| PathBuf::from("window.json"))
}

pub fn load() -> WindowPrefs {
    std::fs::read(prefs_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_opacity(opacity: f32) -> Result<(), String> {
    let prefs = WindowPrefs {
        opacity: opacity.clamp(MIN_OPACITY, 1.0),
    };
    let path = prefs_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败: {e}"))?;
    }
    let bytes = serde_json::to_vec_pretty(&prefs).map_err(|e| format!("序列化配置失败: {e}"))?;
    std::fs::write(&path, bytes).map_err(|e| format!("写入配置失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opacity_clamped() {
        let prefs = WindowPrefs {
            opacity: 0.05,
        };
        assert!((prefs.normalized_opacity() - MIN_OPACITY).abs() < 1e-6);
        let prefs = WindowPrefs { opacity: 1.7 };
        assert!((prefs.normalized_opacity() - 1.0).abs() < 1e-6);
    }
}
