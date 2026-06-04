// SPDX-License-Identifier: MIT OR Apache-2.0

//! 随机延迟生成器
//!
//! 提供基于高斯分布和均匀分布的随机延迟生成，用于模拟
//! 人类操作之间的自然时间间隔波动。

use {
    rand::{RngExt, SeedableRng, rngs::SmallRng},
    serde::{Deserialize, Serialize},
    std::{thread, time::Duration},
};

/// 延迟分布类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DelayDistribution {
    /// 均匀分布
    Uniform,
    /// 高斯分布（截断）
    Gaussian,
}

/// 延迟配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelayConfig {
    /// 延迟均值（毫秒）
    pub mean_ms: u64,
    /// 延迟标准差（毫秒）
    pub stddev_ms: f64,
    /// 延迟最小值（毫秒），截断边界
    pub min_ms: u64,
    /// 延迟最大值（毫秒），截断边界
    pub max_ms: u64,
    /// 分布类型
    pub distribution: DelayDistribution,
}

/// 随机延迟生成器
///
/// 根据配置的分布类型生成随机延迟值，支持同步和异步睡眠。
pub struct DelayGenerator {
    rng: SmallRng,
    config: DelayConfig,
}

impl DelayGenerator {
    /// 创建新的延迟生成器
    pub fn new(config: DelayConfig) -> Self {
        Self {
            rng: SmallRng::from_rng(&mut rand::rng()),
            config,
        }
    }

    /// 生成随机延迟毫秒值
    ///
    /// 根据配置的分布类型生成延迟值，结果截断至 [min_ms, max_ms] 范围。
    pub fn generate_ms(&mut self) -> u64 {
        match self.config.distribution {
            DelayDistribution::Uniform => {
                if self.config.min_ms >= self.config.max_ms {
                    return self.config.min_ms;
                }
                self.rng
                    .random_range(self.config.min_ms..=self.config.max_ms)
            }
            DelayDistribution::Gaussian => self.sample_gaussian(),
        }
    }

    /// 高斯分布采样（Box-Muller 变换 + 截断）
    fn sample_gaussian(&mut self) -> u64 {
        use std::f64::consts::TAU;

        let u1: f64 = self.rng.random();
        let u2: f64 = self.rng.random();
        // Box-Muller: 生成标准正态分布 N(0,1) 样本
        let z = (-2.0 * u1.ln()).sqrt() * (TAU * u2).cos();

        let raw = self.config.mean_ms as f64 + z * self.config.stddev_ms;

        // 截断至 [min_ms, max_ms] 范围
        raw.clamp(self.config.min_ms as f64, self.config.max_ms as f64)
            .round() as u64
    }

    /// 同步睡眠
    ///
    /// 生成随机延迟并调用 `thread::sleep`。
    pub fn sleep(&mut self) {
        let ms = self.generate_ms();
        thread::sleep(Duration::from_millis(ms));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uniform_in_range() {
        let config = DelayConfig {
            mean_ms: 50,
            stddev_ms: 10.0,
            min_ms: 20,
            max_ms: 200,
            distribution: DelayDistribution::Uniform,
        };
        let mut generator = DelayGenerator::new(config);
        for _ in 0..1000 {
            let ms = generator.generate_ms();
            assert!(
                (20..=200).contains(&ms),
                "Uniform delay {ms} out of [20, 200] range"
            );
        }
    }

    #[test]
    fn test_gaussian_in_range() {
        let config = DelayConfig {
            mean_ms: 80,
            stddev_ms: 40.0,
            min_ms: 20,
            max_ms: 200,
            distribution: DelayDistribution::Gaussian,
        };
        let mut generator = DelayGenerator::new(config);
        for _ in 0..1000 {
            let ms = generator.generate_ms();
            assert!(
                (20..=200).contains(&ms),
                "Gaussian delay {ms} out of [20, 200] range"
            );
        }
    }

    #[test]
    fn test_sleep_elapsed() {
        let config = DelayConfig {
            mean_ms: 50,
            stddev_ms: 0.0,
            min_ms: 50,
            max_ms: 50,
            distribution: DelayDistribution::Uniform,
        };
        let mut generator = DelayGenerator::new(config);
        let start = std::time::Instant::now();
        generator.sleep();
        let elapsed = start.elapsed().as_millis() as u64;
        assert!(
            (45..=65).contains(&elapsed),
            "Sleep elapsed {elapsed}ms, expected ~50ms"
        );
    }
}
