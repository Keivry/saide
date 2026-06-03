// SPDX-License-Identifier: MIT OR Apache-2.0

//! 文本逐字键入模拟器
//!
//! 将整段文本拆分为逐字符发送，字符间插入随机延迟，
//! 模拟人类打字时的自然速度波动。

use {
    crate::behavior::delay::{DelayConfig, DelayDistribution, DelayGenerator},
    serde::{Deserialize, Serialize},
};

/// 文本键入配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypingConfig {
    /// 是否启用逐字键入
    pub enabled: bool,
    /// 字符间延迟均值（毫秒）
    pub char_delay_mean_ms: u64,
    /// 字符间延迟标准差（毫秒）
    pub char_delay_stddev_ms: f64,
}

/// 文本键入模拟器
///
/// 组合 `DelayGenerator` 实现逐字符键入。
pub struct TypingSimulator {
    delay_gen: DelayGenerator,
    config: TypingConfig,
}

impl TypingSimulator {
    /// 创建新的键入模拟器
    pub fn new(config: TypingConfig) -> Self {
        let delay_config = DelayConfig {
            mean_ms: config.char_delay_mean_ms,
            stddev_ms: config.char_delay_stddev_ms,
            min_ms: (config.char_delay_mean_ms as i64 - (3.0 * config.char_delay_stddev_ms) as i64)
                .max(5) as u64,
            max_ms: (config.char_delay_mean_ms as f64 + 3.0 * config.char_delay_stddev_ms) as u64
                + 1,
            distribution: DelayDistribution::Gaussian,
        };
        Self {
            delay_gen: DelayGenerator::new(delay_config),
            config,
        }
    }

    /// 逐字符发送文本
    ///
    /// 对文本中的每个字符调用 `inject_fn`，字符间插入随机延迟。
    /// 如果 `enabled = false`，则一次性整段发送。
    ///
    /// `inject_fn` 接收单个 `char`，应负责将其发送到设备。
    pub fn type_text<F>(&mut self, text: &str, mut inject_fn: F)
    where
        F: FnMut(char),
    {
        if !self.config.enabled || text.is_empty() {
            // 禁用时一次性发送整段文本（通过 inject_fn 逐字符但不延迟）
            for ch in text.chars() {
                inject_fn(ch);
            }
            return;
        }

        let chars: Vec<char> = text.chars().collect();
        for (i, &ch) in chars.iter().enumerate() {
            inject_fn(ch);
            // 最后一个字符后不延迟
            if i < chars.len() - 1 {
                self.delay_gen.sleep();
            }
        }
    }

    /// 返回当前配置
    pub fn config(&self) -> &TypingConfig { &self.config }

    /// 是否启用
    pub fn is_enabled(&self) -> bool { self.config.enabled }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_char_order() {
        let config = TypingConfig {
            enabled: true,
            char_delay_mean_ms: 10,
            char_delay_stddev_ms: 0.0,
        };
        let mut sim = TypingSimulator::new(config);

        let mut received = Vec::new();
        sim.type_text("abc", |c| received.push(c));

        assert_eq!(received, vec!['a', 'b', 'c']);
    }

    #[test]
    fn test_total_duration() {
        let config = TypingConfig {
            enabled: true,
            char_delay_mean_ms: 10,
            char_delay_stddev_ms: 2.0,
        };
        let mut sim = TypingSimulator::new(config);

        let start = std::time::Instant::now();
        let mut count = 0;
        sim.type_text("Hello", |_| count += 1);
        let elapsed = start.elapsed().as_millis() as u64;

        assert_eq!(count, 5, "Should send 5 characters");
        // 5 字符，4 次延迟；每次约 10ms，允许较大范围
        let expected_min = 4u64 * (10u64.saturating_sub(3 * 2));
        let expected_max = 4u64 * (10u64 + 3 * 2 + 10); // +10 容错
        assert!(
            elapsed >= expected_min && elapsed <= expected_max,
            "Elapsed {elapsed}ms not in [{expected_min}, {expected_max}] range"
        );
    }

    #[test]
    fn test_typing_order_and_delay() {
        let config = TypingConfig {
            enabled: true,
            char_delay_mean_ms: 50,
            char_delay_stddev_ms: 10.0,
        };
        let mut sim = TypingSimulator::new(config);

        let mut received = Vec::new();
        let start = std::time::Instant::now();
        sim.type_text("hello", |c| received.push(c));
        let elapsed = start.elapsed().as_millis() as u64;

        assert_eq!(received, vec!['h', 'e', 'l', 'l', 'o']);
        let delay_count = 4u64;
        let char_mean = 50u64;
        let char_3sigma = 3u64 * 10;
        let expected_total_min = delay_count * (char_mean - char_3sigma);
        let expected_total_max = delay_count * (char_mean + char_3sigma);
        assert!(
            elapsed >= expected_total_min && elapsed <= expected_total_max,
            "Elapsed {elapsed}ms not in [{expected_total_min}, {expected_total_max}] for 5-char typing"
        );
    }

    #[test]
    fn test_disabled_sends_all() {
        let config = TypingConfig {
            enabled: false,
            char_delay_mean_ms: 100,
            char_delay_stddev_ms: 10.0,
        };
        let mut sim = TypingSimulator::new(config);

        let mut received = Vec::new();
        let start = std::time::Instant::now();
        sim.type_text("test", |c| received.push(c));
        let elapsed = start.elapsed().as_millis();

        assert_eq!(received, vec!['t', 'e', 's', 't']);
        // 禁用时应非常快（< 50ms）
        assert!(
            elapsed < 50,
            "Disabled typing should be fast, took {elapsed}ms"
        );
    }
}
