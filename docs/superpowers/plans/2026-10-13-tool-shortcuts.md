# 小工具全局快捷键实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 xtools 的各 WASM 小工具提供全局快捷键功能，在设置窗口中为每个已发现插件录制组合键并显式开启，由常驻宿主在 Wayland 或 Windows 上向系统注册，按下时直接前置或打开对应工具窗口。

**Architecture:** 将核心逻辑解耦为纯 Rust 模块 `shortcuts.rs`（负责组合键解析、规范化、平台按键语法转换、JSON 持久化、合并与可注册过滤），与 UI/平台完全正交；设置窗口作为独立进程仅负责编辑配置与录制按键，通过防抖写回 `shortcuts.json`；常驻悬浮球宿主通过 mtime 监听文件变化，仅在存在已开启项时按平台向 Wayland XDG Desktop Portal 或 Windows `RegisterHotKey` 注册；激活时复用单例拉起路径，并透传 Wayland 激活令牌。

**Tech Stack:** Rust 2024, Slint 1.17, GTK4, ashpd 0.11 (GlobalShortcuts portal on Wayland), windows-sys 0.61 (`RegisterHotKey`), serde/serde_json.

**Spec:** `docs/superpowers/specs/2026-10-13-tool-shortcuts-design.md`

## Global Constraints

- Linux 仅在 `XDG_SESSION_TYPE=wayland` 时向系统注册全局快捷键；非 Wayland 会话不注册且不报错，仅记录日志。永久不实现、不探测、不兼容 X11。
- 快捷键不会自动向系统注册：录制仅保存按键，必须在设置界面显式开启开关，宿主才会注册该项。已开启集合为空时不创建门户会话，不占用任何系统热键。
- 同一门户会话只能 `BindShortcuts` 一次：集合变化时必须关闭旧会话并新建会话，绝不复用旧会话重复调用。
- 设置窗口不直接注册热键；宿主与设置窗口之间不增加专用 IPC 命令，通过配置文件的 mtime 变更与内容去重驱动重载。
- 快捷键必须包含 Ctrl、Alt 或 Super（Shift 可附加），单键、纯修饰键、Escape 与标点符号均被拒绝。

## Review Focus

1. **已录制但未开启的快捷键被意外注册**：用户录制后若未开启开关，宿主绝不能向系统注册该键。期望行为：过滤逻辑必须硬性要求 `enabled == true` 且按键合法。
2. **ShortcutsChanged 回写触发文件监听死循环**：系统门户回写更新 `shortcuts.json` 的 mtime，导致宿主再次重载并重新建会话。期望行为：写回前先比对内容，写回后立即将内存快照更新为已写回集合，避免下次轮询再次触发。
3. **设置窗口录制时发生快捷键冲突**：用户录入了一个已被其他小工具占用的组合键。期望行为：界面立即拒绝并提示冲突，保留原有的按键绑定，不覆盖对方。
4. **Wayland 激活后工具窗口无法获取焦点**：快捷键被按下后，Portal 会携带 `activation_token`，若启动路径剥除了该环境变量，窗口将无法置顶。期望行为：激活路径必须透传 token，并注入子进程或单例拉起 IPC。
5. **系统桌面回写的 trigger_description 无法机器解析**：桌面返回非标准描述字符串。期望行为：解析失败时保留原有 `accel`，绝不清空或损坏用户现有配置。

---

### Task 1: 组合键模型、语法转换与持久化核心 (`shortcuts.rs`)

**Files:**
- Create: `crates/xtools-host/src/shortcuts.rs`
- Modify: `crates/xtools-host/src/main.rs:1-15`
- Test: `crates/xtools-host/src/shortcuts.rs` (内嵌 `mod tests`)

