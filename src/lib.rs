//! live2d-ai-mod-desk-pet：**桌面透传桌宠**的配置与可观测面（稳定 id `desk-pet`）。
//!
//! # 边界（先读这段，别把它当成窗口实现）
//!
//! 这扇窗**不在本 crate 里**：透传窗仍然住在宿主的
//! `crates/live2d-ai-desktop/src/desk_pet/`（Win32 + `OffscreenRenderer`），
//! 因为主仓缺省 `AVAILABLE_MOD_FACTORIES` 是空表——只有把窗留在宿主，缺省二进制
//! 才能用命令行 `--desk-pet` 开窗。本 crate **不链 Win32 / wgpu /
//! OffscreenRenderer**，也不链接 `user32`，依赖方向只有「Mod → 主仓 `mod-system`」
//! （连 `live2d-ai-runtime` 都用不到：不读密钥、不读 `.env`、不发网络请求）。
//!
//! 表演核心也不进本 Mod：不调用 LLM、不写 `[llm]` / `[tts]`、不接
//! `ModServices.action_tx`（它仍休眠）、不在 Mod 里画皮套、不做口型。口型仍在
//! wasm 的音频时钟里；[state_json] 里的 `voice_active` 只是**一个布尔**，给设置页
//! 看，**不驱动嘴**。
//!
//! # 它做什么
//!
//! 1. 静态 [settings_spec]（四个键，见 [`config`]）：前端按现有 Mod 卡片渲染，
//!    不另起第二套设置页；
//! 2. 一份**与宿主同一张表**的配置解析回落（[`config::DeskPetConfig::from_json`]）：
//!    坏配置只回落该字段，永不 `Err`、永不 panic；
//! 3. 只读状态快照 [state_json]，形状锁死（单测钉住键集合）：
//!
//!    ```json
//!    {
//!      "hit": "opaque",
//!      "opacity": 1.0,
//!      "always_on_top": true,
//!      "height_ratio": 0.75,
//!      "voice_active": false,
//!      "window": { "opened": false, "reason": "host_window" }
//!    }
//!    ```
//!
//! # `window.opened` 为什么恒为 `false`
//!
//! **Mod 不持有 HWND**，也不去桌面进程里查窗——`state_json` 在 web_api 线程上被
//! 调用，查窗会把它阻塞在 HTTP 线程上。窗开没开看宿主日志（`target: "desk_pet"`），
//! 不看这个字段。这里如实报「窗在宿主手里」(`reason: "host_window"`)，不是缺陷。
//!
//! # 缺省态
//!
//! 主仓缺省提交**不**把本 crate 写进 `[workspace.dependencies]`、desktop 依赖或
//! `AVAILABLE_MOD_FACTORIES`（工厂数断言 `mod_count_is_zero` 保持不变），
//! 出厂 `mods.json` 也不启用它。挂载是维护者手动的一步，
//! 见 `docs/architecture/two-repo-mods.md` §4。
//!
//! **不得复用**已删除的旧桌宠 Mod id（宿主 `mods_scan` 的文件夹扫描拒绝表已守着
//! 那一行；这里刻意不把旧 id 字面写进新 crate），也不得从
//! `checkpoint/pre-d1-dormant` 拷回已删 crate。

pub mod config;
pub mod spec;

use std::sync::{Arc, RwLock};

use live2d_ai_mod_system::{
    MOD_API_VERSION, ModDescriptor, ModError, ModEventTopic, ModFactory, ModRegistrar, ModRuntime,
    ModServices, ModSettingsSpec, SubscriptionId,
};

pub use config::{
    ALWAYS_ON_TOP_DEFAULT, DeskPetConfig, HEIGHT_RATIO_DEFAULT, HIT_DEFAULT, HitMode,
    OPACITY_DEFAULT,
};
pub use spec::settings_spec;

