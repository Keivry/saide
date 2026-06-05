// SPDX-License-Identifier: MIT OR Apache-2.0

//! 令牌桶速率限制器
//!
//! 通过令牌桶算法限制单位时间内的最大操作次数，
//! 防止突发性高频操作暴露自动化特征。

use std::time::Instant;

/// 令牌桶速率限制器
///
/// 以固定速率生成令牌，每次操作消耗一个令牌。
/// 支持 burst 参数允许短时突发。
pub struct RateLimiter {
    /// 每秒生成的令牌数
    rate_per_sec: f64,
    /// 最大令牌数（burst）
    max_tokens: f64,
    /// 当前令牌数
    tokens: f64,
    /// 上次补充令牌的时间
    last_refill: Instant,
}

impl RateLimiter {
    /// 创建新的速率限制器
    ///
    /// - `rate_per_sec`: 每秒允许的最大操作数
    /// - `burst`: 允许的最大瞬时突发操作数
    pub fn new(rate_per_sec: u32, burst: u32) -> Self {
        Self {
            rate_per_sec: rate_per_sec as f64,
            max_tokens: burst as f64,
            tokens: burst as f64, // 初始满令牌
            last_refill: Instant::now(),
        }
    }

    /// 尝试获取一个令牌
    ///
    /// 返回 `true` 表示允许操作，`false` 表示被限速（需静默跳过）。
    pub fn try_acquire(&mut self) -> bool {
        self.refill();

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// 动态调整速率（用于会话节奏联动）
    ///
    /// 根据当前活跃度倍率调整令牌生成速率。
    /// 调用后令牌桶上限不会超过 `max_tokens`。
    pub fn set_rate(&mut self, rate_per_sec: f64) {
        self.rate_per_sec = rate_per_sec;
        if self.tokens > self.max_tokens {
            self.tokens = self.max_tokens;
        }
    }

    /// 补充令牌
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.last_refill = now;

        self.tokens += elapsed * self.rate_per_sec;
        if self.tokens > self.max_tokens {
            self.tokens = self.max_tokens;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_allows_within_rate() {
        let mut limiter = RateLimiter::new(10, 3);

        // 前 3 次（burst）都应通过
        for _ in 0..3 {
            assert!(
                limiter.try_acquire(),
                "First 3 requests (burst=3) should be allowed"
            );
        }

        // 第 4 次可能因时间不足而被拒绝
        //（依赖实际执行速度，但通常第一次调用 refill 时 elapsed≈0）
        // 不强制断言第 4 次结果，因为测试机速度可能不同
    }

    #[test]
    fn test_burst_allows_spike() {
        let mut limiter = RateLimiter::new(5, 3);

        // 前 3 次在短时间内应全部通过
        let mut passed = 0;
        for _ in 0..3 {
            if limiter.try_acquire() {
                passed += 1;
            }
        }
        assert_eq!(passed, 3, "Burst=3 should allow 3 rapid requests");
    }

    #[test]
    fn test_refill_over_time() {
        let mut limiter = RateLimiter::new(100, 5);

        // 消耗所有令牌
        for _ in 0..5 {
            assert!(limiter.try_acquire());
        }

        // 立即请求应被拒绝
        assert!(!limiter.try_acquire());

        // 等待 30ms 后应有新令牌（100 tok/s × 0.03s = 3 tokens）
        std::thread::sleep(std::time::Duration::from_millis(30));

        let mut passed = 0;
        for _ in 0..3 {
            if limiter.try_acquire() {
                passed += 1;
            }
        }
        // 至少应有 1 个令牌（考虑到时间精度）
        assert!(
            passed >= 1,
            "After 30ms wait, at least 1 token should be available, got {passed}"
        );
    }
}
