// SPDX-License-Identifier: MIT OR Apache-2.0

use serde::{Deserialize, Serialize};

/// 抖动权重策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum JitterWeighting {
    /// 均匀分布
    #[default]
    Uniform,
    /// 中心加权（高斯分布）
    Center,
}

/// 延迟分布类型（与 behavior::delay::DelayDistribution 对齐）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DelayDistributionConfig {
    Uniform,
    Gaussian,
}

/// 反检测行为配置
///
/// 所有字段均为 `Option`，支持部分配置和预设覆盖。
/// 最终有效值由 `merge_with_preset()` 决定。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorConfig {
    /// 预设名称
    pub preset: Option<super::super::behavior::profiles::BehaviorProfile>,

    /// 全局开关
    #[serde(default)]
    pub enabled: Option<bool>,

    /// 坐标抖动幅度（屏幕尺寸百分比，0.0-0.10）
    #[serde(default)]
    pub position_jitter: Option<f32>,

    /// 抖动权重策略
    #[serde(default)]
    pub jitter_weighting: JitterWeighting,

    // --- 操作间延迟 ---
    /// 是否启用操作间延迟
    #[serde(default)]
    pub inter_action_delay_enabled: bool,

    /// 延迟分布类型
    pub delay_distribution: Option<DelayDistributionConfig>,

    /// 延迟均值（毫秒）
    pub delay_mean_ms: Option<u64>,

    /// 延迟标准差
    pub delay_stddev_ms: Option<f64>,

    /// 延迟最小值（毫秒）
    pub delay_min_ms: Option<u64>,

    /// 延迟最大值（毫秒）
    pub delay_max_ms: Option<u64>,

    // --- 动作内延迟（TouchDown→TouchUp） ---
    /// 是否启用动作内延迟
    #[serde(default)]
    pub intra_action_delay_enabled: bool,

    /// Tap 内延迟均值（毫秒）
    pub tap_downup_delay_mean_ms: Option<u64>,

    /// Tap 内延迟标准差
    pub tap_downup_delay_stddev_ms: Option<f64>,

    // --- 路径模拟 ---
    /// 是否启用路径模拟
    #[serde(default)]
    pub path_simulation_enabled: bool,

    /// 路径采样点数量
    pub path_points: Option<usize>,

    /// 每步像素数
    pub path_step_px: Option<f32>,

    /// 控制点偏移比例
    pub control_offset: Option<f32>,

    /// 滑动持续时间最小值（毫秒）
    pub swipe_duration_min_ms: Option<u64>,

    /// 滑动持续时间最大值（毫秒）
    pub swipe_duration_max_ms: Option<u64>,

    // --- 文本键入 ---
    /// 是否启用逐字键入
    #[serde(default)]
    pub char_by_char_enabled: bool,

    /// 字符间延迟均值（毫秒）
    pub char_delay_mean_ms: Option<u64>,

    /// 字符间延迟标准差
    pub char_delay_stddev_ms: Option<f64>,

    // --- 停滞检测 ---
    /// 是否启用停滞检测
    #[serde(default)]
    pub stall_detection_enabled: bool,

    /// 停滞阈值（连续相同帧数）
    pub stall_threshold: Option<usize>,

    // --- 速率限制 ---
    /// 是否启用速率限制
    #[serde(default)]
    pub rate_limit_enabled: bool,

    /// 每秒最大操作数
    pub rate_limit_ops_per_sec: Option<u32>,

    /// 突发允许量
    pub rate_limit_burst: Option<u32>,

    // --- 触摸压力 ---
    /// 是否启用触摸压力随机化
    #[serde(default)]
    pub touch_pressure_enabled: bool,

    /// 压力均值
    pub touch_pressure_mean: Option<f32>,

    /// 压力标准差
    pub touch_pressure_stddev: Option<f32>,

    /// 压力最小值
    pub touch_pressure_min: Option<f32>,

    /// 压力最大值
    pub touch_pressure_max: Option<f32>,

    // --- pointer_id 交替 ---
    /// 是否启用 pointer_id 交替
    #[serde(default)]
    pub pointer_id_alternation_enabled: bool,

    // --- 微抖动 ---
    /// 是否启用微抖动
    #[serde(default)]
    pub micro_tremor_enabled: bool,

    /// 微抖动频率（Hz）
    pub micro_tremor_frequency_hz: Option<f64>,

    /// 最小振幅（像素）
    pub micro_tremor_amplitude_min: Option<f64>,

    /// 最大振幅（像素）
    pub micro_tremor_amplitude_max: Option<f64>,

    // --- 多指间距 ---
    /// 是否启用多指间距抖动
    #[serde(default)]
    pub pinch_jitter_enabled: bool,

    // --- 会话节奏 ---
    /// 是否启用会话节奏管理
    #[serde(default)]
    pub session_rhythm_enabled: bool,

    /// 活跃度更新周期范围（秒）
    pub cycle_duration_sec_range: Option<(f64, f64)>,

    /// 停顿间隔范围（秒）
    pub pause_interval_sec_range: Option<(f64, f64)>,

    /// 停顿时长范围（秒）
    pub pause_duration_sec_range: Option<(f64, f64)>,

    /// 空闲阈值（秒）
    pub idle_threshold_sec: Option<f64>,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        // 使用 balanced 预设作为默认值
        crate::behavior::profiles::BehaviorProfile::default().to_config()
    }
}

