//! `desk-pet` 的单元测试（`lib.rs` 末尾 `#[cfg(test)] mod tests;` 引入）。
//!
//! 覆盖计划 rc.1 要求的最小集合：工厂可构造、`start` 成功、静态 spec 与 `start`
//! 注册的 spec 相同、坏 config 回落、`state_json` 键集合与 `window.reason`、
//! `VoiceStarted` / `VoiceEnded` 翻转布尔、`api_version` 用常量。

use super::*;
use std::sync::Mutex;

use serde_json::{Value, json};

/// 记录 register_settings / subscribe / unsubscribe 的测试注册器。
#[derive(Default)]
struct RecordingRegistrar {
    specs: Vec<ModSettingsSpec>,
    topics: Vec<ModEventTopic>,
    unsubscribed: Vec<SubscriptionId>,
    next_id: u64,
}

impl ModRegistrar for RecordingRegistrar {
    fn register_settings(&mut self, spec: ModSettingsSpec) -> Result<(), ModError> {
        self.specs.push(spec);
        Ok(())
    }
    fn subscribe(&mut self, topic: ModEventTopic) -> Result<SubscriptionId, ModError> {
        self.next_id += 1;
        self.topics.push(topic);
        Ok(SubscriptionId(self.next_id))
    }
    fn unsubscribe(&mut self, id: SubscriptionId) -> Result<(), ModError> {
        self.unsubscribed.push(id);
        Ok(())
    }
}

/// 收集日志的宿主服务（logger 写进共享 Vec，其它能力给无害实现）。
fn services() -> (ModServices, Arc<Mutex<Vec<String>>>) {
    let logs = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&logs);
    let services = ModServices::new(
        live2d_ai_mod_system::ModActionSender::new(|_| false),
        live2d_ai_mod_system::SaySender::new(|_| true),
        live2d_ai_mod_system::ModEventSender::new(|_, _| true),
        live2d_ai_mod_system::ModLogger::new(move |level, msg| {
            sink.lock().unwrap().push(format!("{level}: {msg}"));
        }),
    );
    (services, logs)
}

fn started_runtime(config: Value) -> DeskPetRuntime {
    let (services, _logs) = services();
    let mut rt = DeskPetRuntime::with_config(services, DeskPetConfig::from_json(&config));
    let mut reg = RecordingRegistrar::default();
    rt.start(&mut reg).expect("start 应成功");
    rt
}

fn state(rt: &mut DeskPetRuntime) -> Value {
    rt.state_json().expect("desk-pet 必须公开 state_json")
}

#[test]
fn descriptor_uses_the_api_version_constant_and_the_desk_pet_id() {
    assert_eq!(DESCRIPTOR.api_version, MOD_API_VERSION);
    assert_eq!(DESCRIPTOR.id, "desk-pet");
    assert!(!DESCRIPTOR.name.is_empty());
    assert_eq!(DESCRIPTOR.version, env!("CARGO_PKG_VERSION"));
}

#[test]
fn factory_creates_a_runtime() {
    let (services, _logs) = services();
    let rt: Box<dyn ModRuntime> = FACTORY
        .create(services, json!({}))
        .expect("create 应成功（坏配置也不是错误）");
    drop(rt);
}

#[test]
fn factory_static_spec_equals_the_module_spec() {
    let from_factory = FACTORY.settings_spec().expect("必须提供静态 schema");
    assert_eq!(from_factory, settings_spec());
}

#[test]
fn start_registers_the_same_spec_and_subscribes_only_voice_topics() {
    let (services, _logs) = services();
    let mut rt = DeskPetRuntime::new(services);
    let mut reg = RecordingRegistrar::default();
    rt.start(&mut reg).expect("start 应成功");

    assert!(rt.is_registered());
    assert_eq!(reg.specs.len(), 1, "应注册一份 settings schema");
    assert_eq!(
        reg.specs[0],
        settings_spec(),
        "start 注册的 schema 必须与静态 spec 逐字段相同"
    );
    assert!(reg.specs[0].validate().is_ok(), "字段 key 必须唯一");
    assert_eq!(
        reg.topics,
        vec![ModEventTopic::VoiceStarted, ModEventTopic::VoiceEnded],
        "只订阅语音起止（不订阅主链事件）"
    );
}

#[test]
fn create_parses_a_good_config() {
    let (services, _logs) = services();
    let mut rt = FACTORY
        .create(
            services,
            json!({
                "hit": "through",
                "opacity": 0.5,
                "always_on_top": false,
                "height_ratio": 0.4,
            }),
        )
        .expect("create 应成功");
    let snapshot = rt.state_json().expect("有 state_json");
    assert_eq!(snapshot["hit"], json!("through"));
    assert_eq!(snapshot["opacity"], json!(0.5));
    assert_eq!(snapshot["always_on_top"], json!(false));
    assert_eq!(snapshot["height_ratio"], json!(0.4));
}

#[test]
fn create_falls_back_on_bad_config_without_failing() {
    // 非对象 / 未知键 / 未知字符串 / 坏类型 / 越界：一律回落，不 Err。
    for bad in [
        json!(null),
        json!([]),
        json!("opaque"),
        json!({"hit": "click", "opacity": "0.5", "always_on_top": "no", "height_ratio": null}),
        json!({"opacity": 9.0, "height_ratio": 0.0}),
    ] {
        let (services, _logs) = services();
        let mut rt = FACTORY
            .create(services, bad.clone())
            .unwrap_or_else(|e| panic!("坏配置不得导致 create 失败：{bad} → {e}"));
        let snapshot = rt.state_json().expect("有 state_json");
        let expected = DeskPetConfig::from_json(&bad);
        assert_eq!(snapshot["hit"], json!(expected.hit.as_str()), "{bad}");
        assert_eq!(snapshot["always_on_top"], json!(expected.always_on_top));
        // opacity 永不为 0（钳到下界 0.1）。
        let opacity = snapshot["opacity"].as_f64().expect("opacity 是数字");
        assert!(opacity >= 0.1, "opacity 不得为 0：{bad} → {opacity}");
    }
}

