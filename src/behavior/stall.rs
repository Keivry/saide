// SPDX-License-Identifier: MIT OR Apache-2.0

//! 画面停滞检测
//!
//! 检测连续操作后画面是否发生变化，
//! 连续 N 次无变化时判定为停滞并暂停操作。

/// 画面停滞检测器
///
/// 维护最近 N 帧的像素哈希，当连续相同哈希数超过阈值时
/// 判定为画面停滞。
pub struct StallDetector {
    /// 停滞阈值（连续相同帧数）
    threshold: usize,
    /// 上一帧的哈希值
    last_hash: Option<u64>,
    /// 连续相同帧计数器
    consecutive_same: usize,
}

impl StallDetector {
    /// 创建新的停滞检测器
    ///
    /// - `threshold`: 连续多少帧无变化后判定为停滞
    pub fn new(threshold: usize) -> Self {
        Self {
            threshold,
            last_hash: None,
            consecutive_same: 0,
        }
    }

    /// 检查当前帧是否导致停滞
    ///
    /// 传入当前帧的哈希值，返回是否检测到停滞。
    /// - 哈希变化时计数器重置
    /// - 连续相同哈希数超过阈值时返回 true
    pub fn check_stall(&mut self, current_hash: u64) -> bool {
        match self.last_hash {
            Some(prev_hash) if prev_hash == current_hash => {
                self.consecutive_same += 1;
                self.consecutive_same >= self.threshold
            }
            _ => {
                self.consecutive_same = 1;
                self.last_hash = Some(current_hash);
                false
            }
        }
    }

    /// 重置停滞检测状态
    pub fn reset(&mut self) {
        self.last_hash = None;
        self.consecutive_same = 0;
    }

    pub fn is_stalled(&self) -> bool { self.consecutive_same >= self.threshold }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stall_after_threshold() {
        let mut detector = StallDetector::new(10);

        for _ in 0..9 {
            assert!(
                !detector.check_stall(42),
                "Should not stall before threshold"
            );
        }

        assert!(detector.check_stall(42), "Should stall at threshold");
    }

    #[test]
    fn test_stall_reset_on_change() {
        let mut detector = StallDetector::new(10);

        // 4 次相同哈希（第 1 次初始化为 1，再 3 次累加为 4）
        for _ in 0..3 {
            assert!(!detector.check_stall(42));
        }

        // 哈希变化，计数器重置
        assert!(!detector.check_stall(99));

        // 重新计数，8 次不触发（初始化为 1 + 8 = 9 < 10）
        for _ in 0..8 {
            assert!(!detector.check_stall(99));
        }

        // 第 10 次触发（1 + 8 + 1 = 10）
        assert!(detector.check_stall(99));
    }

    #[test]
    fn test_reset_clears_state() {
        let mut detector = StallDetector::new(5);

        for _ in 0..3 {
            detector.check_stall(42);
        }

        detector.reset();

        // 重置后需要重新计数
        assert!(!detector.check_stall(42));
    }
}
