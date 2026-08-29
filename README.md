# 🪐 xtools (WASM Edition)

<p align="center">
  <img src="xtools.svg" width="96" height="96" alt="xtools logo" />
</p>

<p align="center">
  <strong>基于 WebAssembly 沙箱插件系统的统一悬浮工具箱</strong>
  <br />
  <em>纯 Rust 构建 · 悬浮球轨道交互 · WASM 插件热插拔 · 声明式 UI · 能力沙箱</em>
</p>

<p align="center">
  <a href="#-核心特性"><img src="https://img.shields.io/badge/Platform-Linux%20(Wayland%20%7C%20X11)-3E7BFA?logo=linux" alt="Platform Support" /></a>
  <a href="#-技术栈"><img src="https://img.shields.io/badge/Language-Rust%202024-F74C00?logo=rust" alt="Rust 2024" /></a>
  <a href="#-技术栈"><img src="https://img.shields.io/badge/Plugin-WASM%20(wasmtime)-654FF0?logo=webassembly" alt="WASM Plugins" /></a>
  <a href="#-技术栈"><img src="https://img.shields.io/badge/GUI-Slint%20%2B%20GTK4-4B32C3" alt="GUI Stack" /></a>
  <img src="https://img.shields.io/badge/License-MIT-green.svg" alt="License: MIT" />
</p>

---

## ✨ 核心特性