/// 本 Mod 的静态描述符（稳定 id `desk-pet`；`api_version` 取常量，不手写数字）。
pub const DESCRIPTOR: ModDescriptor = ModDescriptor {
    id: "desk-pet",
    name: "桌面透传桌宠",
    version: "0.1.0",
    // 对齐 Mod 系统 API 版本；不对齐时 host 会把本 Mod 置 Failed（主链不崩）。
    api_version: MOD_API_VERSION,
};

/// `state_json.window.opened`：本版**恒为 `false`**（窗在宿主里，不在本 Mod）。
pub const WINDOW_OPENED: bool = false;

/// `state_json.window.reason`：窗由宿主线程持有。
pub const WINDOW_REASON: &str = "host_window";

/// `desk-pet` 运行时：持有一份配置快照 + `voice_active` 布尔。
///
/// # 为什么配置是 `Arc<RwLock<_>>` 而不是普通字段
///
/// 配置文件（`mods.json` 的 `mods.desk-pet.config`）由**宿主**读取并交给窗口线程，
/// 本 Mod 只是同一份配置在 Mod 侧的可观测副本。共享句柄让宿主写一次、这里立刻
/// 读得到（[`DeskPetRuntime::config_handle`]），不需要为读一个数字再走一次
/// 事件通道。本 crate 自己**不读文件**。
pub struct DeskPetRuntime {
    services: ModServices,
    config: Arc<RwLock<DeskPetConfig>>,
    /// `VoiceStarted` 置真、`VoiceEnded` / `shutdown` 置假。**不驱动嘴**。
    voice_active: bool,
    /// `start` 注册的 settings schema（单测与「已就绪」判定用）。
    registered: bool,
    /// 订阅 id（`start` 里由 host 发放；`shutdown` 尽力取消）。
    subscriptions: Vec<SubscriptionId>,
}

impl DeskPetRuntime {
    /// 用注入的 host services 构造（配置先取合同缺省）。
    pub fn new(services: ModServices) -> Self {
        Self::with_config(services, DeskPetConfig::default())
    }

    /// 用一份**已解析**的配置构造（宿主读 `mods.json` 后用这个）。
    pub fn with_config(services: ModServices, config: DeskPetConfig) -> Self {
        Self {
            services,
            config: Arc::new(RwLock::new(config)),
            voice_active: false,
            registered: false,
            subscriptions: Vec::new(),
        }
    }

    /// 配置共享句柄（宿主侧更新后，[Self::state_json] 立刻反映新值）。
    pub fn config_handle(&self) -> Arc<RwLock<DeskPetConfig>> {
        Arc::clone(&self.config)
    }

    /// 当前配置快照。
    pub fn config(&self) -> DeskPetConfig {
        *self.config.read().unwrap_or_else(|e| e.into_inner())
    }

    /// `voice_active` 当前值（`VoiceStarted` / `VoiceEnded` 翻转它）。
    pub fn voice_active(&self) -> bool {
        self.voice_active
    }

    /// `start` 是否已注册 schema。
    pub fn is_registered(&self) -> bool {
        self.registered
    }
}

impl ModRuntime for DeskPetRuntime {
    fn start(&mut self, registrar: &mut dyn ModRegistrar) -> Result<(), ModError> {
        // 与 ModFactory::settings_spec 逐字段相同的那一份（单测钉住）。
        registrar.register_settings(settings_spec())?;
        for topic in [ModEventTopic::VoiceStarted, ModEventTopic::VoiceEnded] {
            self.subscriptions.push(registrar.subscribe(topic)?);
        }
        self.registered = true;
        let config = self.config();
        self.services.logger.info(&format!(
            "desk-pet Mod 已启动（hit={} opacity={} always_on_top={} height_ratio={}）",
            config.hit.as_str(),
            config.opacity,
            config.always_on_top,
            config.height_ratio
        ));
        Ok(())
    }

