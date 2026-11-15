//! `desk-pet` 的**静态**设置 schema。
//!
//! 只有四个键（合同见 [`crate::config`] 与 [`crate::config::DeskPetConfig`]）：
//! 前端按现有 Mod 卡片渲染，**不注入 HTML / JS**，也不另起第二套设置页。
//!
//! 同一份 schema 有两个入口，必须逐字段相同（单测
//! `start_registers_the_same_spec_as_the_static_one` 钉住）：
//!
//! - [`ModFactory::settings_spec`](live2d_ai_mod_system::ModFactory::settings_spec)
//!   ——「本 Mod 还没启用」时前端也能渲染表单；
//! - `start` 里 `register_settings`——运行时注册的那一份。
//!
//! **`Number` 字段没有 default 槽**（Mod 系统 schema 只有 Bool / String / Select
//! 带 default）：缺键时的数字由宿主 / 本 crate 的解析函数回落
//! （[crate::config::DeskPetConfig::default]），**不靠前端先写进文件**。
//! `hit` 是 Select，缺省显式写成 `opaque`（它不是第一个「最安全」的直觉值）。

use live2d_ai_mod_system::{ModSettingField, ModSettingsSpec, SelectOption};

use crate::DESCRIPTOR;

/// 设置 schema 版本（前端缓存键；改字段就加 1）。
pub const SETTINGS_VERSION: u32 = 1;

/// `hit` 的缺省字符串（与 [`crate::config::HIT_DEFAULT`] 一致）。
pub const HIT_DEFAULT_TEXT: &str = "opaque";

/// 本 Mod 的静态设置 schema（四个键，顺序即表单渲染顺序）。
pub fn settings_spec() -> ModSettingsSpec {
    ModSettingsSpec {
        mod_id: DESCRIPTOR.id.to_string(),
        title: DESCRIPTOR.name.to_string(),
        version: SETTINGS_VERSION,
        fields: vec![
            ModSettingField::Select {
                key: "hit".to_string(),
                label: "鼠标命中".to_string(),
                options: vec![
                    SelectOption {
                        value: "opaque".to_string(),
                        label: "按不透明像素命中（缺省）".to_string(),
                    },
                    SelectOption {
                        value: "through".to_string(),
                        label: "整窗点击穿透".to_string(),
                    },
                    SelectOption {
                        value: "window".to_string(),
                        label: "整矩形接收鼠标（调试）".to_string(),
                    },
                ],
                default: Some(HIT_DEFAULT_TEXT.to_string()),
            },
            ModSettingField::Number {
                key: "opacity".to_string(),
                label: "不透明度".to_string(),
                min: f64::from(crate::config::OPACITY_MIN),
                max: f64::from(crate::config::OPACITY_MAX),
            },
            ModSettingField::Bool {
                key: "always_on_top".to_string(),
                label: "始终置顶".to_string(),
                default: crate::config::ALWAYS_ON_TOP_DEFAULT,
            },
            ModSettingField::Number {
                key: "height_ratio".to_string(),
                label: "窗高占屏幕比例".to_string(),
                min: f64::from(crate::config::HEIGHT_RATIO_MIN),
                max: f64::from(crate::config::HEIGHT_RATIO_MAX),
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_is_valid_and_lists_the_four_contract_keys_in_order() {
        let spec = settings_spec();
        assert!(spec.validate().is_ok(), "字段 key 必须唯一");
        let keys: Vec<&str> = spec.fields.iter().map(ModSettingField::key).collect();
        assert_eq!(
            keys,
            vec!["hit", "opacity", "always_on_top", "height_ratio"]
        );
        assert_eq!(spec.mod_id, DESCRIPTOR.id);
        assert_eq!(spec.version, SETTINGS_VERSION);
    }

    #[test]
    fn hit_select_default_is_opaque_and_options_match_parser() {
        let spec = settings_spec();
        let field = spec
            .fields
            .iter()
            .find(|f| f.key() == "hit")
            .expect("hit 字段必须在");
        let ModSettingField::Select {
            options, default, ..
        } = field
        else {
            panic!("hit 必须是 Select");
        };
        assert_eq!(default.as_deref(), Some("opaque"));
        let values: Vec<&str> = options.iter().map(|o| o.value.as_str()).collect();
        assert_eq!(values, vec!["opaque", "through", "window"]);
        // 每个选项都必须能被解析函数认出来（否则前端选了也落不到配置上）。
        for value in values {
            assert!(
                crate::config::HitMode::parse(value).is_some(),
                "选项 {value} 解析不出来"
            );
        }
    }

    #[test]
    fn number_bounds_match_the_parser_clamps() {
        let spec = settings_spec();
        let number = |key: &str| -> (f64, f64) {
            let field = spec
                .fields
                .iter()
                .find(|f| f.key() == key)
                .expect("字段存在");
            let ModSettingField::Number { min, max, .. } = field else {
                panic!("{key} 必须是 Number");
            };
            (*min, *max)
        };
        assert_eq!(
            number("opacity"),
            (
                f64::from(crate::config::OPACITY_MIN),
                f64::from(crate::config::OPACITY_MAX)
            )
        );
        assert_eq!(
            number("height_ratio"),
            (
                f64::from(crate::config::HEIGHT_RATIO_MIN),
                f64::from(crate::config::HEIGHT_RATIO_MAX)
            )
        );
    }
}
