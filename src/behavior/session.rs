// SPDX-License-Identifier: MIT OR Apache-2.0

//! 操作节奏周期化管理
//!
//! 模拟人类长时间操作时的自然节奏波动：
//! - 慢周期活跃度波动（5-15 分钟）
//! - 间歇性自然停顿（2-10 秒）

use {
    rand::{RngExt, SeedableRng, rngs::SmallRng},
    serde::{Deserialize, Serialize},
    std::time::Instant,
};

/// 会话节奏配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRhythmConfig {
    /// 是否启用节奏管理
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 活跃度更新周期下限（秒）
    #[serde(default = "default_cycle_min")]
    pub cycle_duration_min_sec: f64,
    /// 活跃度更新周期上限（秒）
    #[serde(default = "default_cycle_max")]
    pub cycle_duration_max_sec: f64,
    /// 停顿间隔下限（秒）
    #[serde(default = "default_pause_interval_min")]
    pub pause_interval_min_sec: f64,
    /// 停顿间隔上限（秒）
    #[serde(default = "default_pause_interval_max")]
    pub pause_interval_max_sec: f64,
    /// 停顿时长下限（秒）
    #[serde(default = "default_pause_duration_min")]
    pub pause_duration_min_sec: f64,
    /// 停顿时长上限（秒）
    #[serde(default = "default_pause_duration_max")]
    pub pause_duration_max_sec: f64,
    /// 空闲阈值（秒），超过此时间无操作才允许停顿
    #[serde(default = "default_idle_threshold")]
    pub idle_threshold_sec: f64,
}

fn default_true() -> bool { true }
fn default_cycle_min() -> f64 { 300.0 }
fn default_cycle_max() -> f64 { 900.0 }
fn default_pause_interval_min() -> f64 { 300.0 }
fn default_pause_interval_max() -> f64 { 900.0 }
fn default_pause_duration_min() -> f64 { 2.0 }
fn default_pause_duration_max() -> f64 { 10.0 }
fn default_idle_threshold() -> f64 { 2.0 }

impl Default for SessionRhythmConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            cycle_duration_min_sec: default_cycle_min(),
            cycle_duration_max_sec: default_cycle_max(),
            pause_interval_min_sec: default_pause_interval_min(),
            pause_interval_max_sec: default_pause_interval_max(),
            pause_duration_min_sec: default_pause_duration_min(),
            pause_duration_max_sec: default_pause_duration_max(),
            idle_threshold_sec: default_idle_threshold(),
        }
    }
}

/// 会话节奏状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// 正常运行
    Active,
    /// 正在停顿
    Paused,
}

/// 会话节奏管理器
pub struct SessionRhythm {
    rng: SmallRng,
    config: SessionRhythmConfig,
    /// 当前活跃度（0.0-1.0）
    pub activity_level: f64,
    /// 上次活跃度更新时间
    last_cycle_update: Instant,
    /// 当前活跃度周期时长（秒）
    current_cycle_duration: f64,
    /// 上次操作时间
    pub last_activity: Instant,
    /// 上次停顿时间
    last_pause: Instant,
    /// 当前停顿时长
    pause_until: Instant,
    /// 当前停顿间隔（秒）
    current_pause_interval: f64,
}

impl SessionRhythm {
    /// 创建新的节奏管理器并初始化周期计时器
    pub fn new(config: SessionRhythmConfig) -> Self {
        let mut rng = SmallRng::from_rng(&mut rand::rng());
        let cycle_duration = if config.enabled {
            rng.random_range(config.cycle_duration_min_sec..=config.cycle_duration_max_sec)
        } else {
            f64::INFINITY
        };
        let pause_interval = if config.enabled {
            rng.random_range(config.pause_interval_min_sec..=config.pause_interval_max_sec)
        } else {
            f64::INFINITY
        };

        let now = Instant::now();
        Self {
            rng,
            config,
            activity_level: 1.0,
            last_cycle_update: now,
            current_cycle_duration: cycle_duration,
            last_activity: now,
            last_pause: now,
            pause_until: now,
            current_pause_interval: pause_interval,
        }
    }

    /// 更新活跃度
    ///
    /// 如果当前周期已过，随机采样新的 activity_level（0.5-1.0）。
    pub fn update_activity_level(&mut self) -> f64 {
        if !self.config.enabled {
            self.activity_level = 1.0;
            return 1.0;
        }

        let now = Instant::now();
        let elapsed = now.duration_since(self.last_cycle_update).as_secs_f64();

        if elapsed >= self.current_cycle_duration {
            self.last_cycle_update = now;
            self.current_cycle_duration = self.rng.random_range(
                self.config.cycle_duration_min_sec..=self.config.cycle_duration_max_sec,
            );
            // activity_level 在 0.5-1.0 之间均匀分布
            self.activity_level = self.rng.random_range(0.5..=1.0_f64);
        }

        self.activity_level
    }

    /// 获取延迟倍率
    ///
    /// 返回 `1.0 + (1 - activity_level) × 0.5`
    pub fn update_delay_modifier(&mut self) -> f64 {
        let level = self.update_activity_level();
        1.0 + (1.0 - level) * 0.5
    }

    /// 获取速率倍率
    ///
    /// 返回 `activity_level`
    pub fn update_rate_multiplier(&mut self) -> f64 {
        self.update_activity_level();
        self.activity_level
    }