impl BehaviorConfig {
    /// 使用预设作为默认值，用户显式配置覆盖
    pub fn merge_with_preset(&self, default_config: &BehaviorConfig) -> BehaviorConfig {
        BehaviorConfig {
            preset: self.preset.or(default_config.preset),
            enabled: self.enabled.or(default_config.enabled),
            position_jitter: self.position_jitter.or(default_config.position_jitter),
            jitter_weighting: self.jitter_weighting,
            inter_action_delay_enabled: self.inter_action_delay_enabled
                || default_config.inter_action_delay_enabled,
            delay_distribution: self
                .delay_distribution
                .or(default_config.delay_distribution),
            delay_mean_ms: self.delay_mean_ms.or(default_config.delay_mean_ms),
            delay_stddev_ms: self.delay_stddev_ms.or(default_config.delay_stddev_ms),
            delay_min_ms: self.delay_min_ms.or(default_config.delay_min_ms),
            delay_max_ms: self.delay_max_ms.or(default_config.delay_max_ms),
            intra_action_delay_enabled: self.intra_action_delay_enabled
                || default_config.intra_action_delay_enabled,
            tap_downup_delay_mean_ms: self
                .tap_downup_delay_mean_ms
                .or(default_config.tap_downup_delay_mean_ms),
            tap_downup_delay_stddev_ms: self
                .tap_downup_delay_stddev_ms
                .or(default_config.tap_downup_delay_stddev_ms),
            path_simulation_enabled: self.path_simulation_enabled
                || default_config.path_simulation_enabled,
            path_points: self.path_points.or(default_config.path_points),
            path_step_px: self.path_step_px.or(default_config.path_step_px),
            control_offset: self.control_offset.or(default_config.control_offset),
            swipe_duration_min_ms: self
                .swipe_duration_min_ms
                .or(default_config.swipe_duration_min_ms),
            swipe_duration_max_ms: self
                .swipe_duration_max_ms
                .or(default_config.swipe_duration_max_ms),
            char_by_char_enabled: self.char_by_char_enabled || default_config.char_by_char_enabled,
            char_delay_mean_ms: self
                .char_delay_mean_ms
                .or(default_config.char_delay_mean_ms),
            char_delay_stddev_ms: self
                .char_delay_stddev_ms
                .or(default_config.char_delay_stddev_ms),
            stall_detection_enabled: self.stall_detection_enabled
                || default_config.stall_detection_enabled,
            stall_threshold: self.stall_threshold.or(default_config.stall_threshold),
            rate_limit_enabled: self.rate_limit_enabled || default_config.rate_limit_enabled,
            rate_limit_ops_per_sec: self
                .rate_limit_ops_per_sec
                .or(default_config.rate_limit_ops_per_sec),
            rate_limit_burst: self.rate_limit_burst.or(default_config.rate_limit_burst),
            touch_pressure_enabled: self.touch_pressure_enabled
                || default_config.touch_pressure_enabled,
            touch_pressure_mean: self
                .touch_pressure_mean
                .or(default_config.touch_pressure_mean),
            touch_pressure_stddev: self
                .touch_pressure_stddev
                .or(default_config.touch_pressure_stddev),
            touch_pressure_min: self
                .touch_pressure_min
                .or(default_config.touch_pressure_min),
            touch_pressure_max: self
                .touch_pressure_max
                .or(default_config.touch_pressure_max),
            pointer_id_alternation_enabled: self.pointer_id_alternation_enabled
                || default_config.pointer_id_alternation_enabled,
            micro_tremor_enabled: self.micro_tremor_enabled || default_config.micro_tremor_enabled,
            micro_tremor_frequency_hz: self
                .micro_tremor_frequency_hz
                .or(default_config.micro_tremor_frequency_hz),
            micro_tremor_amplitude_min: self
                .micro_tremor_amplitude_min
                .or(default_config.micro_tremor_amplitude_min),
            micro_tremor_amplitude_max: self
                .micro_tremor_amplitude_max
                .or(default_config.micro_tremor_amplitude_max),
            pinch_jitter_enabled: self.pinch_jitter_enabled || default_config.pinch_jitter_enabled,
            session_rhythm_enabled: self.session_rhythm_enabled
                || default_config.session_rhythm_enabled,
            cycle_duration_sec_range: self
                .cycle_duration_sec_range
                .or(default_config.cycle_duration_sec_range),
            pause_interval_sec_range: self
                .pause_interval_sec_range
                .or(default_config.pause_interval_sec_range),
            pause_duration_sec_range: self
                .pause_duration_sec_range
                .or(default_config.pause_duration_sec_range),
            idle_threshold_sec: self
                .idle_threshold_sec
                .or(default_config.idle_threshold_sec),
        }
    }

