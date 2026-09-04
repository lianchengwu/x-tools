pub mod convert;

use convert::{
    TIMEZONE_OPTIONS, from_datetime, from_millis, from_now, from_seconds,
    resolve_timezone_by_index,
};
use serde::{Deserialize, Serialize};
use xtools_sdk::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimePlugin {
    pub seconds: String,
    pub millis: String,
    pub local: String,
    pub tz_index: usize,
    pub sec_ok: bool,
    pub ms_ok: bool,
    pub local_ok: bool,
    pub error_seconds: Option<String>,
    pub error_millis: Option<String>,
    pub error_local: Option<String>,
}

impl XPlugin for TimePlugin {
    fn manifest() -> PluginManifest {
        PluginManifest {
            id: "xtools.time".to_string(),
            name: "时间戳转换".to_string(),
            version: "0.7.0".to_string(),
            description: "Unix 秒/毫秒时间戳与本地时间双向转换工具".to_string(),
            author: "xtools".to_string(),
            mark: "clock".to_string(),
            icon_svg: None,
            window: WindowConfig {
                width: 580,
                height: 600,
                resizable: false,
                title: Some("时间戳转换".to_string()),
            },
            permissions: vec![
                Permission::Clipboard,
                Permission::Timer { interval_ms: 1000 },
            ],
        }
    }

    fn init() -> Result<Self, String> {
        let tz = resolve_timezone_by_index(0);
        let (s, ms, local) = from_now(&tz);

        Ok(Self {
            seconds: s.to_string(),
            millis: ms.to_string(),
            local,
            tz_index: 0,
            sec_ok: true,
            ms_ok: true,
            local_ok: true,
            error_seconds: None,
            error_millis: None,
            error_local: None,
        })
    }

    fn render(&self) -> UiView {
        let tz_options: Vec<SelectOption> = TIMEZONE_OPTIONS
            .iter()
            .enumerate()
            .map(|(i, opt)| SelectOption::new(i.to_string(), opt.label))
            .collect();

        let mut children = Vec::new();

        // 1. Seconds Field Card
        let mut sec_items = vec![
            label("秒 (Unix Timestamp / s)"),
            row(vec![
                text_input("input_seconds", &self.seconds),
                button("copy_seconds", "复制"),
            ]),
        ];
        if let Some(err) = &self.error_seconds {
            sec_items.push(error_label(err));
        }
        children.push(card(sec_items));

        // 2. Millis Field Card
        let mut ms_items = vec![
            label("毫秒 (Unix Timestamp / ms)"),
            row(vec![
                text_input("input_millis", &self.millis),
                button("copy_millis", "复制"),
            ]),
        ];
        if let Some(err) = &self.error_millis {
            ms_items.push(error_label(err));
        }
        children.push(card(ms_items));

        // 3. DateTime Field Card
        let mut dt_items = vec![
            row(vec![
                label("时间 (DateTime)"),
                spacer(),
                select("select_tz", tz_options, self.tz_index),
            ]),
            row(vec![
                text_input("input_local", &self.local),
                primary_button("btn_now", "⚡ 现在"),
                button("copy_local", "复制"),
            ]),
        ];
        if let Some(err) = &self.error_local {
            dt_items.push(error_label(err));
        }
        children.push(card(dt_items));

        UiView::new(column(children))
    }