    /// 注册操作活动（刷新 last_activity 时间戳）
    pub fn record_activity(&mut self) { self.last_activity = Instant::now(); }

    /// 检查是否应该停顿
    ///
    /// 仅在以下条件全部满足时返回停顿秒数：
    /// 1. 距上次操作超过 idle_threshold
    /// 2. 距上次停顿超过 pause_interval
    ///
    /// 返回 `Some(duration_sec)` 表示应该停顿多久，`None` 表示不需要停顿。
    pub fn should_pause(&mut self) -> Option<f64> {
        if !self.config.enabled {
            return None;
        }

        let now = Instant::now();

        // 如果当前正在停顿中
        if now < self.pause_until {
            return None; // 等待停顿结束
        }

        let idle_duration = now.duration_since(self.last_activity).as_secs_f64();
        let since_last_pause = now.duration_since(self.last_pause).as_secs_f64();

        if idle_duration >= self.config.idle_threshold_sec
            && since_last_pause >= self.current_pause_interval
        {
            let pause_duration = self.rng.random_range(
                self.config.pause_duration_min_sec..=self.config.pause_duration_max_sec,
            );

            self.pause_until = now + std::time::Duration::from_secs_f64(pause_duration);
            self.last_pause = now;
            self.current_pause_interval = self.rng.random_range(
                self.config.pause_interval_min_sec..=self.config.pause_interval_max_sec,
            );

            Some(pause_duration)
        } else {
            None
        }
    }

    /// 检查是否处于停顿状态
    pub fn is_paused(&self) -> bool { Instant::now() < self.pause_until }

    /// 提前退出停顿状态（收到用户输入时调用）
    pub fn interrupt_pause(&mut self) { self.pause_until = Instant::now(); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_activity_level_bounds() {
        let config = SessionRhythmConfig {
            enabled: true,
            ..Default::default()
        };
        let mut rhythm = SessionRhythm::new(config);

        for _ in 0..1000 {
            // 直接强置周期过期以触发更新
            rhythm.last_cycle_update = Instant::now()
                .checked_sub(std::time::Duration::from_secs(1000))
                .unwrap_or(Instant::now());

            let level = rhythm.update_activity_level();
            assert!(
                (0.5..=1.0).contains(&level),
                "activity_level {level} out of [0.5, 1.0]"
            );
        }
    }

    #[test]
    fn test_delay_modifier_formula() {
        let config = SessionRhythmConfig {
            enabled: true,
            ..Default::default()
        };
        let mut rhythm = SessionRhythm::new(config);

        // 强行设置 activity_level = 0.5
        rhythm.activity_level = 0.5;
        // 防止周期更新覆盖
        rhythm.last_cycle_update = Instant::now();

        let modifier = rhythm.update_delay_modifier();
        assert!(
            (modifier - 1.25).abs() < 0.01,
            "delay_modifier should be 1.25 for activity_level=0.5, got {modifier}"
        );
    }

    #[test]
    fn test_pause_respects_idle() {
        let config = SessionRhythmConfig {
            enabled: true,
            idle_threshold_sec: 2.0,
            ..Default::default()
        };
        let mut rhythm = SessionRhythm::new(config);

        // 刚有活动（last_activity 是 now），不应停顿
        rhythm.record_activity();
        assert!(
            rhythm.should_pause().is_none(),
            "Should not pause when activity is recent"
        );
    }

    #[test]
    fn test_disabled_no_pause() {
        let config = SessionRhythmConfig {
            enabled: false,
            ..Default::default()
        };
        let mut rhythm = SessionRhythm::new(config);

        assert!(rhythm.should_pause().is_none());
        assert!((rhythm.update_delay_modifier() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_rhythm_cycle() {
        let config = SessionRhythmConfig {
            enabled: true,
            ..Default::default()
        };
        let mut rhythm = SessionRhythm::new(config);

        // 初始 activity_level 为 1.0
        assert!((rhythm.activity_level - 1.0).abs() < f64::EPSILON);

        // 周期尚未过期，调用 update_activity_level() 不改变值
        let before = rhythm.update_activity_level();
        assert!(
            (before - 1.0).abs() < f64::EPSILON,
            "activity_level should remain 1.0 before cycle expires"
        );

        // 强制周期过期
        rhythm.last_cycle_update = Instant::now()
            .checked_sub(std::time::Duration::from_secs(1000))
            .unwrap_or(Instant::now());

        // 周期过期后，activity_level 应更新到 [0.5, 1.0] 内
        let after = rhythm.update_activity_level();
        assert!(
            (0.5..=1.0).contains(&after),
            "activity_level {after} out of [0.5, 1.0] after cycle expiry"
        );

        // 连续第二次调用应使用缓存值（新周期尚未过期）
        let cached = rhythm.activity_level;
        let next = rhythm.update_activity_level();
        assert!(
            (next - cached).abs() < f64::EPSILON,
            "activity_level should be cached until next cycle expires"
        );

        // delay_modifier 应与当前 activity_level 一致
        let modifier = rhythm.update_delay_modifier();
        let expected = 1.0 + (1.0 - rhythm.activity_level) * 0.5;
        assert!(
            (modifier - expected).abs() < 0.01,
            "delay_modifier {modifier} != expected {expected}"
        );
    }
}