**Interfaces:**
- Produces:
  - `pub enum Key`: 字母、数字、功能键与指定命名键的闭集。
  - `pub struct Accel`: 规范化的组合键对象（含 `ctrl`, `alt`, `shift`, `super_key`, `key`）。
  - `Accel::parse(s: &str) -> Result<Accel, &'static str>`
  - `Accel::format_canonical(&self) -> String`
  - `Accel::to_xdg_trigger(&self) -> String`
  - `pub struct ShortcutBinding { pub plugin_id: String, pub accel: String, pub enabled: bool }`
  - `pub struct ShortcutFile { pub bindings: Vec<ShortcutBinding> }`
  - `ShortcutFile::load() -> ShortcutFile`
  - `ShortcutFile::save(&self) -> Result<(), String>`
  - `ShortcutFile::active_bindings<'a>(&'a self, discovered_ids: &[String]) -> Vec<&'a ShortcutBinding>`
  - `ShortcutFile::merge_visible(&mut self, visible: &[ShortcutBinding])`
  - `pub fn bindings_equal(a: &[ShortcutBinding], b: &[ShortcutBinding]) -> bool`

- [ ] **Step 1: 编写规范化、非法键拒绝及冲突比对的失败测试**

在 `crates/xtools-host/src/shortcuts.rs` 内编写测试集：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accel_parse_and_canonical_format() {
        let a = Accel::parse("alt+ctrl+t").expect("parse");
        assert_eq!(a.format_canonical(), "Ctrl+Alt+T");
        let a2 = Accel::parse("Super+Shift+F5").expect("parse super shift");
        assert_eq!(a2.format_canonical(), "Shift+Super+F5"); // 顺序为 Ctrl, Alt, Shift, Super
    }

    #[test]
    fn test_accel_rejects_invalid_inputs() {
        assert!(Accel::parse("").is_err());
        assert!(Accel::parse("T").is_err()); // 裸键
        assert!(Accel::parse("Shift+T").is_err()); // 只有 Shift
        assert!(Accel::parse("Ctrl").is_err()); // 单独修饰键
        assert!(Accel::parse("Ctrl+Alt").is_err());
        assert!(Accel::parse("Ctrl+Escape").is_err()); // Escape 不可绑定
        assert!(Accel::parse("Ctrl+Alt+;").is_err()); // 拒绝标点
    }

    #[test]
    fn test_xdg_trigger_format() {
        let a = Accel::parse("Ctrl+Alt+T").unwrap();
        assert_eq!(a.to_xdg_trigger(), "CTRL+ALT+t");
        let a2 = Accel::parse("Super+Space").unwrap();
        assert_eq!(a2.to_xdg_trigger(), "LOGO+space");
    }

    #[test]
    fn test_shortcut_file_active_bindings_filtering() {
        let file = ShortcutFile {
            bindings: vec![
                ShortcutBinding { plugin_id: "p1".into(), accel: "Ctrl+Alt+T".into(), enabled: true },
                ShortcutBinding { plugin_id: "p2".into(), accel: "Ctrl+Alt+J".into(), enabled: false }, // 未开启
                ShortcutBinding { plugin_id: "p3".into(), accel: "invalid".into(), enabled: true },     // 非法键
                ShortcutBinding { plugin_id: "p4".into(), accel: "Ctrl+Alt+T".into(), enabled: true }, // 重复键（后出）
                ShortcutBinding { plugin_id: "p_missing".into(), accel: "Ctrl+Alt+M".into(), enabled: true }, // 未发现
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
                ShortcutBinding { plugin_id: "p_unseen".into(), accel: "Ctrl+Alt+U".into(), enabled: true },
                ShortcutBinding { plugin_id: "p_seen".into(), accel: "Ctrl+Alt+S".into(), enabled: false },
            ],
        };
        let visible = vec![
            ShortcutBinding { plugin_id: "p_seen".into(), accel: "Ctrl+Alt+X".into(), enabled: true },
        ];
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
        let a = vec![ShortcutBinding { plugin_id: "1".into(), accel: "Ctrl+Alt+A".into(), enabled: true }];
        let b = vec![ShortcutBinding { plugin_id: "1".into(), accel: "Ctrl+Alt+A".into(), enabled: true }];
        assert!(bindings_equal(&a, &b));
        let c = vec![ShortcutBinding { plugin_id: "1".into(), accel: "Ctrl+Alt+A".into(), enabled: false }];
        assert!(!bindings_equal(&a, &c));
    }
}
```

- [ ] **Step 2: 运行测试并验证编译失败**

运行：`cargo test -p xtools-wasm -- shortcuts::tests`
预期结果：编译失败（`shortcuts` 模块不存在）。

- [ ] **Step 3: 实现 `shortcuts.rs`**

在 `crates/xtools-host/src/main.rs` 引入 `mod shortcuts;`，并在 `crates/xtools-host/src/shortcuts.rs` 中完整实现：
- `Key` 枚举解析（字母、数字、F1-F12、Space、Tab、Return、BackSpace、Delete、Insert、Home、End、Page_Up、Page_Down、Left、Right、Up、Down）。
- `Accel` 结构体：修饰键排序格式化（`Ctrl+Alt+Shift+Super+<Key>`），解析合法性校验（必须有 Ctrl/Alt/Super）。
- `to_xdg_trigger()`：映射修饰键为 `CTRL/ALT/SHIFT/LOGO`，主键映射为标准 keysym。
- `windows_hotkey_parts()`：在 Windows 下映射修饰键和虚拟键。
- `ShortcutBinding` 与 `ShortcutFile`：序列化、从 `dirs::config_dir()/xtools/shortcuts.json` 读取、去重（同一 plugin_id 保留首项）、`active_bindings`、`merge_visible` 与 `bindings_equal`。

- [ ] **Step 4: 运行单元测试并验证通过**

运行：`cargo test -p xtools-wasm -- shortcuts::tests`
预期结果：全部 PASS。

- [ ] **Step 5: 提交任务成果**

```bash
git add crates/xtools-host/src/shortcuts.rs crates/xtools-host/src/main.rs
git commit -m "feat(shortcuts): 实现组合键语法规范化、平台按键映射与配置持久化"
```

---

### Task 2: 统一启动路径与 Wayland 激活令牌透传 (`runner.rs`, `unix.rs`, `window.rs`)

**Files:**
- Modify: `crates/xtools-host/src/runner.rs:189-245`
- Modify: `crates/xtools-host/src/unix.rs:429-475`
- Modify: `crates/xtools-host/src/windows/window.rs:602-629`
- Modify: `crates/xtools-host/src/tray.rs:85-95`
- Test: `crates/xtools-host/src/runner.rs` (内嵌测试)

**Interfaces:**
- Consumes: `xtools_ui::instance::raise_instance`
- Produces:
  - `pub fn launch_plugin(plugin: &DiscoveredPlugin, activation_token: Option<&str>)`
  - Unix 宿主内 `launch_plugin(plugin: &DiscoveredPlugin, activation_token: Option<&str>)`

- [ ] **Step 1: 编写激活令牌注入命令环境的单测**

在 `crates/xtools-host/src/runner.rs` 底部编写测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_activation_token_to_command() {
        let mut cmd = std::process::Command::new("dummy");
        super::configure_launch_env(&mut cmd, Some("test_token_123"));
        // 验证环境变量 XDG_ACTIVATION_TOKEN 已设置
        let envs: Vec<_> = cmd.get_envs().collect();
        assert!(envs.iter().any(|(k, v)| *k == "XDG_ACTIVATION_TOKEN" && *v == Some(std::ffi::OsStr::new("test_token_123"))));
    }

    #[test]
    fn test_apply_none_activation_token_clears_env() {
        let mut cmd = std::process::Command::new("dummy");
        cmd.env("XDG_ACTIVATION_TOKEN", "stale_token");
        super::configure_launch_env(&mut cmd, None);
        let envs: Vec<_> = cmd.get_envs().collect();
        assert!(envs.iter().any(|(k, v)| *k == "XDG_ACTIVATION_TOKEN" && v.is_none()));
    }
}
```

