// SPDX-License-Identifier: MIT OR Apache-2.0

//! 生理性微抖动（Micro-Tremor）
//!
//! 在触摸移动和长按期间叠加 8-12Hz 高频微抖动，
//! 模拟人类手指的生理性震颤。

use {
    rand::{RngExt, SeedableRng, rngs::SmallRng},
    serde::{Deserialize, Serialize},
    std::f64::consts::TAU,
};

/// 微抖动配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicroTremorConfig {
    /// 是否启用微抖动
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 抖动频率（Hz）
    #[serde(default = "default_frequency_hz")]
    pub frequency_hz: f64,
    /// 最大振幅（像素）
    #[serde(default = "default_amplitude_max")]
    pub amplitude_max_px: f64,
    /// 最小振幅（像素）
    #[serde(default = "default_amplitude_min")]
    pub amplitude_min_px: f64,
}

fn default_true() -> bool { true }
fn default_frequency_hz() -> f64 { 10.0 }
fn default_amplitude_max() -> f64 { 1.5 }
fn default_amplitude_min() -> f64 { 0.5 }

impl Default for MicroTremorConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            frequency_hz: default_frequency_hz(),
            amplitude_max_px: default_amplitude_max(),
            amplitude_min_px: default_amplitude_min(),
        }
    }
}

/// 生理性微抖动生成器
///
/// 使用正弦波叠加高斯噪声生成抖动偏移。
/// 振幅与 pressure 负相关：用力按压时震颤减小。
pub struct MicroTremor {
    rng: SmallRng,
    config: MicroTremorConfig,
    /// 当前相位（累计时间，秒）
    phase: f64,
}

impl MicroTremor {
    /// 创建新的微抖动生成器
    pub fn new(config: MicroTremorConfig) -> Self {
        Self {
            rng: SmallRng::from_rng(&mut rand::rng()),
            config,
            phase: 0.0,
        }
    }

    /// 计算当前时刻的抖动偏移
    ///
    /// - `x`, `y`: 原始坐标（仅用于方向计算，此处不直接使用）
    /// - `pressure`: 当前触摸压力值（0.0-1.0），影响振幅
    /// - `dt`: 时间增量（秒）
    ///
    /// 返回 `(dx, dy)` 偏移量。
    pub fn update(&mut self, _x: f32, _y: f32, pressure: f32, dt: f64) -> (f32, f32) {
        if !self.config.enabled {
            return (0.0, 0.0);
        }

        self.phase += dt;

        // 振幅与 pressure 负相关
        let amplitude = self.config.amplitude_max_px
            + (self.config.amplitude_min_px - self.config.amplitude_max_px) * pressure as f64;

        // 正弦波主成分
        let phase_angle = TAU * self.config.frequency_hz * self.phase;
        let sine_x = phase_angle.sin();
        let sine_y = (phase_angle + TAU / 3.0).sin(); // 相位差 120°

        // 高斯噪声
        let noise_x: f64 = self.rng.random::<f64>() * 2.0 - 1.0;
        let noise_y: f64 = self.rng.random::<f64>() * 2.0 - 1.0;

        let dx = ((sine_x * 0.7 + noise_x * 0.3) * amplitude) as f32;
        let dy = ((sine_y * 0.7 + noise_y * 0.3) * amplitude) as f32;

        (dx, dy)
    }

    /// 是否启用
    pub fn is_enabled(&self) -> bool { self.config.enabled }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_amplitude_in_range() {
        let config = MicroTremorConfig {
            enabled: true,
            frequency_hz: 10.0,
            amplitude_max_px: 1.5,
            amplitude_min_px: 0.5,
        };
        let mut tremor = MicroTremor::new(config);

        let mut max_abs = 0.0f32;
        for _ in 0..1000 {
            let (dx, dy) = tremor.update(0.0, 0.0, 0.5, 0.016); // ~60fps
            max_abs = max_abs.max(dx.abs().max(dy.abs()));
        }

        assert!(
            max_abs <= 2.0,
            "Max amplitude {max_abs} should be <= 2.0 px"
        );
    }

    #[test]
    fn test_zero_drift() {
        let config = MicroTremorConfig {
            enabled: true,
            frequency_hz: 10.0,
            amplitude_max_px: 1.5,
            amplitude_min_px: 0.5,
        };
        let mut tremor = MicroTremor::new(config);

        let mut sum_dx = 0.0f64;
        let mut sum_dy = 0.0f64;
        let n = 1000;

        for _ in 0..n {
            let (dx, dy) = tremor.update(0.0, 0.0, 0.5, 0.016);
            sum_dx += dx as f64;
            sum_dy += dy as f64;
        }

        let avg_dx = sum_dx / n as f64;
        let avg_dy = sum_dy / n as f64;

        // 长期均值应接近 0（< 0.1 px）
        assert!(
            avg_dx.abs() < 0.1,
            "Long-term dx drift {avg_dx} should be ≈ 0"
        );
        assert!(
            avg_dy.abs() < 0.1,
            "Long-term dy drift {avg_dy} should be ≈ 0"
        );
    }

    #[test]
    fn test_pressure_inverse() {
        let config = MicroTremorConfig {
            enabled: true,
            frequency_hz: 10.0,
            amplitude_max_px: 1.5,
            amplitude_min_px: 0.5,
        };
        let mut tremor = MicroTremor::new(config);

        // 记录低压时的振幅
        let mut low_pressure_amp = 0.0f32;
        for _ in 0..100 {
            let (dx, dy) = tremor.update(0.0, 0.0, 0.3, 0.016);
            low_pressure_amp = low_pressure_amp.max(dx.abs().max(dy.abs()));
        }

        // 重置并记录高压时的振幅
        tremor.phase = 0.0;
        let mut high_pressure_amp = 0.0f32;
        for _ in 0..100 {
            let (dx, dy) = tremor.update(0.0, 0.0, 1.0, 0.016);
            high_pressure_amp = high_pressure_amp.max(dx.abs().max(dy.abs()));
        }

        // 高压时振幅应更小（或相等）
        assert!(
            high_pressure_amp <= low_pressure_amp + 0.1,
            "High pressure amplitude ({high_pressure_amp}) should be <= low pressure amplitude ({low_pressure_amp})"
        );
    }

    #[test]
    fn test_disabled_returns_zero() {
        let config = MicroTremorConfig {
            enabled: false,
            ..Default::default()
        };
        let mut tremor = MicroTremor::new(config);

        for _ in 0..10 {
            let (dx, dy) = tremor.update(0.0, 0.0, 0.5, 0.016);
            assert_eq!(dx, 0.0);
            assert_eq!(dy, 0.0);
        }
    }
}
