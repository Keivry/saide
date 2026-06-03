// SPDX-License-Identifier: MIT OR Apache-2.0

//! 行为预设定义
//!
//! 提供三套命名预设：conservative（保守）、balanced（均衡）、aggressive（激进），
//! 每套预设定义一组完整的反检测参数。

use {
    crate::config::behavior::BehaviorConfig,
    serde::{Deserialize, Serialize},
};

/// 行为预设
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BehaviorProfile {
    /// 保守预设：几乎无延迟，仅保留 ±0.5% 坐标抖动
    Conservative,
    /// 均衡预设（默认）：适中的随机化，平衡反检测与响应速度
    #[default]
    Balanced,
    /// 激进预设：强随机化，最大反检测效果但响应稍慢
    Aggressive,
}

impl BehaviorProfile {
    /// 生成对应预设的完整 BehaviorConfig
    pub fn to_config(self) -> BehaviorConfig {
        match self {
            BehaviorProfile::Conservative => conservative_config(),
            BehaviorProfile::Balanced => balanced_config(),
            BehaviorProfile::Aggressive => aggressive_config(),
        }
    }
}

/// conservative 预设：几乎无延迟，仅保留 ±0.5% 坐标抖动
fn conservative_config() -> BehaviorConfig {
    BehaviorConfig {
        preset: Some(BehaviorProfile::Conservative),
        enabled: true,
        position_jitter: 0.005, // ±0.5%, 与当前 CUSTOM_KEYMAPPING_POS_JITTER 一致
        jitter_weighting: crate::config::behavior::JitterWeighting::Uniform,
        inter_action_delay_enabled: false,
        delay_distribution: Some(crate::config::behavior::DelayDistributionConfig::Gaussian),
        delay_mean_ms: Some(0),
        delay_stddev_ms: Some(0.0),
        delay_min_ms: Some(0),
        delay_max_ms: Some(0),
        intra_action_delay_enabled: false,
        tap_downup_delay_mean_ms: Some(0),
        tap_downup_delay_stddev_ms: Some(0.0),
        path_simulation_enabled: false,
        path_points: Some(0),
        path_step_px: Some(8.0),
        control_offset: Some(0.0),
        swipe_duration_min_ms: Some(0),
        swipe_duration_max_ms: Some(0),
        char_by_char_enabled: false,
        char_delay_mean_ms: Some(0),
        char_delay_stddev_ms: Some(0.0),
        stall_detection_enabled: false,
        stall_threshold: Some(10),
        rate_limit_enabled: false,
        rate_limit_ops_per_sec: Some(0),
        rate_limit_burst: Some(0),
        touch_pressure_enabled: false,
        touch_pressure_mean: Some(0.7),
        touch_pressure_stddev: Some(0.05),
        touch_pressure_min: Some(0.3),
        touch_pressure_max: Some(1.0),
        pointer_id_alternation_enabled: false,
        micro_tremor_enabled: false,
        micro_tremor_frequency_hz: Some(10.0),
        micro_tremor_amplitude_min: Some(0.5),
        micro_tremor_amplitude_max: Some(1.5),
        pinch_jitter_enabled: false,
        session_rhythm_enabled: false,
        cycle_duration_sec_range: Some((300.0, 900.0)),
        pause_interval_sec_range: Some((300.0, 900.0)),
        pause_duration_sec_range: Some((2.0, 10.0)),
        idle_threshold_sec: Some(2.0),
    }
}

/// balanced 预设（默认）：均衡的随机化
fn balanced_config() -> BehaviorConfig {
    BehaviorConfig {
        preset: Some(BehaviorProfile::Balanced),
        enabled: true,
        position_jitter: 0.03, // ±3%
        jitter_weighting: crate::config::behavior::JitterWeighting::Uniform,
        inter_action_delay_enabled: true,
        delay_distribution: Some(crate::config::behavior::DelayDistributionConfig::Gaussian),
        delay_mean_ms: Some(80),
        delay_stddev_ms: Some(40.0),
        delay_min_ms: Some(20),
        delay_max_ms: Some(200),
        intra_action_delay_enabled: true,
        tap_downup_delay_mean_ms: Some(80),
        tap_downup_delay_stddev_ms: Some(30.0),
        path_simulation_enabled: true,
        path_points: Some(6),
        path_step_px: Some(8.0),
        control_offset: Some(0.2),
        swipe_duration_min_ms: Some(100),
        swipe_duration_max_ms: Some(400),
        char_by_char_enabled: true,
        char_delay_mean_ms: Some(50),
        char_delay_stddev_ms: Some(25.0),
        stall_detection_enabled: true,
        stall_threshold: Some(10),
        rate_limit_enabled: true,
        rate_limit_ops_per_sec: Some(10),
        rate_limit_burst: Some(3),
        touch_pressure_enabled: true,
        touch_pressure_mean: Some(0.7),
        touch_pressure_stddev: Some(0.15),
        touch_pressure_min: Some(0.3),
        touch_pressure_max: Some(1.0),
        pointer_id_alternation_enabled: true,
        micro_tremor_enabled: true,
        micro_tremor_frequency_hz: Some(10.0),
        micro_tremor_amplitude_min: Some(0.5),
        micro_tremor_amplitude_max: Some(1.5),
        pinch_jitter_enabled: true,
        session_rhythm_enabled: true,
        cycle_duration_sec_range: Some((300.0, 900.0)),
        pause_interval_sec_range: Some((300.0, 900.0)),
        pause_duration_sec_range: Some((2.0, 10.0)),
        idle_threshold_sec: Some(2.0),
    }
}