- [ ] **Step 2: 运行测试并验证编译失败**

运行：`cargo test -p xtools-wasm -- runner::tests::test_apply_activation_token`
预期结果：编译失败（`configure_launch_env` 未定义）。

- [ ] **Step 3: 提取环境变量配置并更新 `launch_plugin` 签名**

1. 在 `crates/xtools-host/src/runner.rs` 中：
   - 提取 `pub(crate) fn configure_launch_env(cmd: &mut std::process::Command, token: Option<&str>)`。当 `token` 为 `Some(t)` 时，设置 `cmd.env("XDG_ACTIVATION_TOKEN", t)`；当为 `None` 时，调用 `cmd.env_remove("XDG_ACTIVATION_TOKEN")`。
   - 更新 `pub fn launch_plugin(plugin: &DiscoveredPlugin, token: Option<&str>)`，在 `raise_instance` 传入 `token`。
2. 同步更新 `crates/xtools-host/src/unix.rs` 中的本地 `launch_plugin(plugin, token)`，使用相同的环境变量逻辑并透传 token。
3. 同步更新 `crates/xtools-host/src/windows/window.rs` 中的本地 `launch_plugin(plugin, _token)`（Windows 忽略 token，保留 `AllowSetForegroundWindow`）。
4. 更新 `tray.rs` 与各点击处理处的原有调用方传入 `None`。