- 🌐 **悬浮球轨道交互**：常驻屏幕右侧的极简主悬浮球，支持自由拖拽；点击平滑展开环绕轨道上的功能球（每颗对应一个插件），再次点击收拢。
- 🧩 **WASM 插件架构**：所有工具均编译为 `.wasm` 模块，由 [wasmtime](https://wasmtime.dev/) 运行时沙箱加载执行——插件崩溃不影响宿主，随放随用、即删即走。
- 🔐 **能力最小化授权**：插件在清单（Manifest）中声明所需权限（剪贴板 / HTTP 域名 / 键值存储 / 定时器），宿主按声明以宿主函数（Host Function）形式供给能力，插件默认与系统隔离。
- 🎨 **声明式 UI**：插件不直接操作窗口，而是通过 `UiView`/`UiNode` 描述界面树、以 `UiEvent`/`UiResponse` 响应交互；由宿主侧 Slint 统一渲染，自带浅色 / 深色主题与无边框窗口 chrome。
- ⚡ **多进程 + 单例窗口**：每个插件窗口由独立进程 `xtools run` 承载，互不干扰；通过平台原生 IPC（Unix Domain Socket / Windows Named Pipe）实现单例，重复点击轨道球时毫秒级拉起已有窗口。
- 🖥️ **桌面深度集成**（Linux）：GTK4 + Layer-Shell 支持 Wayland 与 X11，内置系统托盘（SNI）、KWin 桌面钉扎、fcitx 输入法环境注入。

## 🛠️ 内置插件

| 标记 | 插件 | 说明 | 核心能力 |
| :---: | :--- | :--- | :--- |
| 🕒 | **`xtools.time`** 时间戳转换 | Unix 秒 / 毫秒时间戳与日期时间双向转换 | • 三栏联动实时换算<br/>• 多时区切换<br/>• 一键复制结果 |
| {} | **`xtools.json`** JSON 格式化与校验 | JSON 工具集 | • 格式化 / 压缩 / 去转义 / 校验<br/>• 树形折叠浏览（展开 / 折叠 / 按层折叠）<br/>• 精确行列号错误定位 |
| 文 | **`xtools.trans`** 智能翻译 | 多语言文本翻译 | • 双引擎：MyMemory（免密钥）/ 百度翻译 API<br/>• 语种互换、配置持久化<br/>• 划词粘贴即译<br/>• 百度密钥在托盘「设置」中配置 |
| 智 | **`xtools.ai`** AI 问答 | 基于 OpenAI 兼容接口的多轮 AI 对话 | • 打开自动填入剪贴板内容，手动点击发送<br/>• 聊天气泡界面，支持多轮上下文与历史恢复<br/>• 接口在托盘「设置」中统一配置 |

## 🏗️ 架构设计

```text
                ┌───────────────────────────────┐
                │       xtools-host  (bin)      │
                │  悬浮球 · 轨道菜单 · 系统托盘   │  ← GTK4 (Linux)
                └───────────────┬───────────────┘
                                │ 扫描 plugins/*.wasm → 读取 Manifest
                                │ 点击功能球 → spawn / 复用窗口
                ┌───────────────▼───────────────┐
                │      xtools-runner  (bin)     │
                │       Slint 插件工具窗口       │
                └───────────────┬───────────────┘
                                │ 实例化 .wasm
                ┌───────────────▼───────────────┐
                │         xtools-runtime        │
                │    wasmtime WASM 沙箱运行时    │
                └───────┬───────────────┬───────┘
                        │               │
        ┌───────────────▼───┐   ┌───────▼────────────────┐
        │   插件 .wasm 模块  │   │  宿主能力 (Host Funcs)  │
        │  xtools_plugin_*  │   │  剪贴板 · HTTP · 存储   │
        │  (XPlugin C ABI)  │   │  日志 · 系统时钟        │
        └───────────────────┘   └────────────────────────┘
```

### 工作区组成

| Crate | 类型 | 职责 |
| :--- | :--- | :--- |
| `crates/xtools-protocol` | lib | 插件协议层：`PluginManifest`（元信息 / 窗口 / 权限）、声明式 UI 树 `UiView`/`UiNode`、事件与响应 `UiEvent`/`UiResponse` |
| `crates/xtools-sdk` | lib | 插件开发 SDK：`XPlugin` trait、UI 快捷构建函数、`export_plugin!` 导出宏、宿主能力 API |
| `crates/xtools-ui` | lib | 共享 UI 基建：Slint 主题 token 与通用组件、无边框窗口 chrome、单例 IPC、托盘、输入法引导 |
| `crates/xtools-runtime` | lib | wasmtime 运行时：插件发现（`PluginLoader`）、实例生命周期（`PluginInstance`）、宿主函数注入与插件级存储 |
| `crates/xtools-runner` | bin | 单插件窗口运行器：加载 WASM、双向同步 UI 视图与事件 |
| `crates/xtools-host` | bin `xtools` | 悬浮球宿主（包名 `xtools-wasm`）：轨道菜单动画、插件调度、系统托盘 |
| `plugins/xtools-plugin-*` | cdylib | 官方插件：time / json / trans，编译为 `.wasm` |

## 🚀 快速开始

### 环境要求

- Rust **1.85+**（Edition 2024）
- Linux：GTK4 开发包（如 Debian/Ubuntu 安装 `libgtk-4-dev`）、Wayland 或 X11 会话
- WASM 插件目标：`rustup target add wasm32-unknown-unknown`

### 构建

本工作区完全自包含（`xtools-ui` 已内置于 `crates/`），克隆后即可构建。

一键构建（推荐）：

```bash
./build.sh           # 构建宿主 + WASM 插件，组装 dist/ 便携目录
./build.sh --test    # 构建前先跑全部测试
```

或分步执行：

```bash
# 1. 构建宿主与运行器（产物 target/release/xtools）
cargo build --release

# 2. 构建全部官方插件（WASM）
cargo build --target wasm32-unknown-unknown --release \
    -p xtools-plugin-time -p xtools-plugin-json -p xtools-plugin-trans

# 3. 组装便携目录（可选）
mkdir -p dist/plugins
cp target/release/xtools dist/
cp target/wasm32-unknown-unknown/release/xtools_plugin_*.wasm dist/plugins/
#    按需重命名：xtools_plugin_time.wasm → time.wasm 等
```

运行测试：

```bash
cargo test --workspace
```

> 注：`xtools-runtime` 的集成测试会加载已构建的 WASM 插件（优先 `dist/plugins/`，其次 `target/wasm32-unknown-unknown/release/`）；若尚未构建插件，相关测试会自动跳过。

### 使用

```bash
xtools                # 启动悬浮球与系统托盘（Host 模式）
xtools host           # 同上
xtools run <plugin>   # 直接启动指定插件窗口（如 xtools run time）
xtools <plugin.wasm>  # 按路径或名称直接启动插件
xtools list           # 列出所有已发现的 WASM 插件
xtools --help         # 显示帮助
```

宿主按以下顺序搜索插件（先到先得，按插件 `id` 去重）：

1. 可执行文件同级 `plugins/` 目录（便携模式）
2. 当前工作目录 `plugins/`、`dist/plugins/`、`target/wasm32-unknown-unknown/release/` 等开发路径
3. 用户数据目录（`$XDG_DATA_HOME/xtools/plugins`）
4. 系统目录 `/usr/share/xtools/plugins`、`/usr/local/share/xtools/plugins`

## 🧩 插件开发指南

任意 Rust 库 crate（`crate-type = ["cdylib", "rlib"]`）依赖 `xtools-sdk`，实现 `XPlugin` trait 并用 `export_plugin!` 导出即可成为 xtools 插件：

```toml
# Cargo.toml
[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
xtools-sdk = { path = "crates/xtools-sdk" }   # 或 git / 版本号
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

```rust
use serde::{Deserialize, Serialize};
use xtools_sdk::*;

#[derive(Debug, Serialize, Deserialize)]
pub struct HelloPlugin {
    pub name: String,
}

impl XPlugin for HelloPlugin {
    fn manifest() -> PluginManifest {
        PluginManifest {
            id: "xtools.hello".into(),
            name: "Hello".into(),
            version: "0.1.0".into(),
            description: "示例插件".into(),
            author: "you".into(),
            mark: "Hi".into(),        // 悬浮球上的功能标记
            icon_svg: None,
            window: WindowConfig::default(),
            permissions: vec![],      // 按需申请权限
        }
    }

    fn init() -> Result<Self, String> {
        Ok(Self { name: String::new() })
    }

    fn render(&self) -> UiView {
        UiView::new(column(vec![
            label(&format!("你好, {}!", if self.name.is_empty() { "世界" } else { &self.name })),
            text_input("input_name", &self.name),
            primary_button("btn_greet", "打招呼"),
        ]))
    }

    fn handle_event(&mut self, event: UiEvent) -> Result<UiResponse, String> {
        match event {
            UiEvent::Click { id } if id == "btn_greet" => {
                Ok(UiResponse::ShowToast(Toast {
                    message: format!("你好, {}!", self.name),
                    level: ToastLevel::Success,
                    duration_ms: 1500,
                }))
            }
            UiEvent::InputChanged { id, value } if id == "input_name" => {
                self.name = value;
                Ok(UiResponse::UpdateView(self.render()))
            }
            _ => Ok(UiResponse::NoChange),
        }
    }
}

export_plugin!(HelloPlugin);
```

构建并部署：

```bash
cargo build --target wasm32-unknown-unknown --release -p xtools-plugin-hello
cp target/wasm32-unknown-unknown/release/xtools_plugin_hello.wasm dist/plugins/
xtools list   # 应能看到 xtools.hello
```

### 插件可用能力（由 Manifest 权限声明）

| API | 权限 | 说明 |
| :--- | :--- | :--- |
| `host::clipboard_read()` / `clipboard_write()` | `Permission::Clipboard` | 读写系统剪贴板 |
| `host::http_request(HttpRequest)` | `Permission::Http(域名列表)` | 经宿主代理发起出站 HTTP 请求 |
| `host::storage_get()` / `storage_set()` | `Permission::Storage` | 插件隔离的键值持久化存储 |
| `host::log_info()` / `log_error()` | 无需声明 | 写入宿主日志 |
| `host::now_millis()` | 无需声明 | 宿主系统时钟 |

UI 侧支持文本、输入框、按钮、下拉、开关、选项卡、JSON 树等节点（见 `xtools-protocol/src/ui.rs`），并以 `UiEvent::TimerTick`、`Activated`/`Deactivated` 等事件接收生命周期回调。

## 🗺️ Roadmap

- [ ] Windows 宿主悬浮球（复用同一引擎，Runner / 单例 IPC 组件已就绪）
- [ ] 插件权限按声明强制执行（当前协议已定义权限模型）
- [ ] 插件商店 / 在线分发
- [ ] 更多内置插件

## 📄 License

本项目基于 [MIT License](https://opensource.org/licenses/MIT) 开源。
