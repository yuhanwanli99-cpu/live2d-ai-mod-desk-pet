# live2d-ai-mod-desk-pet

桌面透传桌宠的**配置与可观测面** Mod。crate 名 `live2d-ai-mod-desk-pet`，
稳定 id **`desk-pet`**，工厂 **`FACTORY`**（`DeskPetFactory`），`api_version = 1`。

本仓库是**独立本地仓库**（仓库根 = crate 根，`Cargo.toml` 末尾空的 `[workspace]`），
与主仓 `/home/skystar/Live2D-Ai` **同级**，不是 submodule，也不是 vendored 副本。
主仓缺省提交**不引用**它（工厂表仍是空表，`mod_count_is_zero` 不变）。

## 这是什么 / 不是什么

**是**：

- 一份**静态设置 schema**（四个键：`hit` / `opacity` / `always_on_top` / `height_ratio`），
  前端按现有 Mod 卡片渲染，不另起第二套设置页；
- 一份与宿主**同一张表**的配置解析回落（坏配置只回落该字段，永不 `Err`）；
- 一份只读状态快照 `state_json`（形状见下），订阅 `VoiceStarted` / `VoiceEnded`
  维护一个 `voice_active` 布尔。

**不是**：

- **窗口不在本仓库**。透传窗（Win32 + `OffscreenRenderer` + 帧循环）仍在主仓
  `crates/live2d-ai-desktop/src/desk_pet/`。原因：主仓缺省工厂表是空的，
  只有把窗留在宿主，缺省二进制才能用命令行 `--desk-pet` 开窗（双引号双击与
  `--web` 行为不变）；
- **不链 Win32 / wgpu / `OffscreenRenderer` / `user32`**，不依赖
  `live2d-ai-desktop` 或 `l2d`；依赖方向只有「Mod → 主仓 `live2d-ai-mod-system`」。
  连 `live2d-ai-runtime` 都用不到——本 Mod 不读密钥、不读 `.env`、不发网络请求；
- **不是表演核心**：不调用 LLM、不写 `[llm]` / `[tts]`、不接
  `ModServices.action_tx`（仍休眠）、不画皮套、不做口型。口型仍在 wasm 的音频时钟里，
  `voice_active` 只是给设置页看的布尔，**不驱动嘴**；
- **不做拖拽 / 命中 / DPI**：那三件在宿主窗创建路径（见配对计划 H3 / H4）；
- **不复用已删除的旧桌宠 Mod id**（宿主文件夹扫描的拒绝表已守着那一行，本仓刻意
  不把旧 id 字面写进来），也不从 `checkpoint/pre-d1-dormant` 拷回旧 crate。

## 配置合同

JSON 住在 `mods.json` 的 `mods.desk-pet.config`，**不回流** `live2d-ai.toml`。
与宿主 `crates/live2d-ai-desktop/src/desk_pet/config.rs` 的 `PetConfig` **同一张表**
（改键名或边界必须两边同改；本仓刻意不 path 依赖宿主）。

| key | kind | 缺省 | 边界 |
| --- | --- | --- | --- |
| `hit` | select | `opaque` | `through` 整窗穿透；`opaque` 按 alpha 命中；`window` 整矩形吃鼠标 |
| `opacity` | number | `1.0` | `min = 0.1`，`max = 1.0`（**永不为 0**） |
| `always_on_top` | bool | `true` | |
| `height_ratio` | number | `0.75` | `min = 0.25`，`max = 0.9` |

坏配置不是错误：未知字符串 / 缺键 / 非数字 / 非有限值 ⇒ **该字段**回落缺省；
有限但越界的数字 ⇒ 钳到边界；其余字段照常。`Number` 字段的 schema 没有 default 槽，
缺键时的数字由解析函数回落，不靠前端先写进文件。

## `state_json`（键集合锁死）

```json
{
  "hit": "opaque",
  "opacity": 1.0,
  "always_on_top": true,
  "height_ratio": 0.75,
  "voice_active": false,
  "window": { "opened": false, "reason": "host_window" }
}
```

- 前四项来自本 Mod 的 config（同一套回落）；
- `voice_active` 由 `VoiceStarted` / `VoiceEnded` 翻转，`shutdown` 复位；
- `window.opened` **本版恒 `false`**、`window.reason` 恒 `host_window`：Mod 不持有 HWND，
  也不去桌面进程里查窗（`state_json` 跑在 web_api 线程上，查窗会阻塞 HTTP）。
  窗开没开看宿主日志 `target: "desk_pet"`。

没有 secret 字段；`state_json` 不写盘、不走网络。

## 挂回主仓（维护者手动，**没有动态加载器**）

按主仓 `docs/architecture/two-repo-mods.md` §4 的五步挂载：

1. 主仓根 `Cargo.toml` 的 `[workspace.dependencies]` 加
   `live2d-ai-mod-desk-pet = { path = "../live2d-ai-mod-desk-pet" }`；
2. `crates/live2d-ai-desktop/Cargo.toml` 加 `live2d-ai-mod-desk-pet = { workspace = true }`；
3. `crates/live2d-ai-desktop/src/main.rs` 的 `AVAILABLE_MOD_FACTORIES` 注册
   `&live2d_ai_mod_desk_pet::FACTORY`，并把工厂数断言改成当时那条分支的数量；
4. 本 Mod 没有独立宿主路由（不像 external/chat、voice/transcript），宿主集成只有
   开窗条件 + 注册表里的 enable；
5. 要缺省启用才动 `cli_entry::default_mods_manifest`——**缺省提交不做这一步**。

缺省分支**不要**带上挂载改动；演练放在单独分支（见计划 rc.3）。

## 开窗条件（宿主行为，本仓只提供配置与状态）

窗打开的条件是下面两条之一：命令行 `--desk-pet`，或本进程工厂里有 `desk-pet` 且
`mods.json` 里 `enabled` 为真。两条都不满足就不开窗（双击与 `--web` 保持今天的样子）。
命令行优先于 `enabled: false`；工厂为 0 时只有命令行能开窗。
非 Windows 的 `--desk-pet` 仍是**退出码 2** 的用法错误（不 bind、不开窗）。

## 门禁

```bash
cd /home/skystar/live2d-ai-mod-desk-pet
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

不并进主仓的 `rust-ratio`。宿主侧（主仓）门禁另见配对计划 §6。

## 许可

AGPL-3.0-only（见 `LICENSE`），与主仓及模板 crate 一致。

公开仓库：未推送（按计划，维护者另说再推）。