- [ ] **Step 4: 运行测试并验证通过**

运行：`cargo test -p xtools-wasm -- runner::tests`
预期结果：全部 PASS。

- [ ] **Step 5: 提交任务成果**

```bash
git add crates/xtools-host/src/runner.rs crates/xtools-host/src/unix.rs crates/xtools-host/src/windows/window.rs crates/xtools-host/src/tray.rs
git commit -m "feat(runner): 增强小工具拉起机制以支持 Wayland 激活令牌透传"
```

---

### Task 3: 设置界面快捷键配置卡片与按键录制 (`runner.slint`, `settings_window.rs`)

**Files:**
- Modify: `crates/xtools-host/ui/runner.slint`
- Modify: `crates/xtools-host/src/settings_window.rs`
- Test: `crates/xtools-host/src/settings_window.rs` (内嵌测试)

**Interfaces:**
- Consumes: `crate::shortcuts::{Accel, ShortcutBinding, ShortcutFile}`
- Produces:
  - Slint struct `ShortcutItem`: `{ id: string, name: string, mark: string, accel: string, enabled: bool, recording: bool, conflict: string }`
  - Slint callbacks: `shortcut-record-clicked(string)`, `shortcut-clear-clicked(string)`, `shortcut-toggle-enabled(string, bool)`, `shortcut-key-pressed(string, KeyEvent) -> bool`

- [ ] **Step 1: 编写录制状态机与冲突判定的单测**

在 `crates/xtools-host/src/settings_window.rs` 编写纯状态测试：

```rust
#[cfg(test)]
mod shortcut_ui_tests {
    use super::*;
    use crate::shortcuts::{Accel, ShortcutBinding};

    #[test]
    fn test_record_conflict_detection() {
        let existing = vec![
            ShortcutBinding { plugin_id: "xtools.time".into(), accel: "Ctrl+Alt+T".into(), enabled: true },
            ShortcutBinding { plugin_id: "xtools.json".into(), accel: "".into(), enabled: false },
        ];
        // 尝试为 json 录入已被 time 占用的组合键
        let conflict = check_accel_conflict(&existing, "xtools.json", "Ctrl+Alt+T");
        assert!(conflict.is_some());
        assert_eq!(conflict.unwrap(), "快捷键已被其他工具占用");
    }

    #[test]
    fn test_record_non_conflict() {
        let existing = vec![
            ShortcutBinding { plugin_id: "xtools.time".into(), accel: "Ctrl+Alt+T".into(), enabled: true },
        ];
        let conflict = check_accel_conflict(&existing, "xtools.json", "Ctrl+Alt+J");
        assert!(conflict.is_none());
    }
}
```

