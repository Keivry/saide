// SPDX-License-Identifier: MIT OR Apache-2.0

//! pointer_id 动态交替
//!
//! 在每次触摸操作中动态交替 pointer_id，消除全部事件
//! 使用固定 `POINTER_ID_MOUSE` 的自动化指纹。

use {
    rand::{RngExt, SeedableRng, rngs::SmallRng},
    serde::{Deserialize, Serialize},
};

/// Pointer ID 交替配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointerConfig {
    /// 是否启用 pointer_id 交替
    #[serde(default = "default_true")]
    pub alternation_enabled: bool,
}

fn default_true() -> bool { true }

impl Default for PointerConfig {
    fn default() -> Self {
        Self {
            alternation_enabled: default_true(),
        }
    }
}

/// scrcpy 协议定义的三种指针 ID
pub const POINTER_ID_MOUSE: u64 = u64::MAX;
pub const POINTER_ID_GENERIC_FINGER: u64 = u64::MAX - 1;
pub const POINTER_ID_VIRTUAL_FINGER: u64 = u64::MAX - 2;

/// 可用的 pointer_id 池
pub const POINTER_ID_POOL: [u64; 3] = [
    POINTER_ID_MOUSE,
    POINTER_ID_GENERIC_FINGER,
    POINTER_ID_VIRTUAL_FINGER,
];

/// 指针管理器
///
/// 管理 pointer_id 池，为每次操作分配 pointer_id。
/// 单指操作随机选择，同一操作内保持一致；
/// 多指操作为不同手指分配不同 pointer_id。
pub struct PointerManager {
    rng: SmallRng,
    config: PointerConfig,
    /// 当前操作的 pointer_id 缓存（同一操作内保持不变）
    current_pointer_id: Option<u64>,
    /// 多指操作中已分配的 pointer_id 集合
    used_ids: Vec<u64>,
}

impl PointerManager {
    /// 创建新的指针管理器
    pub fn new(config: PointerConfig) -> Self {
        Self {
            rng: SmallRng::from_rng(&mut rand::rng()),
            config,
            current_pointer_id: None,
            used_ids: Vec::new(),
        }
    }

    /// 获取下一个 pointer_id
    ///
    /// - `new_gesture`: 如果是新一轮单指操作则为 true
    /// - `multi_finger`: 如果是多指操作中的额外手指则为 true
    pub fn next_pointer_id(&mut self, new_gesture: bool, multi_finger: bool) -> u64 {
        if !self.config.alternation_enabled {
            // 禁用时固定使用 POINTER_ID_MOUSE
            return POINTER_ID_MOUSE;
        }

        if new_gesture {
            // 新操作：重置缓存
            self.current_pointer_id = None;
            self.used_ids.clear();
        }

        if multi_finger {
            // 多指操作：分配尚未使用的 pointer_id（排除当前手指和已用 ID）
            let current = self.current_pointer_id.unwrap_or(POINTER_ID_MOUSE);
            let available: Vec<u64> = POINTER_ID_POOL
                .iter()
                .copied()
                .filter(|id| *id != current && !self.used_ids.contains(id))
                .collect();

            let id = if available.is_empty() {
                // 所有 ID 都用过了，随机选一个
                let idx = self.rng.random_range(0..POINTER_ID_POOL.len());
                POINTER_ID_POOL[idx]
            } else {
                let idx = self.rng.random_range(0..available.len());
                available[idx]
            };

            self.used_ids.push(id);
            return id;
        }

        // 单指操作：同一操作内保持相同 pointer_id
        if let Some(id) = self.current_pointer_id {
            return id;
        }

        let idx = self.rng.random_range(0..POINTER_ID_POOL.len());
        let id = POINTER_ID_POOL[idx];
        self.current_pointer_id = Some(id);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_finger_variety() {
        let config = PointerConfig {
            alternation_enabled: true,
        };
        let mut pm = PointerManager::new(config);

        let mut ids = Vec::new();
        for _ in 0..10 {
            ids.push(pm.next_pointer_id(true, false));
        }

        // 10 次操作至少出现 2 种不同 pointer_id
        ids.sort();
        ids.dedup();
        assert!(
            ids.len() >= 2,
            "10 operations should use at least 2 different pointer IDs, got {}",
            ids.len()
        );
    }

    #[test]
    fn test_multi_finger_distinct() {
        let config = PointerConfig {
            alternation_enabled: true,
        };
        let mut pm = PointerManager::new(config);

        let id1 = pm.next_pointer_id(true, false); // 手指 1
        let id2 = pm.next_pointer_id(false, true); // 手指 2（多指）

        assert_ne!(id1, id2, "Multi-finger should assign different pointer IDs");
    }

    #[test]
    fn test_same_gesture_same_id() {
        let config = PointerConfig {
            alternation_enabled: true,
        };
        let mut pm = PointerManager::new(config);

        let id1 = pm.next_pointer_id(true, false);
        // 同一操作内的后续事件（TouchDown 之后再次调用）
        let id2 = pm.next_pointer_id(false, false);

        assert_eq!(id1, id2, "Same gesture should use same pointer_id");
    }

    #[test]
    fn test_disabled_fixed_id() {
        let config = PointerConfig {
            alternation_enabled: false,
        };
        let mut pm = PointerManager::new(config);

        for _ in 0..10 {
            assert_eq!(
                pm.next_pointer_id(true, false),
                POINTER_ID_MOUSE,
                "Disabled should always return POINTER_ID_MOUSE"
            );
        }
    }
}
