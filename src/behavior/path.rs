// SPDX-License-Identifier: MIT OR Apache-2.0

//! 贝塞尔曲线路径生成器
//!
//! 为鼠标移动和滑动操作生成三次贝塞尔曲线路径，
//! 支持可配置的采样点数量和缓入缓出（ease-in-out）时间分布。

use {
    rand::{RngExt, SeedableRng, rngs::SmallRng},
    serde::{Deserialize, Serialize},
};

/// 贝塞尔曲线上的一个采样点
#[derive(Debug, Clone, Copy)]
pub struct BezierPoint {
    /// X 坐标
    pub x: f32,
    /// Y 坐标
    pub y: f32,
    /// 时间参数 t（0.0-1.0），用于时序分布
    pub t: f32,
}

/// 路径配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathConfig {
    /// 路径采样点数量（为 0 时按 step_px 自动计算）
    pub path_points: usize,
    /// 每步像素数（path_points 为 0 时使用）
    pub path_step_px: f32,
    /// 控制点偏移比例（相对屏幕尺寸）
    pub control_offset: f32,
}

/// 三次贝塞尔曲线计算
///
/// 实现 design.md D3 中的公式：
/// P(t) = (1-t)³P₀ + 3(1-t)²tP₁ + 3(1-t)t²P₂ + t³P₃
pub fn cubic_bezier(
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
    t: f32,
) -> (f32, f32) {
    let t2 = t * t;
    let t3 = t2 * t;
    let one_minus_t = 1.0 - t;
    let one_minus_t2 = one_minus_t * one_minus_t;
    let one_minus_t3 = one_minus_t2 * one_minus_t;

    let x = one_minus_t3 * p0.0
        + 3.0 * one_minus_t2 * t * p1.0
        + 3.0 * one_minus_t * t2 * p2.0
        + t3 * p3.0;
    let y = one_minus_t3 * p0.1
        + 3.0 * one_minus_t2 * t * p1.1
        + 3.0 * one_minus_t * t2 * p2.1
        + t3 * p3.1;

    (x, y)
}

/// 缓入缓出（ease-in-out）时间分布函数
///
/// 使路径在起点和终点附近速度较慢，中间较快。
pub fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 {
        2.0 * t * t
    } else {
        let t = -2.0 * t + 2.0;
        1.0 - t * t / 2.0
    }
}

/// 路径生成器
pub struct PathGenerator {
    rng: SmallRng,
    config: PathConfig,
}

impl PathGenerator {
    /// 创建新的路径生成器
    pub fn new(config: PathConfig) -> Self {
        Self {
            rng: SmallRng::from_rng(&mut rand::rng()),
            config,
        }
    }

