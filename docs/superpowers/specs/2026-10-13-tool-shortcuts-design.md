# 小工具全局快捷键

## 目标

已发现的每个插件可以在设置里录一条全局快捷键。录制只保存组合键。用户在设置里打开该工具的开关后，常驻悬浮球才向系统注册这一条。按下已注册的快捷键时，打开或前置对应工具窗口。

Linux 只在 Wayland 会话注册。Windows 用 `RegisterHotKey`。产品不再支持 X11；本功能不实现 X11，也不在本次删除宿主里已有的 X11 运行代码。

## 非目标

- 预置任何默认快捷键，或在用户未开启时向系统注册。
- X11、`XGrabKey`、X11 探测分支、X11 兜底。
- 修改工具窗口内已有操作键（翻译 `Enter`、编码 `Ctrl+Enter`）。
- 设置进程自己调用 `BindShortcuts`。
- 门户拒绝或 `RegisterHotKey` 失败时，自动把 `enabled` 改回 false。
- 设置窗口打开期间热刷新插件列表。
- 本次改写 README 里已有的平台声明。快捷键相关说明只写「Wayland 与 Windows」。

## 角色

设置窗口和悬浮球是两个进程。

- 设置进程只读写 `shortcuts.json`，并录制组合键。
- 宿主进程（悬浮球）读取该文件，并且只注册已开启的绑定。触发后走现有打开工具路径。
- 宿主没在跑时，快捷键不生效。配置仍可保存，下次宿主启动再加载。

不新增实例 IPC 命令。

## 配置

路径与 `window.json` 相同的配置根：`dirs::config_dir()/xtools/shortcuts.json`。没有配置目录时用相对路径 `shortcuts.json`。

```json
{
  "bindings": [
    { "plugin_id": "xtools.trans", "accel": "Ctrl+Alt+T", "enabled": false }
  ]
}
```

- `plugin_id`：插件清单 id，例如 `xtools.trans`。
- `accel`：规范写法。修饰键顺序固定为 Ctrl、Alt、Shift、Super，用 `+` 连接，最后是主键。示例：`Ctrl+Alt+T`、`Ctrl+Shift+F5`、`Super+Space`。
- `enabled`：缺省为 false。缺省必须是 false，避免旧文件或手改文件在未显式开启时被注册。
- 未知字段忽略。`enabled` 缺失视为 false。同一 `plugin_id` 出现多条时只保留第一条，读取不因此回写文件。

插件已卸载的绑定留在文件里：不注册，设置列表不显示。设置保存时只更新当前已发现插件的行，保留未见 id 的整条绑定（含 `enabled`）。

清除某插件：删除该 `plugin_id` 的绑定，不是留下空 `accel`。

## 组合键

录制与解析接受的主键闭集：

- 拉丁字母 A–Z
- 数字 0–9
- F1–F12
- Space、Tab、Return、BackSpace、Delete、Insert、Home、End、Page_Up、Page_Down、Left、Right、Up、Down

不接受标点。Escape 只用于取消录制，不能成为绑定。

合法绑定必须包含 Ctrl、Alt、Super 之一。只有 Shift、裸键、单独修饰键都非法。

比较和存盘前先规范化：修饰键按 Ctrl、Alt、Shift、Super 排序，字母主键大写显示。`alt+ctrl+t` 与 `Ctrl+Alt+T` 是同一组合。