    /// 只认语音起止；payload 一律不解析（本 Mod 不看内容，只看开始/结束）。
    fn on_event(&mut self, topic: ModEventTopic, payload: &str) -> Result<(), ModError> {
        let _ = payload;
        match topic {
            ModEventTopic::VoiceStarted => self.voice_active = true,
            ModEventTopic::VoiceEnded => self.voice_active = false,
            // 未订阅的主题不该到这里；到了也只记一条 debug，绝不 Err
            // （Err 会让 host 把 Mod 置 Failed，为一个无关事件不值得）。
            other => {
                self.services
                    .logger
                    .info(&format!("desk-pet 收到未订阅的事件：{}", other.as_str()));
            }
        }
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), ModError> {
        self.voice_active = false;
        self.registered = false;
        self.services.logger.info("desk-pet Mod 已关闭");
        Ok(())
    }

    /// 只读快照：**不写盘、不走网络、不阻塞**（键集合见模块文档，单测锁死）。
    fn state_json(&mut self) -> Option<serde_json::Value> {
        let config = self.config();
        Some(serde_json::json!({
            "hit": config.hit.as_str(),
            "opacity": json_ratio(config.opacity),
            "always_on_top": config.always_on_top,
            "height_ratio": json_ratio(config.height_ratio),
            "voice_active": self.voice_active,
            "window": window_state_json(WINDOW_OPENED),
        }))
    }
}

/// `f32` 旋钮 → `state_json` 里的数字（**去掉 f32→f64 展宽噪声**）。
///
/// `json!(0.4_f32)` 会吐出 `0.4000000059604645`：f32 的 0.4 与 f64 的 0.4 不是同一个
/// 数，serde 只是把它无损展宽。合同里的形状是 `0.75` / `1.0` 这种干净数字，前端还要
/// 直接显示，所以这里按 4 位小数取整——比用户旋钮的任何实际粒度都细，且合同里的
/// 全部缺省 / 边界（`0.1` / `0.25` / `0.75` / `0.9` / `1.0`）取整后逐位不变。
pub fn json_ratio(value: f32) -> serde_json::Value {
    let rounded = (f64::from(value) * 10_000.0).round() / 10_000.0;
    serde_json::Number::from_f64(rounded)
        .map(serde_json::Value::Number)
        // 合同范围是 0.1..=1.0 的有限值；真拿到 NaN/Inf 就如实报 null，
        // 绝不 panic（state_json 跑在 web_api 线程上）。
        .unwrap_or(serde_json::Value::Null)
}

/// `state_json.window` 段（`{"opened":…, "reason":…}`）。
///
/// 抽成纯函数只为让「`opened` 恒 `false`、`reason` 恒 `host_window`」这条
/// 合同有两处可断言（运行时快照与此处的单测）。
pub fn window_state_json(opened: bool) -> serde_json::Value {
    serde_json::json!({
        "opened": opened,
        "reason": WINDOW_REASON,
    })
}

/// 静态工厂（挂进宿主时注册 `&FACTORY`）。
pub struct DeskPetFactory;

impl ModFactory for DeskPetFactory {
    fn descriptor(&self) -> &'static ModDescriptor {
        &DESCRIPTOR
    }

    /// 静态 schema：本 Mod 还没启用时前端也要能渲染配置表单。
    fn settings_spec(&self) -> Option<ModSettingsSpec> {
        Some(settings_spec())
    }

    fn create(
        &self,
        services: ModServices,
        config: serde_json::Value,
    ) -> Result<Box<dyn ModRuntime>, ModError> {
        // 坏配置**不**是构造失败：from_json 只回落字段，不返回 Err。
        Ok(Box::new(DeskPetRuntime::with_config(
            services,
            DeskPetConfig::from_json(&config),
        )))
    }
}

/// 工厂单例（宿主写 `&live2d_ai_mod_desk_pet::FACTORY`）。
pub const FACTORY: DeskPetFactory = DeskPetFactory;

/// 单元测试（拆到独立文件：源码文件 ≤500 行的门禁口径与主仓一致）。
#[cfg(test)]
mod tests;