    fn handle_event(&mut self, event: UiEvent) -> Result<UiResponse, String> {
        let tz = resolve_timezone_by_index(self.tz_index);

        match event {
            UiEvent::Click { id } => match id.as_str() {
                "btn_now" => {
                    let (s, ms, local) = from_now(&tz);
                    self.seconds = s.to_string();
                    self.millis = ms.to_string();
                    self.local = local;
                    self.sec_ok = true;
                    self.ms_ok = true;
                    self.local_ok = true;
                    self.error_seconds = None;
                    self.error_millis = None;
                    self.error_local = None;
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "copy_seconds" => {
                    let trimmed = self.seconds.trim();
                    if !trimmed.is_empty() && trimmed.parse::<i64>().is_ok() {
                        let _ = host::clipboard_write(trimmed);
                        Ok(UiResponse::ShowToast(Toast {
                            message: "已复制秒时间戳".to_string(),
                            level: ToastLevel::Success,
                            duration_ms: 1500,
                        }))
                    } else {
                        Ok(UiResponse::NoChange)
                    }
                }
                "copy_millis" => {
                    let trimmed = self.millis.trim();
                    if !trimmed.is_empty() && trimmed.parse::<i64>().is_ok() {
                        let _ = host::clipboard_write(trimmed);
                        Ok(UiResponse::ShowToast(Toast {
                            message: "已复制毫秒时间戳".to_string(),
                            level: ToastLevel::Success,
                            duration_ms: 1500,
                        }))
                    } else {
                        Ok(UiResponse::NoChange)
                    }
                }
                "copy_local" => {
                    let trimmed = self.local.trim();
                    if !trimmed.is_empty() {
                        let _ = host::clipboard_write(trimmed);
                        Ok(UiResponse::ShowToast(Toast {
                            message: "已复制日期时间".to_string(),
                            level: ToastLevel::Success,
                            duration_ms: 1500,
                        }))
                    } else {
                        Ok(UiResponse::NoChange)
                    }
                }
                _ => Ok(UiResponse::NoChange),
            },
            UiEvent::InputChanged { id, value } => match id.as_str() {
                "input_seconds" => {
                    let trimmed = value.trim();
                    self.seconds = value.clone();
                    if trimmed.is_empty() {
                        self.error_seconds = Some("这一栏是空的\n输入一个值，或点「现在」填入当前时间。".to_string());
                        self.sec_ok = false;
                        return Ok(UiResponse::UpdateView(self.render()));
                    }
                    match trimmed
                        .parse::<i64>()
                        .ok()
                        .and_then(|n| from_seconds(n, &tz).ok())
                    {
                        Some((s, ms, local)) => {
                            self.seconds = s.to_string();
                            self.millis = ms.to_string();
                            self.local = local;
                            self.sec_ok = true;
                            self.ms_ok = true;
                            self.local_ok = true;
                            self.error_seconds = None;
                            self.error_millis = None;
                            self.error_local = None;
                        }
                        None => {
                            self.error_seconds = Some("秒数无效。输入 Unix 秒，或点「现在」。".to_string());
                            self.sec_ok = false;
                        }
                    }
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "input_millis" => {
                    let trimmed = value.trim();
                    self.millis = value.clone();
                    if trimmed.is_empty() {
                        self.error_millis = Some("这一栏是空的\n输入一个值，或点「现在」填入当前时间。".to_string());
                        self.ms_ok = false;
                        return Ok(UiResponse::UpdateView(self.render()));
                    }
                    match trimmed
                        .parse::<i64>()
                        .ok()
                        .and_then(|n| from_millis(n, &tz).ok())
                    {
                        Some((s, ms, local)) => {
                            self.seconds = s.to_string();
                            self.millis = ms.to_string();
                            self.local = local;
                            self.sec_ok = true;
                            self.ms_ok = true;
                            self.local_ok = true;
                            self.error_seconds = None;
                            self.error_millis = None;
                            self.error_local = None;
                        }
                        None => {
                            self.error_millis = Some("毫秒无效。输入 Unix 毫秒，或点「现在」。".to_string());
                            self.ms_ok = false;
                        }
                    }
                    Ok(UiResponse::UpdateView(self.render()))
                }
                "input_local" => {
                    let trimmed = value.trim();
                    self.local = value.clone();
                    if trimmed.is_empty() {
                        self.error_local = Some("这一栏是空的\n输入一个值，或点「现在」填入当前时间。".to_string());
                        self.local_ok = false;
                        return Ok(UiResponse::UpdateView(self.render()));
                    }
                    match from_datetime(trimmed, &tz) {
                        Ok((s, ms, local)) => {
                            self.seconds = s.to_string();
                            self.millis = ms.to_string();
                            self.local = local;
                            self.sec_ok = true;
                            self.ms_ok = true;
                            self.local_ok = true;
                            self.error_seconds = None;
                            self.error_millis = None;
                            self.error_local = None;
                        }
                        Err(_) => {
                            self.error_local = Some("时间格式无效。按 年-月-日 时:分:秒 填写，或点「现在」。".to_string());
                            self.local_ok = false;
                        }
                    }
                    Ok(UiResponse::UpdateView(self.render()))
                }
                _ => Ok(UiResponse::NoChange),
            },
            UiEvent::SelectChanged { id, index, .. } => {
                if id == "select_tz" {
                    self.tz_index = index;
                    let new_tz = resolve_timezone_by_index(index);
                    let sec_str = self.seconds.trim();
                    if let Ok(s) = sec_str.parse::<i64>() {
                        if let Ok((_, _, dt_str)) = from_seconds(s, &new_tz) {
                            self.local = dt_str;
                            self.local_ok = true;
                            self.error_local = None;
                            return Ok(UiResponse::UpdateView(self.render()));
                        }
                    }
                    let ms_str = self.millis.trim();
                    if let Ok(ms) = ms_str.parse::<i64>() {
                        if let Ok((_, _, dt_str)) = from_millis(ms, &new_tz) {
                            self.local = dt_str;
                            self.local_ok = true;
                            self.error_local = None;
                            return Ok(UiResponse::UpdateView(self.render()));
                        }
                    }
                    Ok(UiResponse::UpdateView(self.render()))
                } else {
                    Ok(UiResponse::NoChange)
                }
            }
            _ => Ok(UiResponse::NoChange),
        }
    }
}

export_plugin!(TimePlugin);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_plugin_init_and_render() {
        let plugin = TimePlugin::init().unwrap();
        let view = plugin.render();
        assert!(matches!(view.root, UiNode::Container { .. }));
        assert!(!plugin.seconds.is_empty());
        assert!(!plugin.millis.is_empty());
        assert!(!plugin.local.is_empty());
    }

    #[test]
    fn test_time_plugin_input_seconds() {
        let mut plugin = TimePlugin::init().unwrap();
        let evt = UiEvent::InputChanged {
            id: "input_seconds".to_string(),
            value: "1700000000".to_string(),
        };
        let resp = plugin.handle_event(evt).unwrap();
        assert!(matches!(resp, UiResponse::UpdateView(_)));
        assert_eq!(plugin.seconds, "1700000000");
        assert_eq!(plugin.millis, "1700000000000");
        assert!(plugin.local.contains("2023-11-15"));
        assert!(plugin.error_seconds.is_none());
    }

    #[test]
    fn test_time_plugin_input_invalid_seconds() {
        let mut plugin = TimePlugin::init().unwrap();
        let evt = UiEvent::InputChanged {
            id: "input_seconds".to_string(),
            value: "not_a_number".to_string(),
        };
        let _ = plugin.handle_event(evt).unwrap();
        assert!(plugin.error_seconds.is_some());
        assert!(!plugin.sec_ok);
    }

    #[test]
    fn test_time_plugin_tz_switch() {
        let mut plugin = TimePlugin::init().unwrap();
        plugin.handle_event(UiEvent::InputChanged {
            id: "input_seconds".to_string(),
            value: "1700000000".to_string(),
        }).unwrap();
        let local_utc8 = plugin.local.clone();

        plugin.handle_event(UiEvent::SelectChanged {
            id: "select_tz".to_string(),
            index: 1, // UTC
            value: "1".to_string(),
        }).unwrap();
        assert_ne!(plugin.local, local_utc8);
    }
}