#[test]
fn state_json_uses_the_locked_key_set() {
    let mut rt = started_runtime(json!({}));
    let snapshot = state(&mut rt);
    let object = snapshot.as_object().expect("state_json 必须是对象");
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "always_on_top",
            "height_ratio",
            "hit",
            "opacity",
            "voice_active",
            "window"
        ],
        "state_json 键集合锁死（新增字段要同步改这条断言与模块文档）"
    );
    let window = snapshot["window"].as_object().expect("window 是对象");
    let mut window_keys: Vec<&str> = window.keys().map(String::as_str).collect();
    window_keys.sort_unstable();
    assert_eq!(window_keys, vec!["opened", "reason"]);
}

#[test]
fn state_json_never_claims_the_window_is_open_and_says_host_window() {
    let mut rt = started_runtime(json!({}));
    let snapshot = state(&mut rt);
    assert_eq!(snapshot["window"]["opened"], json!(false));
    assert_eq!(snapshot["window"]["reason"], json!("host_window"));
    assert_eq!(snapshot["window"]["reason"], json!(WINDOW_REASON));
    // 纯函数那条路同样只能报 host_window。
    assert_eq!(window_state_json(true)["reason"], json!(WINDOW_REASON));
    assert_eq!(window_state_json(true)["opened"], json!(true));
}

#[test]
fn state_json_reports_contract_defaults_for_an_empty_config() {
    let mut rt = started_runtime(json!({}));
    let snapshot = state(&mut rt);
    assert_eq!(snapshot["hit"], json!("opaque"));
    assert_eq!(snapshot["opacity"], json!(1.0));
    assert_eq!(snapshot["always_on_top"], json!(true));
    assert_eq!(snapshot["height_ratio"], json!(0.75));
    assert_eq!(snapshot["voice_active"], json!(false));
}

#[test]
fn voice_events_flip_the_boolean_and_shutdown_resets_it() {
    let mut rt = started_runtime(json!({}));
    assert!(!rt.voice_active());

    rt.on_event(ModEventTopic::VoiceStarted, "{}").unwrap();
    assert!(rt.voice_active());
    assert_eq!(state(&mut rt)["voice_active"], json!(true));

    // 重复的开始事件不改变语义（幂等）。
    rt.on_event(ModEventTopic::VoiceStarted, "{}").unwrap();
    assert!(rt.voice_active());

    rt.on_event(ModEventTopic::VoiceEnded, "{}").unwrap();
    assert!(!rt.voice_active());
    assert_eq!(state(&mut rt)["voice_active"], json!(false));

    rt.on_event(ModEventTopic::VoiceStarted, "{}").unwrap();
    rt.shutdown().unwrap();
    assert!(!rt.voice_active(), "shutdown 必须复位 voice_active");
    assert!(!rt.is_registered());
    assert_eq!(state(&mut rt)["voice_active"], json!(false));
}

#[test]
fn unrelated_events_do_not_fail_the_mod() {
    // Err 会让 host 把 Mod 置 Failed——为一个没订阅的主题不值得。
    let mut rt = started_runtime(json!({}));
    for topic in ModEventTopic::ALL {
        rt.on_event(*topic, "{}").expect("任何事件都不得 Err");
    }
    assert!(!rt.voice_active(), "其它主题不得翻转 voice_active");
}

#[test]
fn state_json_does_not_touch_voice_or_config_and_is_repeatable() {
    let mut rt = started_runtime(json!({"hit": "window"}));
    let first = state(&mut rt);
    let second = state(&mut rt);
    assert_eq!(first, second, "state_json 是只读快照，连读两次必须相同");
}

#[test]
fn host_side_config_updates_are_visible_in_state_json() {
    // 宿主读文件 → 写共享句柄 → Mod 侧快照立刻反映（不需要事件通道）。
    let mut rt = started_runtime(json!({}));
    assert_eq!(state(&mut rt)["hit"], json!("opaque"));
    {
        let handle = rt.config_handle();
        let mut guard = handle.write().unwrap();
        *guard = DeskPetConfig::from_json(&json!({"hit": "through", "height_ratio": 0.4}));
    }
    let snapshot = state(&mut rt);
    assert_eq!(snapshot["hit"], json!("through"));
    assert_eq!(snapshot["height_ratio"], json!(0.4));
    assert_eq!(rt.config().hit, HitMode::Through);
}

#[test]
fn start_logs_the_effective_config() {
    let (services, logs) = services();
    let mut rt = DeskPetRuntime::with_config(
        services,
        DeskPetConfig::from_json(&json!({"hit": "through"})),
    );
    let mut reg = RecordingRegistrar::default();
    rt.start(&mut reg).unwrap();
    let lines = logs.lock().unwrap().clone();
    assert!(
        lines
            .iter()
            .any(|line| line.contains("desk-pet Mod 已启动") && line.contains("hit=through")),
        "启动日志应带上生效配置：{lines:?}"
    );
}