- [ ] **Step 2: 运行测试并验证编译失败**

运行：`cargo test -p xtools-wasm -- shortcut_ui_tests`
预期结果：编译失败（`check_accel_conflict` 未定义）。

- [ ] **Step 3: 在 `runner.slint` 中声明结构与界面卡片**

在 `crates/xtools-host/ui/runner.slint` 中：
1. 声明 `export struct ShortcutItem`。
2. 在 `RunnerWindow` 属性区增加：
   - `in-out property <[ShortcutItem]> settings-shortcuts: [];`
   - `callback shortcut-record-clicked(string);`
   - `callback shortcut-clear-clicked(string);`
   - `callback shortcut-toggle-enabled(string, bool);`
   - `callback shortcut-key-captured(string, string, bool, bool, bool, bool) -> bool;`
3. 在「窗口外观」卡片下方紧随增加「🛠️ 工具快捷键」卡片：
   - 顶部说明文本：“点击「录制」后按下组合键（需包含 Ctrl/Alt/Super），开启后由悬浮球全局注册；仅 Wayland 与 Windows 生效”。
   - 列表遍历渲染每行：图标/Mark、插件名称、当前组合键显示（未录制显示“未设置”）、录制按钮（录制中显示“录制中…”）、清除按钮、开启 Switch 开关（未录制时禁用）。
   - 录制状态下挂接按键捕获。

- [ ] **Step 4: 在 `settings_window.rs` 中绑定数据与防抖保存**

1. 扫描已发现插件，结合 `ShortcutFile::load()` 构建 `ShortcutItem` 列表并同步给 Slint。
2. 实现 `check_accel_conflict` 逻辑并验证。
3. 处理 `shortcut-record-clicked`、`shortcut-clear-clicked`、`shortcut-toggle-enabled`。
4. 处理录制中的按键捕获：
   - 过滤 Escape（取消录制并退出）。
   - 解析修饰键与主键，若合法则通过 `check_accel_conflict` 校验；冲突时在对应项标记 conflict，否则写回并自动触发防抖保存。
5. 接入已有的 `flash_autosave` 提示。

- [ ] **Step 5: 运行测试并验证通过**

运行：`cargo test -p xtools-wasm -- settings_window`
预期结果：全部 PASS。

- [ ] **Step 6: 提交任务成果**

```bash
git add crates/xtools-host/ui/runner.slint crates/xtools-host/src/settings_window.rs
git commit -m "feat(settings): 在设置窗口中添加小工具快捷键卡片与录制管理"
```

---

### Task 4: Wayland XDG Desktop Portal 全局快捷键后端 (`shortcut_portal.rs`, `unix.rs`)

**Files:**
- Create: `crates/xtools-host/src/shortcut_portal.rs`
- Modify: `crates/xtools-host/Cargo.toml`
- Modify: `crates/xtools-host/src/main.rs`
- Modify: `crates/xtools-host/src/unix.rs:716-860`
- Test: `crates/xtools-host/src/shortcut_portal.rs` (内嵌测试)

**Interfaces:**
- Consumes: `crate::shortcuts::{ShortcutBinding, ShortcutFile, bindings_equal}`
- Produces:
  - `pub struct PortalShortcutController`
  - `PortalShortcutController::start() -> Self`
  - `PortalShortcutController::sync_bindings(&self, bindings: Vec<ShortcutBinding>)`

- [ ] **Step 1: 编写门户回写比对防死循环单测**

在 `crates/xtools-host/src/shortcut_portal.rs` 编写测试：