交给门户的 `preferred_trigger` 使用 [shortcuts XDG 规范](https://specifications.freedesktop.org/shortcuts/latest/)：修饰键名为 `CTRL`、`ALT`、`SHIFT`、`LOGO`（Super 映射为 LOGO），主键用下表的 keysym，不带 `XKB_KEY_` 前缀。`Ctrl+Alt+T` 变成 `CTRL+ALT+t`，`Super+Space` 变成 `LOGO+space`。文件里不存这套字符串。

规范主键与 keysym、Windows 虚拟键一一对应：

| 规范主键 | keysym | 虚拟键 |
| --- | --- | --- |
| A–Z | a–z | `VK_A`–`VK_Z` |
| 0–9 | 0–9 | `VK_0`–`VK_9` |
| F1–F12 | F1–F12 | `VK_F1`–`VK_F12` |
| Space | space | `VK_SPACE` |
| Tab | Tab | `VK_TAB` |
| Return | Return | `VK_RETURN` |
| BackSpace | BackSpace | `VK_BACK` |
| Delete | Delete | `VK_DELETE` |
| Insert | Insert | `VK_INSERT` |
| Home | Home | `VK_HOME` |
| End | End | `VK_END` |
| Page_Up | Page_Up | `VK_PRIOR` |
| Page_Down | Page_Down | `VK_NEXT` |
| Left / Right / Up / Down | 同名 | `VK_LEFT` / `VK_RIGHT` / `VK_UP` / `VK_DOWN` |

Windows 在注册边界把修饰键转成 `MOD_CONTROL`、`MOD_ALT`、`MOD_SHIFT`、`MOD_WIN`，主键用上表虚拟键。不另存一套。

## 设置界面

在设置页「窗口外观」卡片旁增加「工具快捷键」卡片。不改设置窗口默认尺寸，卡片进入现有滚动页。

每个已发现插件一行：

- 插件名
- 当前规范组合键，或「未设置」
- 录制
- 清除（没有绑定时不可用）
- 开启开关。没有合法组合键时开关不可用，且不能被打开

录制：

- 同时只有一行处于录制。再点另一行的录制，前一行取消且不保存。
- 录制中显示「请按下组合键，Esc 取消」。
- 下一次合法组合键写入该行并退出录制。此时 `enabled` 保持原值；原先未开启则仍不注册。录制中开关不可用，避免把尚未确认的按键注册出去。
- Esc 取消，保留原组合键。
- 非法按键留在录制态，不报错。
- 该组合键已被其他插件使用（不论对方是否开启）则拒绝，保留原组合键，行内显示冲突，直到该行下一次开始录制或成功保存。不覆盖对方。

开、关、录制成功、清除都立即写文件，并沿用现有「已自动保存」提示。写失败时保持内存中的上一份成功状态，并在卡片上显示错误。

卡片说明固定为：需要悬浮球在运行；仅 Wayland 会话会真正注册；第一次绑定以及之后每次重新绑定，桌面可能弹出系统授权。不写「不支持 X11」这种把 X11 当成待兼容平台的句子。

## 宿主注册

宿主启动时读取文件。之后每 1 秒看一次 mtime。比较的是解析后的绑定列表，顺序敏感；JSON 对象字段顺序无关。与上次成功应用的列表相同则不重新注册，只记住新的 mtime。这样宿主自己回写文件不会打成循环。

可注册集合 = `enabled == true` 且组合键合法且该 `plugin_id` 仍被发现。顺序与文件中出现顺序一致。

集合为空：若已有门户会话或已注册的 Windows 热键，全部卸掉，不再创建新会话。

### Wayland

仅当 `XDG_SESSION_TYPE=wayland`。其他 Linux 会话不注册，每个宿主进程只打一条日志。设置里的开关状态照常保存。

用 `ashpd` 调用 `org.freedesktop.portal.GlobalShortcuts`（本机门户版本为 2）。不依赖门户记住上次绑定，也不调用 `ListShortcuts` 恢复。每次需要注册都新建会话。会话放在宿主进程内的专用线程，不阻塞 GTK 主循环。`Activated` 必须切回宿主主循环后再打开工具。

门户限制：一个会话只能 `BindShortcuts` 一次。因此每次可注册集合发生变化时：

1. 关闭旧会话。
2. 集合为空则停止。
3. `CreateSession`。
4. 对该会话调用一次 `BindShortcuts`。`parent_window` 传空字符串。shortcut id 用 `plugin_id`。`description` 用插件显示名。`preferred_trigger` 用上一节的 XDG 写法。
5. 绑定失败、用户拒绝，或会话创建失败：打警告，关闭该会话，不修改 `enabled`。不要在同一会话上再次 `BindShortcuts`。下一次文件内容变化或宿主重启再新建会话尝试一次。不要紧循环重试。

`ShortcutsChanged` 以及 `BindShortcuts` 的响应只保证有 `description` 和 `trigger_description`，不保证有机器可读的 trigger。回写规则：

- 若该条带有可解析的 trigger 字段，或 `trigger_description` 能解析为 XDG 写法或本文件的规范写法，则把对应已开启绑定的 `accel` 更新为规范写法。
- 解析不了则不改 `accel`，只打日志。
- 不新建绑定，不改变任何 `enabled`，不删除绑定。
- 写回前先读盘合并。宿主只改上述 `accel`。内容与盘上一致则不写。写回成功后，把内存中的「上次已应用列表」更新为写回后的列表，避免下一轮 mtime 变化再次绑定。

### Windows

在宿主窗口上对可注册集合调用 `RegisterHotKey`。热键 id 从 1 起递增，映射到 `plugin_id`。重新注册前先 `UnregisterHotKey` 掉上一轮全部 id。某条被系统拒绝则跳过该条并打日志，其余继续，不回滚已成功的，不改 `enabled`。`window_proc` 处理 `WM_HOTKEY` 后打开对应工具。

## 打开工具

触发时按 `plugin_id` 在当前已发现插件里查找，找到则打开。找不到则忽略并打日志。

打开动作与轨道球点击相同：已有窗口则前置，没有则 `xtools run`。Wayland 的 `Activated` 若带 `activation_token`，传给这次前置或拉起，并且这条拉起路径不要清掉 `XDG_ACTIVATION_TOKEN`。没有 token 时与现在点击轨道球一样。

## 错误处理

- 文件不存在：视为没有绑定。不创建文件，直到设置第一次保存。
- JSON 损坏：整份视为没有绑定，打日志，不覆盖原文件。某条缺 `plugin_id`、`accel` 非法，或 `plugin_id` 重复：跳过该条，保留其余条，读取不回写。
- `enabled` 为 true 但没有合法 `accel`：不注册。
- 多条已开启绑定在规范化后组合键相同：文件中先出现的注册，后面的跳过并打日志。设置界面在写入前拒绝这种新录制。
- 设置保存与宿主回写都是先读再合并。设置只改当前列表。宿主只按上一节规则改 `accel`。
- `launch_plugin` 已有的启动失败日志保持不变，快捷键路径不另做提示。

## 测试

只测纯逻辑，不连接门户，不创建窗口，不调用 `RegisterHotKey`。

- 规范化：`alt+ctrl+t` 变成 `Ctrl+Alt+T`；修饰键顺序；字母显示大写。
- 拒绝：裸键、只有 Shift、单独修饰键、Escape、标点、空字符串。
- 闭集内的字母、数字、功能键、列出的命名键可以规范化。
- 冲突：另一条已有相同规范组合键时，新录制被拒绝，原绑定不变。
- 读写往返，含 `enabled` 缺省为 false。
- 损坏 JSON 读取为空，且读取本身不写文件。
- 合并：保存当前列表时保留未见 `plugin_id` 及其 `enabled`。
- 可注册集合等于已开启、组合键合法、插件 id 在给定发现列表中。未开启即使组合键合法也不在集合内。
- 已开启集合里重复组合键：先出现的保留。
- 同一 `plugin_id` 多条时只保留第一条。
- 两份解析后的绑定列表按顺序比较是否相等，供 mtime 去重。字段顺序不同但解析结果相同视为相等。
- 规范写法与 XDG trigger、Windows modifier/虚拟键的转换各有一对往返用例。

## 改动文件

- `crates/xtools-host/src/shortcuts.rs`：模型、解析、规范化、冲突、读写、合并、可注册过滤、XDG / Windows 转换。
- `crates/xtools-host/src/shortcut_portal.rs`：仅 Unix。Wayland 下的会话、一次绑定、信号、回写。
- `crates/xtools-host/src/unix.rs`：Wayland 且存在已开启绑定时才建会话；1 秒 mtime 检查。
- `crates/xtools-host/src/windows/window.rs`：注册、卸载、`WM_HOTKEY`。
- `crates/xtools-host/src/runner.rs`：打开路径能带上可选的 activation token。
- `crates/xtools-host/src/settings_window.rs`：发现插件、录制、清除、开关、保存。
- `crates/xtools-host/ui/runner.slint`：卡片与回调。
- `crates/xtools-host/src/main.rs`：模块声明。
- `crates/xtools-host/Cargo.toml`：Unix 依赖 `ashpd`。
- `README.md`：使用一节加一句，全局快捷键在设置中录制并开启，仅 Wayland 与 Windows。

## 完成标准

- 设置里可以录制、清除、开启、关闭每个已发现插件的一条快捷键。
- 未开启的组合键不会被宿主注册；一条都没开时不创建门户会话，也不调用 `RegisterHotKey`。
- Wayland 上开启后，宿主新建会话并只 `BindShortcuts` 一次；按下后打开或前置该工具。
- Windows 上开启后 `RegisterHotKey` 生效，`WM_HOTKEY` 打开同一工具。
- 非 Wayland 的 Linux 会话不注册。
- 上述单元测试通过。
