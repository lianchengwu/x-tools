//! 小工具全局快捷键：组合键规范化、平台转换、配置读写。
//! 设置进程和宿主进程共用这一份纯逻辑，不碰门户、不建窗口。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MOD_ALT: u32 = 0x0001;
const MOD_CONTROL: u32 = 0x0002;
const MOD_SHIFT: u32 = 0x0004;
const MOD_WIN: u32 = 0x0008;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedKey {
    Space,
    Tab,
    Return,
    BackSpace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Left,
    Right,
    Up,
    Down,
}

impl NamedKey {
    fn canonical(self) -> &'static str {
        match self {
            Self::Space => "Space",
            Self::Tab => "Tab",
            Self::Return => "Return",
            Self::BackSpace => "BackSpace",
            Self::Delete => "Delete",
            Self::Insert => "Insert",
            Self::Home => "Home",
            Self::End => "End",
            Self::PageUp => "Page_Up",
            Self::PageDown => "Page_Down",
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Up => "Up",
            Self::Down => "Down",
        }
    }

    fn keysym(self) -> &'static str {
        match self {
            Self::Space => "space",
            other => other.canonical(),
        }
    }

    fn vk(self) -> u32 {
        match self {
            Self::Space => 0x20,
            Self::Tab => 0x09,
            Self::Return => 0x0D,
            Self::BackSpace => 0x08,
            Self::Delete => 0x2E,
            Self::Insert => 0x2D,
            Self::Home => 0x24,
            Self::End => 0x23,
            Self::PageUp => 0x21,
            Self::PageDown => 0x22,
            Self::Left => 0x25,
            Self::Right => 0x27,
            Self::Up => 0x26,
            Self::Down => 0x28,
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "space" => Some(Self::Space),
            "tab" => Some(Self::Tab),
            "return" => Some(Self::Return),
            "backspace" => Some(Self::BackSpace),
            "delete" => Some(Self::Delete),
            "insert" => Some(Self::Insert),
            "home" => Some(Self::Home),
            "end" => Some(Self::End),
            "page_up" | "pageup" => Some(Self::PageUp),
            "page_down" | "pagedown" => Some(Self::PageDown),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "up" => Some(Self::Up),
            "down" => Some(Self::Down),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Letter(char),
    Digit(char),
    Function(u8),
    Named(NamedKey),
}

impl Key {
    fn canonical(self) -> String {
        match self {
            Self::Letter(ch) => ch.to_ascii_uppercase().to_string(),
            Self::Digit(ch) => ch.to_string(),
            Self::Function(n) => format!("F{n}"),
            Self::Named(named) => named.canonical().to_string(),
        }
    }

    fn keysym(self) -> String {
        match self {
            Self::Letter(ch) => ch.to_ascii_lowercase().to_string(),
            Self::Digit(ch) => ch.to_string(),
            Self::Function(n) => format!("F{n}"),
            Self::Named(named) => named.keysym().to_string(),
        }
    }

    fn vk(self) -> u32 {
        match self {
            Self::Letter(ch) => ch.to_ascii_uppercase() as u32,
            Self::Digit(ch) => ch as u32,
            Self::Function(n) => 0x70 + u32::from(n - 1),
            Self::Named(named) => named.vk(),
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        let lower = raw.to_ascii_lowercase();
        if lower == "escape" || lower == "esc" {
            return None;
        }
        if raw.chars().count() == 1 {
            let ch = raw.chars().next()?;
            if ch.is_ascii_alphabetic() {
                return Some(Self::Letter(ch.to_ascii_uppercase()));
            }
            if ch.is_ascii_digit() {
                return Some(Self::Digit(ch));
            }
            return None;
        }
        if let Some(rest) = lower.strip_prefix('f') {
            if let Ok(n) = rest.parse::<u8>() {
                if (1..=12).contains(&n) {
                    return Some(Self::Function(n));
                }
            }
        }
        NamedKey::parse(&lower).map(Self::Named)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accel {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
    pub key: Key,
}

impl Accel {
    pub fn parse(raw: &str) -> Result<Self, &'static str> {
        parse_parts(raw)
    }

    pub fn parse_xdg(raw: &str) -> Result<Self, &'static str> {
        parse_parts(raw)
    }

    pub fn format_canonical(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.super_key {
            parts.push("Super");
        }
        let key = self.key.canonical();
        parts.push(&key);
        parts.join("+")
    }

    pub fn to_xdg_trigger(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("CTRL".to_string());
        }
        if self.alt {
            parts.push("ALT".to_string());
        }
        if self.shift {
            parts.push("SHIFT".to_string());
        }
        if self.super_key {
            parts.push("LOGO".to_string());
        }
        parts.push(self.key.keysym());
        parts.join("+")
    }

    /// Windows `RegisterHotKey` 的 `(modifiers, vk)`。全平台可测，不调用系统 API。
    pub fn to_win_hotkey(&self) -> (u32, u32) {
        let mut mods = 0;
        if self.alt {
            mods |= MOD_ALT;
        }
        if self.ctrl {
            mods |= MOD_CONTROL;
        }
        if self.shift {
            mods |= MOD_SHIFT;
        }
        if self.super_key {
            mods |= MOD_WIN;
        }
        (mods, self.key.vk())
    }
}

fn parse_parts(raw: &str) -> Result<Accel, &'static str> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("空快捷键");
    }
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut super_key = false;
    let mut key = None;
    for part in raw.split('+') {
        let token = part.trim();
        if token.is_empty() {
            return Err("空片段");
        }
        let lower = token.to_ascii_lowercase();
        let is_mod = match lower.as_str() {
            "ctrl" | "control" => {
                ctrl = true;
                true
            }
            "alt" => {
                alt = true;
                true
            }
            "shift" => {
                shift = true;
                true
            }
            "super" | "meta" | "win" | "logo" => {
                super_key = true;
                true
            }
            _ => false,
        };
        if is_mod {
            continue;
        }
        if key.is_some() {
            return Err("多个主键");
        }
        key = Some(Key::parse(token).ok_or("主键不在闭集")?);
    }
    if !ctrl && !alt && !super_key {
        return Err("必须包含 Ctrl、Alt 或 Super");
    }
    Ok(Accel {
        ctrl,
        alt,
        shift,
        super_key,
        key: key.ok_or("缺少主键")?,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutBinding {
    pub plugin_id: String,
    pub accel: String,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutFile {
    #[serde(default)]
    pub bindings: Vec<ShortcutBinding>,
}

impl ShortcutFile {
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .map(|p| p.join("xtools").join("shortcuts.json"))
            .unwrap_or_else(|| PathBuf::from("shortcuts.json"))
    }

    pub fn load() -> Self {
        Self::load_from(&Self::path())
    }

    pub fn load_from(path: &Path) -> Self {
        let Ok(bytes) = std::fs::read(path) else {
            return Self::default();
        };
        #[derive(Deserialize)]
        struct RawFile {
            #[serde(default)]
            bindings: Vec<RawBinding>,
        }
        #[derive(Deserialize)]
        struct RawBinding {
            #[serde(default)]
            plugin_id: String,
            #[serde(default)]
            accel: String,
            #[serde(default)]
            enabled: bool,
        }
        let Ok(raw) = serde_json::from_slice::<RawFile>(&bytes) else {
            log::warn!("快捷键配置损坏，忽略: {}", path.display());
            return Self::default();
        };
        let mut bindings = Vec::new();
        for entry in raw.bindings {
            if entry.plugin_id.is_empty() {
                continue;
            }
            if bindings
                .iter()
                .any(|b: &ShortcutBinding| b.plugin_id == entry.plugin_id)
            {
                continue;
            }
            let Ok(accel) = Accel::parse(&entry.accel).or_else(|_| Accel::parse_xdg(&entry.accel))
            else {
                log::warn!("跳过非法快捷键 {} ({})", entry.plugin_id, entry.accel);
                continue;
            };
            bindings.push(ShortcutBinding {
                plugin_id: entry.plugin_id,
                accel: accel.format_canonical(),
                enabled: entry.enabled,
            });
        }
        Self { bindings }
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_to(&Self::path())
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败: {e}"))?;
            }
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| format!("序列化快捷键失败: {e}"))?;
        std::fs::write(path, bytes).map_err(|e| format!("写入快捷键失败: {e}"))
    }

    pub fn active_bindings<'a>(&'a self, discovered_ids: &[String]) -> Vec<&'a ShortcutBinding> {
        let mut seen_accel = Vec::new();
        let mut out = Vec::new();
        for binding in &self.bindings {
            if !binding.enabled || !discovered_ids.iter().any(|id| id == &binding.plugin_id) {
                continue;
            }
            let Ok(accel) = Accel::parse(&binding.accel) else {
                continue;
            };
            let canonical = accel.format_canonical();
            if seen_accel.iter().any(|existing| existing == &canonical) {
                log::warn!("跳过重复快捷键 {} ({canonical})", binding.plugin_id);
                continue;
            }
            seen_accel.push(canonical);
            out.push(binding);
        }
        out
    }

    /// 用当前可见插件行替换同 id 绑定，未见 id 原样保留。
    pub fn merge_visible(&mut self, visible: &[ShortcutBinding]) {
        let mut next = Vec::new();
        for binding in &self.bindings {
            if visible.iter().any(|row| row.plugin_id == binding.plugin_id) {
                continue;
            }
            next.push(binding.clone());
        }
        next.extend(visible.iter().cloned());
        self.bindings = next;
    }
}