```rust
#[cfg(test)]
mod portal_tests {
    use super::*;
    use crate::shortcuts::{ShortcutBinding, bindings_equal};

    #[test]
    fn test_portal_triggers_update_does_not_loop_if_identical() {
        let current = vec![
            ShortcutBinding { plugin_id: "xtools.trans".into(), accel: "Ctrl+Alt+T".into(), enabled: true },
        ];
        // 模拟桌面传回一致的 trigger
        let updated = apply_portal_trigger(&current, "xtools.trans", "CTRL+ALT+t");
        assert!(bindings_equal(&current, &updated));
    }

    #[test]
    fn test_portal_triggers_update_modifies_accel_only() {
        let current = vec![
            ShortcutBinding { plugin_id: "xtools.trans".into(), accel: "Ctrl+Alt+T".into(), enabled: true },
        ];
        // 模拟桌面改键为 Ctrl+Alt+G
        let updated = apply_portal_trigger(&current, "xtools.trans", "CTRL+ALT+g");
        assert_eq!(updated[0].accel, "Ctrl+Alt+G");
        assert_eq!(updated[0].enabled, true);
    }
}
```

- [ ] **Step 2: 运行测试并验证编译失败**

运行：`cargo test -p xtools-wasm -- portal_tests`
预期结果：编译失败（`apply_portal_trigger` 未定义）。

- [ ] **Step 3: 添加 `ashpd` 依赖并实现 `shortcut_portal.rs`**

1. 在 `crates/xtools-host/Cargo.toml` 的 `target.'cfg(unix)'.dependencies` 中加入：
   ```toml
   ashpd = { version = "0.11", default-features = false, features = ["async-io"] }
   ```
2. 在 `crates/xtools-host/src/main.rs` 中声明 `#[cfg(unix)] mod shortcut_portal;`。
3. 实现 `shortcut_portal.rs`：
   - 内部启动后台线程与异步运行时处理 `GlobalShortcuts`。
   - `sync_bindings` 发送当前期望绑定的集合。
   - 当绑定集合变化时：关闭旧会话（若有），如果集合为空则退出；否则调用 `CreateSession` 并对新会话调用一次 `BindShortcuts`。
   - 监听 `Activated` 信号，通过 `glib::idle_add_local` 将事件分发至 GTK 主线程，调用 `unix::launch_plugin` 并传入 `activation_token`。
   - 监听 `ShortcutsChanged` 信号，更新绑定并通过 `apply_portal_trigger` 合并写回磁盘，同时同步内存快照防自触发。

- [ ] **Step 4: 在 `unix.rs` 中集成 mtime 轮询与控制器**

在 `crates/xtools-host/src/unix.rs` 的宿主启动流中：
1. 检查环境变量 `XDG_SESSION_TYPE == "wayland"`，非 Wayland 时打日志并跳过。
2. 启动 `PortalShortcutController`。
3. 挂载 1 秒定时器（`glib::timeout_add_local(Duration::from_secs(1), ...)`）监控 `shortcuts.json` 的 mtime。当 mtime 发生变化时，加载并提取 `active_bindings`；若与上次生效集合不同，调用 `controller.sync_bindings` 并更新生效快照。

- [ ] **Step 5: 运行测试并验证通过**

运行：`cargo test -p xtools-wasm -- shortcut_portal`
预期结果：全部 PASS。

- [ ] **Step 6: 提交任务成果**

```bash
git add crates/xtools-host/Cargo.toml crates/xtools-host/src/main.rs crates/xtools-host/src/shortcut_portal.rs crates/xtools-host/src/unix.rs
git commit -m "feat(portal): 基于 ashpd GlobalShortcuts 实现 Wayland 全局快捷键注册与激活"
```

---

### Task 5: Windows `RegisterHotKey` 后端与消息循环集成 (`windows/window.rs`)

**Files:**
- Modify: `crates/xtools-host/src/windows/window.rs:40-120,655-745`
- Test: `crates/xtools-host/src/windows/window.rs` (内嵌测试)

**Interfaces:**
- Consumes: `crate::shortcuts::{ShortcutFile, bindings_equal}`
- Produces:
  - `HostWindow::reload_shortcuts(&mut self)`
  - `WM_HOTKEY` 处理逻辑

- [ ] **Step 1: 编写 Windows 热键映射与 ID 分配的单测**