    /// 获取有效 enabled 值（None → true）
    pub fn effective_enabled(&self) -> bool { self.enabled.unwrap_or(true) }

    /// 获取有效 position_jitter 值（None → 0.03）
    pub fn effective_position_jitter(&self) -> f32 { self.position_jitter.unwrap_or(0.03) }

    /// 验证配置参数合法性，对非法值记录 warning 并使用默认值
    pub fn validate(&mut self) {
        use tracing::warn;

        if let Some(jitter) = self.position_jitter
            && (jitter < 0.0 || jitter > 0.10)
        {
            warn!(
                "behavior.position_jitter = {} out of [0.0, 0.10], falling back to 0.03",
                jitter
            );
            self.position_jitter = Some(0.03);
        }

        if let (Some(min), Some(max)) = (self.delay_min_ms, self.delay_max_ms)
            && min > max
        {
            warn!(
                "behavior.delay_min_ms ({}) > delay_max_ms ({}), swapping",
                min, max
            );
            self.delay_min_ms = Some(max);
            self.delay_max_ms = Some(min);
        }

        if let Some(mean) = self.touch_pressure_mean
            && (mean < 0.0 || mean > 1.0)
        {
            warn!(
                "behavior.touch_pressure_mean = {} out of [0.0, 1.0], falling back to 0.7",
                mean
            );
            self.touch_pressure_mean = Some(0.7);
        }
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::behavior::profiles::BehaviorProfile};

    #[test]
    fn test_preset_values() {
        let conservative = BehaviorProfile::Conservative.to_config();
        let balanced = BehaviorProfile::Balanced.to_config();
        let aggressive = BehaviorProfile::Aggressive.to_config();

        // 坐标抖动幅度递增：保守 < 均衡 < 激进
        assert!(
            conservative.effective_position_jitter() < balanced.effective_position_jitter(),
            "conservative jitter ({}) should be less than balanced jitter ({})",
            conservative.effective_position_jitter(),
            balanced.effective_position_jitter()
        );
        assert!(
            balanced.effective_position_jitter() < aggressive.effective_position_jitter(),
            "balanced jitter ({}) should be less than aggressive jitter ({})",
            balanced.effective_position_jitter(),
            aggressive.effective_position_jitter()
        );

        // 延迟均值递增：保守 < 均衡 < 激进
        assert!(
            conservative.delay_mean_ms < balanced.delay_mean_ms,
            "conservative delay should be less than balanced delay"
        );
        assert!(
            balanced.delay_mean_ms < aggressive.delay_mean_ms,
            "balanced delay should be less than aggressive delay"
        );

        // 激进预设启用路径模拟，保守不启用
        assert!(!conservative.path_simulation_enabled);
        assert!(aggressive.path_simulation_enabled);
    }

    #[test]
    fn test_merge_override() {
        let mut user_cfg = BehaviorConfig::default();
        user_cfg.enabled = Some(false);
        user_cfg.delay_mean_ms = Some(999);

        let balanced = BehaviorProfile::Balanced.to_config();
        let merged = user_cfg.merge_with_preset(&balanced);

        // 用户显式设置应覆盖预设
        assert!(
            !merged.effective_enabled(),
            "user disabled should override preset"
        );
        assert_eq!(
            merged.delay_mean_ms,
            Some(999),
            "user delay should override preset"
        );
    }

    #[test]
    fn test_validate_invalid() {
        let mut cfg = BehaviorConfig::default();
        cfg.position_jitter = Some(-1.0); // 超出 [0.0, 0.10]
        cfg.delay_min_ms = Some(100);
        cfg.delay_max_ms = Some(50); // min > max
        cfg.touch_pressure_mean = Some(2.0); // 超出 [0.0, 1.0]

        // 不应 panic
        cfg.validate();

        // 验证非法值被修正
        assert!(
            (cfg.effective_position_jitter() - 0.03).abs() < f32::EPSILON,
            "position_jitter should be reset to default 0.03, got {}",
            cfg.effective_position_jitter()
        );
        assert_eq!(cfg.delay_min_ms, Some(50), "min/max should be swapped");
        assert_eq!(cfg.delay_max_ms, Some(100), "min/max should be swapped");
        assert!(
            (cfg.touch_pressure_mean.unwrap() - 0.7).abs() < f32::EPSILON,
            "touch_pressure_mean should be reset to 0.7, got {}",
            cfg.touch_pressure_mean.unwrap()
        );
    }
}