pub fn bindings_equal(a: &[ShortcutBinding], b: &[ShortcutBinding]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).all(|(left, right)| {
        left.plugin_id == right.plugin_id
            && left.enabled == right.enabled
            && same_accel(&left.accel, &right.accel)
    })
}

fn same_accel(left: &str, right: &str) -> bool {
    match (Accel::parse(left), Accel::parse(right)) {
        (Ok(a), Ok(b)) => a == b,
        _ => left == right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_accel_parse_and_canonical_format() {
        let a = Accel::parse("alt+ctrl+t").expect("parse");
        assert_eq!(a.format_canonical(), "Ctrl+Alt+T");
        let a2 = Accel::parse("Super+Shift+F5").expect("parse super shift");
        assert_eq!(a2.format_canonical(), "Shift+Super+F5");
    }

    #[test]
    fn test_accel_rejects_invalid_inputs() {
        assert!(Accel::parse("").is_err());
        assert!(Accel::parse("T").is_err());
        assert!(Accel::parse("Shift+T").is_err());
        assert!(Accel::parse("Ctrl").is_err());
        assert!(Accel::parse("Ctrl+Alt").is_err());
        assert!(Accel::parse("Ctrl+Escape").is_err());
        assert!(Accel::parse("Ctrl+Alt+;").is_err());
    }

    #[test]
    fn test_named_keys_round_trip() {
        for raw in [
            "Ctrl+Space",
            "Alt+Tab",
            "Super+Return",
            "Ctrl+BackSpace",
            "Ctrl+Delete",
            "Ctrl+Insert",
            "Ctrl+Home",
            "Ctrl+End",
            "Ctrl+Page_Up",
            "Ctrl+Page_Down",
            "Ctrl+Left",
            "Ctrl+Right",
            "Ctrl+Up",
            "Ctrl+Down",
            "Ctrl+0",
            "Ctrl+F12",
        ] {
            let accel = Accel::parse(raw).unwrap_or_else(|_| panic!("parse {raw}"));
            assert_eq!(accel.format_canonical(), raw);
        }
    }

    #[test]
    fn test_xdg_trigger_format() {
        let a = Accel::parse("Ctrl+Alt+T").unwrap();
        assert_eq!(a.to_xdg_trigger(), "CTRL+ALT+t");
        let a2 = Accel::parse("Super+Space").unwrap();
        assert_eq!(a2.to_xdg_trigger(), "LOGO+space");
        assert_eq!(
            Accel::parse_xdg("CTRL+ALT+t").unwrap().format_canonical(),
            "Ctrl+Alt+T"
        );
        assert_eq!(
            Accel::parse_xdg("LOGO+space").unwrap().format_canonical(),
            "Super+Space"
        );
    }

    #[test]
    fn test_win_hotkey_modifiers_and_vk() {
        let (mods, vk) = Accel::parse("Ctrl+Alt+T").unwrap().to_win_hotkey();
        assert_eq!(vk, b'T' as u32);
        assert_eq!(mods & 0x0002, 0x0002);
        assert_eq!(mods & 0x0001, 0x0001);
        let (mods, vk) = Accel::parse("Ctrl+Page_Up").unwrap().to_win_hotkey();
        assert_eq!(vk, 0x21);
        assert_eq!(mods, 0x0002);
    }

    #[test]
    fn test_shortcut_file_active_bindings_filtering() {
        let file = ShortcutFile {
            bindings: vec![
                ShortcutBinding {
                    plugin_id: "p1".into(),
                    accel: "Ctrl+Alt+T".into(),
                    enabled: true,
                },
                ShortcutBinding {
                    plugin_id: "p2".into(),
                    accel: "Ctrl+Alt+J".into(),
                    enabled: false,
                },
                ShortcutBinding {
                    plugin_id: "p3".into(),
                    accel: "invalid".into(),
                    enabled: true,
                },
                ShortcutBinding {
                    plugin_id: "p4".into(),
                    accel: "Ctrl+Alt+T".into(),
                    enabled: true,
                },
                ShortcutBinding {
                    plugin_id: "p_missing".into(),
                    accel: "Ctrl+Alt+M".into(),
                    enabled: true,
                },
            ],
        };
        let discovered = vec!["p1".into(), "p2".into(), "p3".into(), "p4".into()];
        let active = file.active_bindings(&discovered);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].plugin_id, "p1");
    }

    #[test]
    fn test_merge_visible_preserves_unseen_ids() {
        let mut file = ShortcutFile {
            bindings: vec![
                ShortcutBinding {
                    plugin_id: "p_unseen".into(),
                    accel: "Ctrl+Alt+U".into(),
                    enabled: true,
                },
                ShortcutBinding {
                    plugin_id: "p_seen".into(),
                    accel: "Ctrl+Alt+S".into(),
                    enabled: false,
                },
            ],
        };
        let visible = vec![ShortcutBinding {
            plugin_id: "p_seen".into(),
            accel: "Ctrl+Alt+X".into(),
            enabled: true,
        }];
        file.merge_visible(&visible);
        assert_eq!(file.bindings.len(), 2);
        assert_eq!(file.bindings[0].plugin_id, "p_unseen");
        assert_eq!(file.bindings[0].enabled, true);
        assert_eq!(file.bindings[1].plugin_id, "p_seen");
        assert_eq!(file.bindings[1].accel, "Ctrl+Alt+X");
        assert_eq!(file.bindings[1].enabled, true);
    }

    #[test]
    fn test_bindings_equal_order_sensitive() {
        let a = vec![ShortcutBinding {
            plugin_id: "1".into(),
            accel: "Ctrl+Alt+A".into(),
            enabled: true,
        }];
        let b = vec![ShortcutBinding {
            plugin_id: "1".into(),
            accel: "Ctrl+Alt+A".into(),
            enabled: true,
        }];
        assert!(bindings_equal(&a, &b));
        let c = vec![ShortcutBinding {
            plugin_id: "1".into(),
            accel: "Ctrl+Alt+A".into(),
            enabled: false,
        }];
        assert!(!bindings_equal(&a, &c));
        let swapped_fields = vec![ShortcutBinding {
            plugin_id: "1".into(),
            accel: "ctrl+alt+a".into(),
            enabled: true,
        }];
        assert!(bindings_equal(&a, &swapped_fields));
    }

    #[test]
    fn test_load_roundtrip_and_enabled_default() {
        let dir = std::env::temp_dir().join(format!(
            "xtools-shortcuts-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("shortcuts.json");
        assert!(ShortcutFile::load_from(&path).bindings.is_empty());
        assert!(!path.exists(), "missing file must not be created");

        fs::write(&path, b"{not json").unwrap();
        assert!(ShortcutFile::load_from(&path).bindings.is_empty());
        assert_eq!(fs::read(&path).unwrap(), b"{not json");

        let raw = br#"{"bindings":[
            {"plugin_id":"a","accel":"alt+ctrl+t"},
            {"plugin_id":"a","accel":"Ctrl+Alt+J","enabled":true},
            {"accel":"Ctrl+Alt+K","enabled":true},
            {"plugin_id":"bad","accel":"nope","enabled":true},
            {"plugin_id":"b","accel":"Super+Space","enabled":true}
        ]}"#;
        fs::write(&path, raw).unwrap();
        let before = fs::read(&path).unwrap();
        let loaded = ShortcutFile::load_from(&path);
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(loaded.bindings.len(), 2);
        assert_eq!(loaded.bindings[0].plugin_id, "a");
        assert_eq!(loaded.bindings[0].accel, "Ctrl+Alt+T");
        assert!(!loaded.bindings[0].enabled);
        assert_eq!(loaded.bindings[1].plugin_id, "b");
        assert!(loaded.bindings[1].enabled);

        loaded.save_to(&path).unwrap();
        let again = ShortcutFile::load_from(&path);
        assert!(bindings_equal(&loaded.bindings, &again.bindings));
        let _ = fs::remove_dir_all(&dir);
    }
}