在 `crates/xtools-host/src/windows/window.rs`（或在 `shortcuts.rs` 的 win32 模块下）编写测试：

```rust
#[cfg(test)]
mod win_hotkey_tests {
    use crate::shortcuts::Accel;

    #[test]
    fn test_win_hotkey_modifiers_and_vk() {
        let a = Accel::parse("Ctrl+Alt+T").unwrap();
        let (mods, vk) = a.to_win_hotkey();
        assert_eq!(vk, b'T' as u32);
        // MOD_CONTROL (0x0002) | MOD_ALT (0x0001)
        assert_eq!(mods & 0x0002, 0x0002);
        assert_eq!(mods & 0x0001, 0x0001);
    }
}
```

- [ ] **Step 2: 运行测试并验证通过/失败**

运行：`cargo test -p xtools-wasm -- win_hotkey_tests`
预期结果：验证转换正确通过。

- [ ] **Step 3: 在 `HostWindow` 中管理热键注册与 `WM_HOTKEY` 响应**

在 `crates/xtools-host/src/windows/window.rs` 中：
1. 在 `HostWindow` 结构体增加：
   - `const TIMER_SHORTCUTS: usize = 3;`
   - `registered_hotkeys: HashMap<i32, String>`（热键 ID -> `plugin_id`）。
   - `applied_shortcuts: Vec<ShortcutBinding>`。
   - `last_shortcuts_mtime: Option<std::time::SystemTime>`。
2. 在窗口初始化时启动 `SetTimer(hwnd, TIMER_SHORTCUTS, 1000, None)`。
3. 实现 `reload_shortcuts(&mut self)`：
   - 遍历 `registered_hotkeys`，调用 `UnregisterHotKey` 全部卸载。
   - 读取 `ShortcutFile::load()`，过滤 `active_bindings`。
   - 遍历激活项，分配 ID 从 1 递增，调用 `RegisterHotKey(hwnd, id, mods, vk)`；若失败打警告并继续其余项。
   - 记录 `applied_shortcuts`。
4. 在 `window_proc` 的 `WM_TIMER` 中响应 `TIMER_SHORTCUTS`：检查 mtime，若文件变化且内容不同则调用 `reload_shortcuts`。
5. 在 `window_proc` 中处理 `WM_HOTKEY`：根据 `wparam` 查找 `registered_hotkeys`，匹配插件后调用本地 `launch_plugin(p, None)`。
6. 窗口销毁时在 `Drop` 中调用 `UnregisterHotKey` 清理。

- [ ] **Step 4: 编译检查代码**

运行：`cargo check -p xtools-wasm`
预期结果：编译无警告/错误。

- [ ] **Step 5: 提交任务成果**

```bash
git add crates/xtools-host/src/windows/window.rs
git commit -m "feat(windows): 通过 RegisterHotKey 和 WM_HOTKEY 支持 Windows 全局快捷键"
```

---

### Task 6: 文档补充与整包验证集成 (`README.md`, 集成烟测)

**Files:**
- Modify: `README.md:120-135`
- Test: 整体单元测试套件

**Interfaces:**
- Consumes: 全功能

- [ ] **Step 1: 在 `README.md` 中说明快捷键功能**

在 `README.md` 的「核心特性」或「使用」章节补充说明：
- 可以在系统设置中为各小工具配置全局快捷键。
- 录制快捷键后需手动开启开关方可生效。
- 全局快捷键支持 Linux (Wayland) 与 Windows；X11 会话不注册。

- [ ] **Step 2: 运行全工作区单元测试**

运行：`cargo test --workspace`
预期结果：全工作区测试全部 PASS。

- [ ] **Step 3: 运行宿主端构建验证**

运行：`cargo build -p xtools-wasm`
预期结果：构建成功，生成 `target/debug/xtools`。

- [ ] **Step 4: 提交任务成果**

```bash
git add README.md
git commit -m "docs: 在 README 中补充小工具全局快捷键说明"
```