    /// 生成从起点到终点的贝塞尔曲线路径
    ///
    /// - `start_x`, `start_y`: 起点坐标
    /// - `end_x`, `end_y`: 终点坐标
    /// - `screen_width`, `screen_height`: 屏幕尺寸，用于计算控制点偏移
    pub fn generate(
        &mut self,
        start_x: f32,
        start_y: f32,
        end_x: f32,
        end_y: f32,
        screen_width: f32,
        screen_height: f32,
    ) -> Vec<BezierPoint> {
        let num_points = self.calculate_num_points(start_x, start_y, end_x, end_y);

        if num_points <= 2 {
            // 只有起点和终点，不需要贝塞尔曲线
            return vec![
                BezierPoint {
                    x: start_x,
                    y: start_y,
                    t: 0.0,
                },
                BezierPoint {
                    x: end_x,
                    y: end_y,
                    t: 1.0,
                },
            ];
        }

        // 计算两个控制点：在起点-终点连线两侧随机偏移
        let dx = end_x - start_x;
        let dy = end_y - start_y;
        let dist = (dx * dx + dy * dy).sqrt().max(1.0);
        let offset = self.config.control_offset;

        // 垂直方向单位向量
        let nx = -dy / dist;
        let ny = dx / dist;

        // 控制点偏移量
        let max_offset = offset * screen_width.max(screen_height);
        let c1_offset: f32 = self.rng.random_range(-1.0..=1.0) * max_offset;
        let c2_offset: f32 = self.rng.random_range(-1.0..=1.0) * max_offset;

        let p0 = (start_x, start_y);
        let p3 = (end_x, end_y);

        // P1 在起点和终点的 1/3 处，加随机垂直偏移
        let p1 = (
            (start_x + dx * 0.33 + nx * c1_offset).clamp(0.0, screen_width),
            (start_y + dy * 0.33 + ny * c1_offset).clamp(0.0, screen_height),
        );
        // P2 在起点和终点的 2/3 处，加另一个随机垂直偏移
        let p2 = (
            (start_x + dx * 0.67 + nx * c2_offset).clamp(0.0, screen_width),
            (start_y + dy * 0.67 + ny * c2_offset).clamp(0.0, screen_height),
        );

        let mut points = Vec::with_capacity(num_points);

        for i in 0..num_points {
            let t_raw = if num_points == 1 {
                0.0
            } else {
                i as f32 / (num_points - 1) as f32
            };
            // 应用 ease-in-out 时间重映射
            let t = ease_in_out(t_raw);
            let (x, y) = cubic_bezier(p0, p1, p2, p3, t);

            points.push(BezierPoint {
                x: x.clamp(0.0, screen_width),
                y: y.clamp(0.0, screen_height),
                t,
            });
        }

        points
    }

    /// 根据配置计算采样点数量
    fn calculate_num_points(&self, start_x: f32, start_y: f32, end_x: f32, end_y: f32) -> usize {
        if self.config.path_points > 0 {
            self.config.path_points
        } else {
            let dx = end_x - start_x;
            let dy = end_y - start_y;
            let dist = (dx * dx + dy * dy).sqrt();
            (dist / self.config.path_step_px.max(1.0)).ceil() as usize + 1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ease_in_out_boundaries() {
        assert!((ease_in_out(0.0) - 0.0).abs() < f32::EPSILON);
        assert!((ease_in_out(1.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_ease_in_out_symmetry() {
        assert!((ease_in_out(0.5) - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_path_point_count() {
        let config = PathConfig {
            path_points: 8,
            path_step_px: 8.0,
            control_offset: 0.2,
        };
        let mut generator = PathGenerator::new(config);
        let points = generator.generate(100.0, 200.0, 500.0, 600.0, 1080.0, 2340.0);
        assert_eq!(
            points.len(),
            8,
            "Should generate exactly path_points=8 points"
        );
    }

    #[test]
    fn test_path_start_end_exact() {
        let config = PathConfig {
            path_points: 6,
            path_step_px: 8.0,
            control_offset: 0.2,
        };
        let mut generator = PathGenerator::new(config);
        let points = generator.generate(100.0, 200.0, 500.0, 600.0, 1080.0, 2340.0);

        let first = &points[0];
        let last = &points[points.len() - 1];

        assert!(
            (first.x - 100.0).abs() < 1.0 && (first.y - 200.0).abs() < 1.0,
            "First point should be near start: ({}, {})",
            first.x,
            first.y
        );
        assert!(
            (last.x - 500.0).abs() < 1.0 && (last.y - 600.0).abs() < 1.0,
            "Last point should be near end: ({}, {})",
            last.x,
            last.y
        );
    }

    #[test]
    fn test_path_within_bounds() {
        let config = PathConfig {
            path_points: 10,
            path_step_px: 8.0,
            control_offset: 0.2,
        };
        let mut generator = PathGenerator::new(config);
        let width = 1080.0;
        let height = 2340.0;
        let points = generator.generate(100.0, 200.0, 500.0, 600.0, width, height);

        for point in &points {
            assert!(
                point.x >= 0.0 && point.x <= width,
                "Point x={} out of [0, {}] bounds",
                point.x,
                width
            );
            assert!(
                point.y >= 0.0 && point.y <= height,
                "Point y={} out of [0, {}] bounds",
                point.y,
                height
            );
        }
    }
}