/// aggressive 预设：强随机化
fn aggressive_config() -> BehaviorConfig {
    BehaviorConfig {
        preset: Some(BehaviorProfile::Aggressive),
        enabled: true,
        position_jitter: 0.05, // ±5%
        jitter_weighting: crate::config::behavior::JitterWeighting::Center,
        inter_action_delay_enabled: true,
        delay_distribution: Some(crate::config::behavior::DelayDistributionConfig::Gaussian),
        delay_mean_ms: Some(200),
        delay_stddev_ms: Some(80.0),
        delay_min_ms: Some(50),
        delay_max_ms: Some(500),
        intra_action_delay_enabled: true,
        tap_downup_delay_mean_ms: Some(120),
        tap_downup_delay_stddev_ms: Some(40.0),
        path_simulation_enabled: true,
        path_points: Some(10),
        path_step_px: Some(8.0),
        control_offset: Some(0.25),
        swipe_duration_min_ms: Some(200),
        swipe_duration_max_ms: Some(600),
        char_by_char_enabled: true,
        char_delay_mean_ms: Some(100),
        char_delay_stddev_ms: Some(40.0),
        stall_detection_enabled: true,
        stall_threshold: Some(8),
        rate_limit_enabled: true,
        rate_limit_ops_per_sec: Some(5),
        rate_limit_burst: Some(2),
        touch_pressure_enabled: true,
        touch_pressure_mean: Some(0.7),
        touch_pressure_stddev: Some(0.25),
        touch_pressure_min: Some(0.3),
        touch_pressure_max: Some(1.0),
        pointer_id_alternation_enabled: true,
        micro_tremor_enabled: true,
        micro_tremor_frequency_hz: Some(10.0),
        micro_tremor_amplitude_min: Some(0.5),
        micro_tremor_amplitude_max: Some(2.0),
        pinch_jitter_enabled: true,
        session_rhythm_enabled: true,
        cycle_duration_sec_range: Some((180.0, 600.0)),
        pause_interval_sec_range: Some((180.0, 600.0)),
        pause_duration_sec_range: Some((3.0, 15.0)),
        idle_threshold_sec: Some(2.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conservative_preset() {
        let config = BehaviorProfile::Conservative.to_config();
        assert!(!config.inter_action_delay_enabled);
        assert!(!config.char_by_char_enabled);
        assert!(!config.touch_pressure_enabled);
        assert!(!config.pointer_id_alternation_enabled);
        assert!(!config.micro_tremor_enabled);
        assert!(!config.session_rhythm_enabled);
        assert!((config.position_jitter - 0.005).abs() < f32::EPSILON);
    }

    #[test]
    fn test_balanced_preset() {
        let config = BehaviorProfile::Balanced.to_config();
        assert!(config.inter_action_delay_enabled);
        assert_eq!(config.delay_mean_ms, Some(80));
        assert_eq!(config.delay_min_ms, Some(20));
        assert_eq!(config.delay_max_ms, Some(200));
        assert!((config.position_jitter - 0.03).abs() < f32::EPSILON);
        assert_eq!(config.touch_pressure_stddev, Some(0.15));
        assert_eq!(config.micro_tremor_amplitude_max, Some(1.5));
    }

    #[test]
    fn test_aggressive_preset() {
        let config = BehaviorProfile::Aggressive.to_config();
        assert_eq!(config.delay_mean_ms, Some(200));
        assert_eq!(config.delay_max_ms, Some(500));
        assert!((config.position_jitter - 0.05).abs() < f32::EPSILON);
        assert_eq!(config.touch_pressure_stddev, Some(0.25));
        assert_eq!(config.micro_tremor_amplitude_max, Some(2.0));
    }

    #[test]
    fn test_default_is_balanced() {
        let profile = BehaviorProfile::default();
        assert_eq!(profile, BehaviorProfile::Balanced);
    }
}
