use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepthMode {
    /// 旧版：底边以中线为中心收缩成梯形，逐行水平压缩。
    /// 形变更夸张、更有"折叠"感，但不是几何正确的透视。
    Stretch,
    /// 新版：把画面当作绕铰链向外旋转的平面，做真透视投影。
    /// 每行等比缩放，几何正确的等腰梯形。
    Perspective,
}

impl Default for DepthMode {
    fn default() -> Self {
        Self::Perspective
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    // ---- 模拟开合屏幕 ----
    pub open_angle: f64,
    pub closed_angle: f64,
    pub close_duration_ms: u64,
    pub hold_duration_ms: u64,
    pub open_duration_ms: u64,

    /// 物理合盖 / 开盖时自动播放对应动画。
    #[serde(default = "default_true")]
    pub auto_demo_on_lid: bool,

    /// 翻开盖子后，等多久才开始抓屏播放动画（毫秒）。
    /// 快机型 300~500 够；老机型或 S3 睡眠可能要 1000~1500。
    #[serde(default = "default_lid_open_delay")]
    pub lid_open_delay_ms: u64,

    // ---- 形变模式 ----
    #[serde(default)]
    pub depth_mode: DepthMode,

    // ---- 效果参数 ----
    pub threshold_angle: f64,
    pub blur_span: f64,
    pub max_blur_radius: f64,
    pub max_dim: f64,
    pub viewing_distance: f64,
    pub recession: f64,
    pub blur_evenness: f64,
    pub dim_reach: f64,
}

fn default_true() -> bool {
    true
}

fn default_lid_open_delay() -> u64 {
    300
}

impl Default for Config {
    fn default() -> Self {
        Self {
            open_angle: 130.0,
            closed_angle: 5.0,
            close_duration_ms: 500,
            hold_duration_ms: 0,
            open_duration_ms: 2000,

            auto_demo_on_lid: true,
            lid_open_delay_ms: 300,

            depth_mode: DepthMode::default(),

            threshold_angle: 90.0,
            blur_span: 60.0,
            max_blur_radius: 70.0,
            max_dim: 0.4,
            viewing_distance: 6.0,
            recession: 1.0,
            blur_evenness: 0.0,
            dim_reach: 0.5,
        }
    }
}

fn config_path() -> PathBuf {
    let dir = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("windows-duo");
    std::fs::create_dir_all(&dir).ok();
    dir.join("config.json")
}

impl Config {
    pub fn load() -> Self {
        std::fs::read_to_string(config_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(config_path(), json);
        }
    }
}