// SPDX-License-Identifier: MIT OR Apache-2.0

//! 触摸压力随机化
//!
//! 为触摸事件的 `pressure` 字段生成符合截断高斯分布的随机值，
//! 消除固定 pressure=1.0 的自动化指纹。

use {
    rand::{RngExt, SeedableRng, rngs::SmallRng},
    serde::{Deserialize, Serialize},
};

/// 触摸压力配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TouchParamsConfig {
    /// 压力均值（默认 0.7）
    #[serde(default = "default_pressure_mean")]
    pub pressure_mean: f32,
    /// 压力标准差（默认 0.15）
    #[serde(default = "default_pressure_stddev")]
    pub pressure_stddev: f32,
    /// 压力最小值（默认 0.3）
    #[serde(default = "default_pressure_min")]
    pub pressure_min: f32,
    /// 压力最大值（默认 1.0）
    #[serde(default = "default_pressure_max")]
    pub pressure_max: f32,
}

fn default_pressure_mean() -> f32 { 0.7 }
fn default_pressure_stddev() -> f32 { 0.15 }
fn default_pressure_min() -> f32 { 0.3 }
fn default_pressure_max() -> f32 { 1.0 }

impl Default for TouchParamsConfig {
    fn default() -> Self {
        Self {
            pressure_mean: default_pressure_mean(),
            pressure_stddev: default_pressure_stddev(),
            pressure_min: default_pressure_min(),
            pressure_max: default_pressure_max(),
        }
    }
}

/// 触摸压力生成器
///
/// 使用截断高斯分布生成随机压力值。
pub struct TouchParams {
    rng: SmallRng,
    config: TouchParamsConfig,
}

impl TouchParams {
    /// 创建新的压力生成器
    pub fn new(config: TouchParamsConfig) -> Self {
        Self {
            rng: SmallRng::from_rng(&mut rand::rng()),
            config,
        }
    }

    /// 生成随机压力值
    ///
    /// 使用 Box-Muller 变换生成高斯分布样本，
    /// 截断至 [pressure_min, pressure_max] 范围。
    pub fn generate_pressure(&mut self) -> f32 {
        use std::f64::consts::TAU;

        let u1: f64 = self.rng.random();
        let u2: f64 = self.rng.random();
        let z = (-2.0 * u1.ln()).sqrt() * (TAU * u2).cos();

        let raw = self.config.pressure_mean as f64 + z * self.config.pressure_stddev as f64;
        raw.clamp(
            self.config.pressure_min as f64,
            self.config.pressure_max as f64,
        ) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pressure_in_range() {
        let config = TouchParamsConfig {
            pressure_mean: 0.7,
            pressure_stddev: 0.15,
            pressure_min: 0.3,
            pressure_max: 1.0,
        };
        let mut tp = TouchParams::new(config);
        for _ in 0..1000 {
            let p = tp.generate_pressure();
            assert!(p >= 0.3 && p <= 1.0, "Pressure {p} out of [0.3, 1.0] range");
        }
    }
}
