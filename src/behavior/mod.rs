// SPDX-License-Identifier: MIT OR Apache-2.0

//! 行为模拟引擎 — 反作弊检测的核心模块
//!
//! 本模块提供人类行为模拟的统一入口 [`BehaviorEngine`]，组合以下子模块：
//!
//! - [`delay`] — 随机延迟生成器（高斯/均匀分布）
//! - [`path`] — 贝塞尔曲线路径生成器
//! - [`typing`] — 文本逐字键入模拟器
//! - [`pressure`] — 触摸压力随机化
//! - [`pointer`] — pointer_id 动态交替
//! - [`tremor`] — 生理性微抖动（8-12Hz）
//! - [`rate_limit`] — 令牌桶速率限制器
//! - [`stall`] — 画面停滞检测
//! - [`session`] — 操作节奏周期化管理
//! - [`profiles`] — 行为预设定义
//!
//! [`BehaviorEngine`]: struct.BehaviorEngine.html
//! [`delay`]: delay/index.html
//! [`path`]: path/index.html
//! [`typing`]: typing/index.html
//! [`pressure`]: pressure/index.html
//! [`pointer`]: pointer/index.html
//! [`tremor`]: tremor/index.html
//! [`rate_limit`]: rate_limit/index.html
//! [`stall`]: stall/index.html
//! [`session`]: session/index.html
//! [`profiles`]: profiles/index.html

pub mod delay;
pub mod path;
pub mod pointer;
pub mod pressure;
pub mod profiles;
pub mod rate_limit;
pub mod session;
pub mod stall;
pub mod tremor;
pub mod typing;

use {
    crate::{
        behavior::{
            delay::{DelayConfig, DelayDistribution, DelayGenerator},
            path::{PathConfig, PathGenerator},
            pointer::{PointerConfig, PointerManager},
            pressure::{TouchParams, TouchParamsConfig},
            rate_limit::RateLimiter,
            session::{SessionRhythm, SessionRhythmConfig},
            stall::StallDetector,
            tremor::{MicroTremor, MicroTremorConfig},
            typing::{TypingConfig, TypingSimulator},
        },
        config::behavior::{BehaviorConfig, DelayDistributionConfig, JitterWeighting},
        controller::control_sender::{
            AndroidMotionEventAction,
            ControlMessage,
            ControlSender,
            Position,
        },
    },
    rand::{RngExt, SeedableRng, rngs::SmallRng},
    std::{f64::consts::TAU, thread, time::Duration},
};

// ─── BehaviorEngine ──────────────────────────────────────────────────────────

/// 行为模拟引擎 — 反作弊检测的统一入口
///
/// 组合所有行为子模块，为触摸、滑动、文本输入等操作提供
/// 可配置的类人行为模拟。支持优雅降级（`enabled = false` 时
/// 回退至原始便捷方法）。
///
/// # 示例
///
/// ```no_run
/// use saide::{behavior::BehaviorEngine, config::behavior::BehaviorConfig};
///
/// let config = BehaviorConfig::default();
/// let mut engine = BehaviorEngine::new(config, 1080, 2340);
/// // engine.execute_tap(500, 300, &sender);
/// ```
pub struct BehaviorEngine {
    /// 全局行为配置
    config: BehaviorConfig,
    /// 随机数生成器（用于坐标抖动等）
    rng: SmallRng,
    /// 操作间随机延迟生成器
    delay_generator: DelayGenerator,
    /// 贝塞尔曲线路径生成器
    path_generator: PathGenerator,
    /// 文本逐字键入模拟器
    typing_simulator: TypingSimulator,
    /// 触摸压力随机化器
    touch_params: TouchParams,
    /// pointer_id 动态交替管理器
    pointer_manager: PointerManager,
    /// 生理性微抖动生成器
    micro_tremor: MicroTremor,
    /// 令牌桶速率限制器
    rate_limiter: RateLimiter,
    /// 操作节奏周期化管理器
    session_rhythm: SessionRhythm,
    /// 画面停滞检测器
    stall_detector: StallDetector,
    /// 屏幕宽度（像素）
    screen_width: u16,
    /// 屏幕高度（像素）
    screen_height: u16,
}

impl BehaviorEngine {
    /// 创建新的行为模拟引擎
    ///
    /// 根据提供的 [`BehaviorConfig`] 初始化所有子模块。
    ///
    /// # 参数
    ///
    /// - `config`: 全局行为配置
    /// - `screen_w`: 设备屏幕宽度（像素）
    /// - `screen_h`: 设备屏幕高度（像素）
    pub fn new(config: BehaviorConfig, screen_w: u16, screen_h: u16) -> Self {
        let delay_generator = DelayGenerator::new(Self::build_delay_config(&config));
        let path_generator = PathGenerator::new(Self::build_path_config(&config));
        let typing_simulator = TypingSimulator::new(Self::build_typing_config(&config));
        let touch_params = TouchParams::new(Self::build_pressure_config(&config));
        let pointer_manager = PointerManager::new(Self::build_pointer_config(&config));
        let micro_tremor = MicroTremor::new(Self::build_tremor_config(&config));
        let rate_limiter = RateLimiter::new(
            config.rate_limit_ops_per_sec.unwrap_or(10),
            config.rate_limit_burst.unwrap_or(3),
        );
        let session_rhythm = SessionRhythm::new(Self::build_rhythm_config(&config));
        let stall_detector = StallDetector::new(config.stall_threshold.unwrap_or(30));

        Self {
            config,
            rng: SmallRng::from_rng(&mut rand::rng()),
            delay_generator,
            path_generator,
            typing_simulator,
            touch_params,
            pointer_manager,
            micro_tremor,
            rate_limiter,
            session_rhythm,
            stall_detector,
            screen_width: screen_w,
            screen_height: screen_h,
        }
    }

    // ── 私有构造辅助 ────────────────────────────────────────────────────

    fn build_delay_config(config: &BehaviorConfig) -> DelayConfig {
        DelayConfig {
            mean_ms: config.delay_mean_ms.unwrap_or(80),
            stddev_ms: config.delay_stddev_ms.unwrap_or(40.0),
            min_ms: config.delay_min_ms.unwrap_or(20),
            max_ms: config.delay_max_ms.unwrap_or(200),
            distribution: match config
                .delay_distribution
                .unwrap_or(DelayDistributionConfig::Gaussian)
            {
                DelayDistributionConfig::Gaussian => DelayDistribution::Gaussian,
                DelayDistributionConfig::Uniform => DelayDistribution::Uniform,
            },
        }
    }

    fn build_path_config(config: &BehaviorConfig) -> PathConfig {
        PathConfig {
            path_points: config.path_points.unwrap_or(6),
            path_step_px: config.path_step_px.unwrap_or(8.0),
            control_offset: config.control_offset.unwrap_or(0.2),
        }
    }

    fn build_typing_config(config: &BehaviorConfig) -> TypingConfig {
        TypingConfig {
            enabled: config.char_by_char_enabled,
            char_delay_mean_ms: config.char_delay_mean_ms.unwrap_or(50),
            char_delay_stddev_ms: config.char_delay_stddev_ms.unwrap_or(25.0),
        }
    }

    fn build_pressure_config(config: &BehaviorConfig) -> TouchParamsConfig {
        TouchParamsConfig {
            pressure_mean: config.touch_pressure_mean.unwrap_or(0.7),
            pressure_stddev: config.touch_pressure_stddev.unwrap_or(0.15),
            pressure_min: config.touch_pressure_min.unwrap_or(0.3),
            pressure_max: config.touch_pressure_max.unwrap_or(1.0),
        }
    }

    fn build_pointer_config(config: &BehaviorConfig) -> PointerConfig {
        PointerConfig {
            alternation_enabled: config.pointer_id_alternation_enabled,
        }
    }

    fn build_tremor_config(config: &BehaviorConfig) -> MicroTremorConfig {
        MicroTremorConfig {
            enabled: config.micro_tremor_enabled,
            frequency_hz: config.micro_tremor_frequency_hz.unwrap_or(10.0),
            amplitude_min_px: config.micro_tremor_amplitude_min.unwrap_or(0.5),
            amplitude_max_px: config.micro_tremor_amplitude_max.unwrap_or(1.5),
        }
    }

    fn build_rhythm_config(config: &BehaviorConfig) -> SessionRhythmConfig {
        SessionRhythmConfig {
            enabled: config.session_rhythm_enabled,
            cycle_duration_min_sec: config
                .cycle_duration_sec_range
                .map(|r| r.0)
                .unwrap_or(300.0),
            cycle_duration_max_sec: config
                .cycle_duration_sec_range
                .map(|r| r.1)
                .unwrap_or(900.0),
            pause_interval_min_sec: config
                .pause_interval_sec_range
                .map(|r| r.0)
                .unwrap_or(300.0),
            pause_interval_max_sec: config
                .pause_interval_sec_range
                .map(|r| r.1)
                .unwrap_or(900.0),
            pause_duration_min_sec: config.pause_duration_sec_range.map(|r| r.0).unwrap_or(2.0),
            pause_duration_max_sec: config.pause_duration_sec_range.map(|r| r.1).unwrap_or(10.0),
            idle_threshold_sec: config.idle_threshold_sec.unwrap_or(2.0),
        }
    }

    // ── 公共 API ────────────────────────────────────────────────────────

    /// 对坐标施加随机抖动
    ///
    /// 根据 `position_jitter` 和 `jitter_weighting` 在原始坐标周围
    /// 添加随机偏移。结果被钳制在屏幕边界内。
    ///
    /// - `jitter_weighting = Uniform`: 均匀分布 `[-jitter_amount, +jitter_amount]`
    /// - `jitter_weighting = Center`: 高斯分布 `N(0, jitter_amount)`
    ///
    /// 抖动幅度 = `position_jitter × screen_dimension`
    pub fn jitter_pos(&mut self, x: u32, y: u32) -> (u32, u32) {
        let jitter_x = self.config.effective_position_jitter() * self.screen_width as f32;
        let jitter_y = self.config.effective_position_jitter() * self.screen_height as f32;

        let (dx, dy) = match self.config.jitter_weighting {
            JitterWeighting::Uniform => {
                let dx: f32 = self.rng.random_range(-jitter_x..jitter_x);
                let dy: f32 = self.rng.random_range(-jitter_y..jitter_y);
                (dx, dy)
            }
            JitterWeighting::Center => {
                let (dx, dy) = self.sample_gaussian_2d(jitter_x, jitter_y);
                (dx, dy)
            }
        };

        let nx = (x as f32 + dx).clamp(0.0, (self.screen_width.saturating_sub(1)) as f32) as u32;
        let ny = (y as f32 + dy).clamp(0.0, (self.screen_height.saturating_sub(1)) as f32) as u32;

        (nx, ny)
    }

    /// 执行触摸点击操作（TouchDown → TouchUp）
    ///
    /// 当 `enabled = false` 时，直接使用 `ControlSender` 便捷方法。
    /// 启用后：经过速率限制 → 会话节奏检查 → 坐标抖动 →
    /// 压力随机化 → pointer_id 交替 → `send_custom()` 注入。
    pub fn execute_tap(&mut self, x: u32, y: u32, sender: &ControlSender) {
        if !self.config.effective_enabled() {
            let _ = sender.send_touch_down(x, y);
            let _ = sender.send_touch_up(x, y);
            return;
        }

        // 速率限制
        if self.config.rate_limit_enabled && !self.rate_limiter.try_acquire() {
            return;
        }

        // 会话节奏：更新活跃度、延迟倍率、速率倍率
        let delay_mod = self.session_rhythm.update_delay_modifier();
        let _rate_mult = self.session_rhythm.update_rate_multiplier();

        // 检查是否需要间歇性停顿
        if let Some(pause_sec) = self.session_rhythm.should_pause() {
            thread::sleep(Duration::from_secs_f64(pause_sec));
            self.session_rhythm.interrupt_pause();
        }

        // 坐标抖动
        let (jx, jy) = self.jitter_pos(x, y);

        // 压力随机化
        let pressure = if self.config.touch_pressure_enabled {
            self.touch_params.generate_pressure()
        } else {
            1.0
        };

        // pointer_id 交替
        let pointer_id = self.pointer_manager.next_pointer_id(true, false);

        // 构造并发送 TouchDown
        let down_msg = ControlMessage::InjectTouchEvent {
            action: AndroidMotionEventAction::Down,
            pointer_id,
            position: Position::new(jx, jy, self.screen_width, self.screen_height),
            pressure,
            action_button: 0,
            buttons: 0,
        };
        let _ = sender.send_custom(&down_msg);

        // 动作内延迟（TouchDown → TouchUp）
        if self.config.intra_action_delay_enabled {
            let raw_delay = self.gen_intra_action_delay_ms() as f64;
            let delay_ms = (raw_delay * delay_mod) as u64;
            thread::sleep(Duration::from_millis(delay_ms));
        }

        // 构造并发送 TouchUp
        let up_msg = ControlMessage::InjectTouchEvent {
            action: AndroidMotionEventAction::Up,
            pointer_id,
            position: Position::new(jx, jy, self.screen_width, self.screen_height),
            pressure,
            action_button: 0,
            buttons: 0,
        };
        let _ = sender.send_custom(&up_msg);

        // 记录活动
        self.session_rhythm.record_activity();

        // Micro-tremor 在单次 tap 中不叠加（仅用于 touchMove 和长按）
    }

    /// 执行滑动操作（TouchDown → TouchMove* → TouchUp）
    ///
    /// 使用贝塞尔曲线生成中间路径点，兼顾坐标抖动、微抖动、
    /// 压力随机化和 pointer_id 交替。
    pub fn execute_swipe(&mut self, from: (u32, u32), to: (u32, u32), sender: &ControlSender) {
        if !self.config.effective_enabled() {
            let _ = sender.send_touch_down(from.0, from.1);
            let _ = sender.send_touch_move(to.0, to.1);
            let _ = sender.send_touch_up(to.0, to.1);
            return;
        }

        // 速率限制
        if self.config.rate_limit_enabled && !self.rate_limiter.try_acquire() {
            return;
        }

        // 会话节奏
        let delay_mod = self.session_rhythm.update_delay_modifier();
        let _rate_mult = self.session_rhythm.update_rate_multiplier();

        if let Some(pause_sec) = self.session_rhythm.should_pause() {
            thread::sleep(Duration::from_secs_f64(pause_sec));
            self.session_rhythm.interrupt_pause();
        }

        // 坐标抖动
        let (jx_start, jy_start) = self.jitter_pos(from.0, from.1);
        let (jx_end, jy_end) = self.jitter_pos(to.0, to.1);

        // 贝塞尔路径
        let points = self.path_generator.generate(
            jx_start as f32,
            jy_start as f32,
            jx_end as f32,
            jy_end as f32,
            self.screen_width as f32,
            self.screen_height as f32,
        );

        if points.is_empty() {
            return;
        }

        let pointer_id = self.pointer_manager.next_pointer_id(true, false);
        let pressure = if self.config.touch_pressure_enabled {
            self.touch_params.generate_pressure()
        } else {
            1.0
        };

        // TouchDown（第一个点）
        let first = &points[0];
        let down_msg = ControlMessage::InjectTouchEvent {
            action: AndroidMotionEventAction::Down,
            pointer_id,
            position: Position::new(
                first.x as u32,
                first.y as u32,
                self.screen_width,
                self.screen_height,
            ),
            pressure,
            action_button: 0,
            buttons: 0,
        };
        let _ = sender.send_custom(&down_msg);

        // TouchMove（中间点）
        let point_count = points.len();
        for (i, point) in points.iter().enumerate().skip(1) {
            let mut px = point.x;
            let mut py = point.y;

            // 微抖动叠加（非最后一个点）
            if self.config.micro_tremor_enabled && i < point_count - 1 {
                let dt = 0.016; // ~60fps 帧间隔
                let (dx, dy) = self.micro_tremor.update(px, py, pressure, dt);
                px += dx;
                py += dy;
            }

            let move_msg = ControlMessage::InjectTouchEvent {
                action: AndroidMotionEventAction::Move,
                pointer_id,
                position: Position::new(
                    px as u32,
                    py as u32,
                    self.screen_width,
                    self.screen_height,
                ),
                pressure,
                action_button: 0,
                buttons: 0,
            };
            let _ = sender.send_custom(&move_msg);

            // 点间延迟（应用会话节奏修饰符）
            let raw_delay = self.delay_generator.generate_ms() as f64;
            thread::sleep(Duration::from_millis((raw_delay * delay_mod) as u64));
        }

        // TouchUp（最后一个点）
        let last = points.last().unwrap();
        let up_msg = ControlMessage::InjectTouchEvent {
            action: AndroidMotionEventAction::Up,
            pointer_id,
            position: Position::new(
                last.x as u32,
                last.y as u32,
                self.screen_width,
                self.screen_height,
            ),
            pressure,
            action_button: 0,
            buttons: 0,
        };
        let _ = sender.send_custom(&up_msg);

        self.session_rhythm.record_activity();
    }

    /// 执行文本输入（逐字符或整段发送）
    ///
    /// `inject_fn` 接收单个 `char`，由调用者负责将其注入设备
    /// （例如通过 `ControlMessage::InjectKeycode`）。
    ///
    /// 若 `char_by_char_enabled = false`，则直接逐字符调用 `inject_fn`
    /// 而不插入延迟。
    pub fn execute_text<F: FnMut(char)>(&mut self, text: &str, inject_fn: F) {
        self.typing_simulator.type_text(text, inject_fn);
    }

    /// 获取下一个 pointer_id（供调用者在多事件序列中保持一致性）
    pub fn next_pointer_id(&mut self, new_gesture: bool, multi_finger: bool) -> u64 {
        self.pointer_manager
            .next_pointer_id(new_gesture, multi_finger)
    }

    /// 生成触摸压力值
    pub fn generate_pressure(&mut self) -> f32 {
        if self.config.touch_pressure_enabled {
            self.touch_params.generate_pressure()
        } else {
            1.0
        }
    }

    /// 发送单次触摸按下事件（带 pointer_id + pressure，不经过率限制）
    pub fn send_touch_down_event(
        &mut self,
        x: u32,
        y: u32,
        sender: &ControlSender,
        pointer_id: u64,
    ) {
        let pressure = self.generate_pressure();
        let (jx, jy) = self.jitter_pos(x, y);
        let msg =
            self.build_touch_event(AndroidMotionEventAction::Down, jx, jy, pointer_id, pressure);
        sender.send_custom(&msg).ok();
    }

    /// 发送单次触摸移动事件
    pub fn send_touch_move_event(
        &mut self,
        x: u32,
        y: u32,
        sender: &ControlSender,
        pointer_id: u64,
    ) {
        let pressure = self.generate_pressure();
        let (jx, jy) = self.jitter_pos(x, y);
        let msg =
            self.build_touch_event(AndroidMotionEventAction::Move, jx, jy, pointer_id, pressure);
        sender.send_custom(&msg).ok();
    }

    /// 发送单次触摸抬起事件
    pub fn send_touch_up_event(&mut self, x: u32, y: u32, sender: &ControlSender, pointer_id: u64) {
        let pressure = self.generate_pressure();
        let (jx, jy) = self.jitter_pos(x, y);
        let msg =
            self.build_touch_event(AndroidMotionEventAction::Up, jx, jy, pointer_id, pressure);
        sender.send_custom(&msg).ok();
    }

    /// 构造参数化触摸事件
    ///
    /// 不通过 `BehaviorEngine` 发送，仅返回构造好的 [`ControlMessage`]，
    /// 供调用者自行发送。
    #[allow(dead_code)]
    pub(crate) fn build_touch_event(
        &self,
        action: AndroidMotionEventAction,
        x: u32,
        y: u32,
        pointer_id: u64,
        pressure: f32,
    ) -> ControlMessage {
        ControlMessage::InjectTouchEvent {
            action,
            pointer_id,
            position: Position::new(x, y, self.screen_width, self.screen_height),
            pressure,
            action_button: 0,
            buttons: 0,
        }
    }

    /// 多指操作间距抖动
    ///
    /// 在理想指间距上叠加高斯分布随机偏移（σ = `ideal_distance × 0.01`），
    /// 截断至 ±3%。按压力度（`pressure`）正向关联偏移幅度。
    pub fn jitter_pinch_distance(&mut self, ideal_distance: f32, pressure: f32) -> f32 {
        let sigma = (ideal_distance as f64 * 0.01).max(f64::EPSILON);
        let z = self.sample_gaussian();
        let offset = z * sigma;

        // pressure 增大偏移幅度
        let scaled = offset * (1.0 + pressure as f64 * 0.5);

        // 截断至 ±3%
        let max_offset = ideal_distance as f64 * 0.03;
        let clamped = scaled.clamp(-max_offset, max_offset);

        ideal_distance + clamped as f32
    }

    /// 更新屏幕尺寸
    ///
    /// 应在设备旋转或分辨率变化时调用。
    pub fn update_screen_size(&mut self, w: u16, h: u16) {
        self.screen_width = w;
        self.screen_height = h;
    }

    /// 动态重配置行为参数
    ///
    /// 使用新的 [`BehaviorConfig`] 重新初始化所有子模块，
    /// 同时保留屏幕尺寸。
    pub fn reconfigure(&mut self, config: BehaviorConfig) {
        self.delay_generator = DelayGenerator::new(Self::build_delay_config(&config));
        self.path_generator = PathGenerator::new(Self::build_path_config(&config));
        self.typing_simulator = TypingSimulator::new(Self::build_typing_config(&config));
        self.touch_params = TouchParams::new(Self::build_pressure_config(&config));
        self.pointer_manager = PointerManager::new(Self::build_pointer_config(&config));
        self.micro_tremor = MicroTremor::new(Self::build_tremor_config(&config));
        self.rate_limiter = RateLimiter::new(
            config.rate_limit_ops_per_sec.unwrap_or(10),
            config.rate_limit_burst.unwrap_or(3),
        );
        self.session_rhythm = SessionRhythm::new(Self::build_rhythm_config(&config));
        self.stall_detector = StallDetector::new(config.stall_threshold.unwrap_or(30));
        self.config = config;
    }

    /// 检查画面是否停滞
    ///
    /// 传入当前帧哈希值。若未启用停滞检测或未检测到停滞则返回 `false`。
    pub fn check_frame_hash(&mut self, frame_hash: u64) -> bool {
        if !self.config.stall_detection_enabled {
            return false;
        }
        self.stall_detector.check_stall(frame_hash)
    }

    /// 返回当前屏幕宽度
    pub fn screen_width(&self) -> u16 { self.screen_width }

    /// 返回当前屏幕高度
    pub fn screen_height(&self) -> u16 { self.screen_height }

    // ── 私有辅助方法 ────────────────────────────────────────────────────

    /// 生成动作内延迟（Tap 的 TouchDown → TouchUp 间隔）
    ///
    /// 使用截断高斯分布：mean 取自配置、σ 取自配置、
    /// 截断至 [30, 250] ms。
    fn gen_intra_action_delay_ms(&mut self) -> u64 {
        let mean = self.config.tap_downup_delay_mean_ms.unwrap_or(80) as f64;
        let stddev = self.config.tap_downup_delay_stddev_ms.unwrap_or(30.0);
        let z = self.sample_gaussian();
        let raw = mean + z * stddev;
        raw.clamp(30.0, 250.0).round() as u64
    }

    /// 单个标准正态分布样本（Box-Muller 变换）
    fn sample_gaussian(&mut self) -> f64 {
        let u1: f64 = self.rng.random();
        let u2: f64 = self.rng.random();
        (-2.0 * u1.ln()).sqrt() * (TAU * u2).cos()
    }

    /// 生成二维独立高斯样本（用于坐标抖动 Center 模式）
    fn sample_gaussian_2d(&mut self, sigma_x: f32, sigma_y: f32) -> (f32, f32) {
        let z1 = self.sample_gaussian();
        let z2 = self.sample_gaussian();
        (z1 as f32 * sigma_x, z2 as f32 * sigma_y)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::behavior::pointer::{
            POINTER_ID_GENERIC_FINGER,
            POINTER_ID_MOUSE,
            POINTER_ID_VIRTUAL_FINGER,
        },
        std::{
            io::Read,
            net::{TcpListener, TcpStream},
            thread,
            time::Instant,
        },
    };

    // ── 测试辅助 ────────────────────────────────────────────────────────

    /// 创建一个 mock TCP 服务器，返回 `(TcpStream, JoinHandle<Vec<u8>>)`。
    /// 调用者通过 stream 构造 `ControlSender`；`handle` 用于获取捕获的字节。
    fn setup_mock_server() -> (TcpStream, thread::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut all_data = Vec::new();
            let mut buf = [0u8; 1024];
            stream
                .set_read_timeout(Some(Duration::from_millis(500)))
                .unwrap();
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 {
                    break;
                }
                all_data.extend_from_slice(&buf[..n]);
            }
            all_data
        });

        thread::sleep(Duration::from_millis(10));
        let stream = TcpStream::connect(addr).unwrap();
        (stream, handle)
    }

    /// 创建默认 balanced 配置的 BehaviorEngine
    fn make_engine() -> BehaviorEngine {
        BehaviorEngine::new(BehaviorConfig::default(), 1080, 2340)
    }

    /// 创建 conservative 配置（启用了但极轻微）的 BehaviorEngine
    fn make_conservative_engine() -> BehaviorEngine {
        let config = crate::behavior::profiles::BehaviorProfile::Conservative.to_config();
        BehaviorEngine::new(config, 1080, 2340)
    }

    /// 创建 aggressive 配置的 BehaviorEngine
    fn make_aggressive_engine() -> BehaviorEngine {
        let config = crate::behavior::profiles::BehaviorProfile::Aggressive.to_config();
        BehaviorEngine::new(config, 1080, 2340)
    }

    // ── jitter_pos 测试 ─────────────────────────────────────────────────

    #[test]
    fn test_jitter_pos_within_bounds() {
        let mut engine = make_engine();
        let w = engine.screen_width() as f32;
        let h = engine.screen_height() as f32;
        let jitter = engine.config.effective_position_jitter();

        for _ in 0..500 {
            let (jx, jy) = engine.jitter_pos(540, 1170); // screen center
            // f32 jitter 偏移在 clamp → as u32 时会截断，需要向上取整作为边界
            let max_offset_x = (jitter * w).ceil() as u32;
            let max_offset_y = (jitter * h).ceil() as u32;

            assert!(
                jx >= 540u32.saturating_sub(max_offset_x)
                    && jx
                        <= 540u32
                            .saturating_add(max_offset_x)
                            .min(engine.screen_width() as u32 - 1),
                "jitter_pos x={jx} should be near 540 ± {max_offset_x}"
            );
            assert!(
                jy >= 1170u32.saturating_sub(max_offset_y)
                    && jy
                        <= 1170u32
                            .saturating_add(max_offset_y)
                            .min(engine.screen_height() as u32 - 1),
                "jitter_pos y={jy} should be near 1170 ± {max_offset_y}"
            );
        }
    }

    #[test]
    fn test_jitter_pos_center_weighting() {
        // Aggressive 预设使用 Center (gaussian) 加权
        let mut engine = make_aggressive_engine();
        assert_eq!(engine.config.jitter_weighting, JitterWeighting::Center);

        let center = (540u32, 1170u32);
        let jitter_x = engine.config.effective_position_jitter() * engine.screen_width as f32;
        let half_x = (jitter_x * 0.5).ceil() as u32;
        let total = 2000u32;

        let mut within_half_jitter = 0u32;
        for _ in 0..total {
            let (jx, _) = engine.jitter_pos(center.0, center.1);
            if jx >= center.0.saturating_sub(half_x) && jx <= center.0.saturating_add(half_x) {
                within_half_jitter += 1;
            }
        }

        // 高斯分布 0.5σ 内约 38.3%，保守断言 > 30%
        assert!(
            within_half_jitter as f64 > total as f64 * 0.30,
            "Center-weighted jitter should have >30% samples within half-range (got {}/{})",
            within_half_jitter,
            total
        );
    }

    #[test]
    fn test_jitter_pos_clamped_to_bounds() {
        let mut engine = make_engine();

        // 角落坐标：抖动不应越界
        for _ in 0..200 {
            let (jx, jy) = engine.jitter_pos(0, 0);
            assert!(jx < engine.screen_width as u32);
            assert!(jy < engine.screen_height as u32);
        }

        for _ in 0..200 {
            let w = engine.screen_width as u32 - 1;
            let h = engine.screen_height as u32 - 1;
            let (jx, jy) = engine.jitter_pos(w, h);
            assert!(jx <= w);
            assert!(jy <= h);
        }
    }

    // ── jitter_pinch_distance 测试 ──────────────────────────────────────

    #[test]
    fn test_jitter_pinch_distance_range() {
        let mut engine = make_engine();
        let ideal = 200.0f32;
        let max_deviation = ideal * 0.03; // ±3%

        for _ in 0..500 {
            let result = engine.jitter_pinch_distance(ideal, 0.7);
            assert!(
                (result - ideal).abs() <= max_deviation + f32::EPSILON,
                "jitter_pinch_distance({ideal}) = {result}, deviation {:.3} > ±{:.3}",
                (result - ideal).abs(),
                max_deviation
            );
        }
    }

    #[test]
    fn test_jitter_pinch_distance_pressure_effect() {
        let mut engine = make_engine();
        let ideal = 200.0f32;

        // 采样多次取平均绝对值偏移
        fn avg_abs_offset(engine: &mut BehaviorEngine, ideal: f32, pressure: f32, n: usize) -> f32 {
            let mut sum = 0.0f32;
            for _ in 0..n {
                let result = engine.jitter_pinch_distance(ideal, pressure);
                sum += (result - ideal).abs();
            }
            sum / n as f32
        }

        let low_pressure_avg = avg_abs_offset(&mut engine, ideal, 0.3, 300);
        let high_pressure_avg = avg_abs_offset(&mut engine, ideal, 1.0, 300);

        // 高压时偏移应 ≥ 低压（正向关联）
        assert!(
            high_pressure_avg >= low_pressure_avg * 0.8,
            "High pressure avg offset ({high_pressure_avg}) should be >= 0.8× low ({low_pressure_avg})"
        );
    }

    // ── build_touch_event 测试 ─────────────────────────────────────────

    #[test]
    fn test_build_touch_event_fields() {
        let engine = make_engine();
        let msg = engine.build_touch_event(
            AndroidMotionEventAction::Down,
            100,
            200,
            POINTER_ID_GENERIC_FINGER,
            0.85,
        );

        match msg {
            ControlMessage::InjectTouchEvent {
                action,
                pointer_id,
                position,
                pressure,
                action_button,
                buttons,
            } => {
                // action 通过模式匹配已验证，此处仅验证值字段
                let _ = action;
                assert_eq!(pointer_id, POINTER_ID_GENERIC_FINGER);
                assert!((pressure - 0.85).abs() < f32::EPSILON);
                assert_eq!(action_button, 0);
                assert_eq!(buttons, 0);
                // Position 值：验证 x, y, screen 信息
                assert_eq!(position.x, 100);
                assert_eq!(position.y, 200);
                assert_eq!(position.screen_width, engine.screen_width());
                assert_eq!(position.screen_height, engine.screen_height());
            }
            _ => panic!("Expected InjectTouchEvent variant"),
        }
    }

    // ── enabled=false 降级路径测试 ──────────────────────────────────────

    #[test]
    fn test_enabled_false_fallback() {
        // conservative 预设：enabled=true 但 pointer/pressure 关闭
        // 构造一个 enabled=false 的配置
        let config = BehaviorConfig {
            enabled: Some(false),
            ..Default::default()
        };
        let mut engine = BehaviorEngine::new(config, 1080, 2340);

        let (stream, handle) = setup_mock_server();
        let sender = ControlSender::new(stream, 1080, 2340);

        let start = Instant::now();
        engine.execute_tap(500, 300, &sender);

        drop(sender);
        let data = handle.join().unwrap();

        let elapsed = start.elapsed().as_millis();
        // enabled=false 时不应有额外延迟（< 100ms）
        assert!(
            elapsed < 100,
            "Disabled tap should be fast, took {elapsed}ms"
        );

        // 应有 2 个触摸事件：TouchDown + TouchUp = 64 字节
        assert_eq!(
            data.len(),
            64,
            "Expected 64 bytes (2 events), got {}",
            data.len()
        );

        // 检查事件类型和 action
        assert_eq!(data[0], 2, "First event should be touch type");
        assert_eq!(data[1], 0, "First event should be Down action");
        assert_eq!(data[32], 2, "Second event should be touch type");
        assert_eq!(data[33], 1, "Second event should be Up action");

        // 禁用时 pointer_id 应为 POINTER_ID_MOUSE (u64::MAX) = 8 字节 0xFF
        let pid_bytes = &data[2..10];
        assert_eq!(
            pid_bytes, &[0xFFu8; 8],
            "Disabled mode should use POINTER_ID_MOUSE"
        );
    }

    #[test]
    fn test_enabled_false_does_not_panic() {
        let config = BehaviorConfig {
            enabled: Some(false),
            ..Default::default()
        };
        let mut engine = BehaviorEngine::new(config, 1080, 2340);

        let (stream, _handle) = setup_mock_server();
        let sender = ControlSender::new(stream, 1080, 2340);

        // 不应 panic
        engine.execute_tap(500, 300, &sender);
        engine.execute_swipe((100, 200), (300, 400), &sender);
    }

    // ── reconfigure 测试 ────────────────────────────────────────────────

    #[test]
    fn test_reconfigure_updates_behavior() {
        let mut engine = make_conservative_engine();

        // Conservative: 几乎无延迟
        let delay_before = {
            let start = Instant::now();
            // accumulate a few gen calls to warm up
            engine.delay_generator.generate_ms();
            engine.delay_generator.generate_ms();
            start.elapsed()
        };
        assert!(
            delay_before.as_millis() < 20,
            "Conservative should have near-zero delay"
        );

        // 重配置为 aggressive
        let aggressive_config = crate::behavior::profiles::BehaviorProfile::Aggressive.to_config();
        engine.reconfigure(aggressive_config);

        // Aggressive: delay_mean_ms = 200, min=50
        let test_delay = engine.delay_generator.generate_ms();
        assert!(
            test_delay >= 50,
            "After aggressive reconfigure, delay should be >= 50ms, got {test_delay}"
        );

        // 验证 pointer_manager 也被更新
        let id = engine.pointer_manager.next_pointer_id(true, false);
        // Aggressive 启用了 pointer 交替，但单次采样可能是 MOUSE
        assert!(
            [
                POINTER_ID_MOUSE,
                POINTER_ID_GENERIC_FINGER,
                POINTER_ID_VIRTUAL_FINGER
            ]
            .contains(&id),
            "pointer_id should be one of the three known IDs"
        );
    }

    // ── Rate limiter 跳过测试 ───────────────────────────────────────────

    #[test]
    fn test_tap_rate_limited_skip() {
        // 使用极低速率（1 tok/s, burst=1）确保速率限制确实阻断
        let config = BehaviorConfig {
            rate_limit_enabled: true,
            rate_limit_ops_per_sec: Some(1),
            rate_limit_burst: Some(1),
            ..Default::default()
        };
        let mut engine = BehaviorEngine::new(config, 1080, 2340);

        let (stream, handle) = setup_mock_server();
        let sender = ControlSender::new(stream, 1080, 2340);

        // 第一次 tap 消耗唯一 burst 令牌
        engine.execute_tap(500, 300, &sender);

        // 第二次 tap 应立即被限速拒绝
        engine.execute_tap(500, 300, &sender);

        drop(sender);
        let data = handle.join().unwrap();

        // 仅 1 次 tap（2 事件 = 64 字节）
        assert_eq!(
            data.len(),
            64,
            "Rate-limited 2nd tap should be silently skipped, got {} bytes",
            data.len()
        );
    }

    // ── execute_text 测试 ────────────────────────────────────────────────

    #[test]
    fn test_execute_text_char_order() {
        let config = BehaviorConfig::default();
        let mut engine = BehaviorEngine::new(config, 1080, 2340);

        let mut received = Vec::new();
        engine.execute_text("hello", |c| received.push(c));

        assert_eq!(received, vec!['h', 'e', 'l', 'l', 'o']);
    }

    #[test]
    fn test_execute_text_disabled_no_delay() {
        let config = BehaviorConfig {
            char_by_char_enabled: false,
            char_delay_mean_ms: Some(100),
            ..Default::default()
        };
        let mut engine = BehaviorEngine::new(config, 1080, 2340);

        let mut received = Vec::new();
        let start = Instant::now();
        engine.execute_text("test", |c| received.push(c));
        let elapsed = start.elapsed().as_millis();

        assert_eq!(received, vec!['t', 'e', 's', 't']);
        assert!(
            elapsed < 50,
            "Disabled char-by-char should be fast, took {elapsed}ms"
        );
    }

    // ── update_screen_size 测试 ──────────────────────────────────────────

    #[test]
    fn test_update_screen_size() {
        let mut engine = make_engine();
        assert_eq!(engine.screen_width(), 1080);
        assert_eq!(engine.screen_height(), 2340);

        engine.update_screen_size(2340, 1080);
        assert_eq!(engine.screen_width(), 2340);
        assert_eq!(engine.screen_height(), 1080);
    }

    // ── pointer_id 有效性检查 ───────────────────────────────────────────

    #[test]
    fn test_pointer_id_in_known_pool() {
        let mut engine = make_engine();

        for _ in 0..20 {
            let id = engine.pointer_manager.next_pointer_id(true, false);
            assert!(
                [
                    POINTER_ID_MOUSE,
                    POINTER_ID_GENERIC_FINGER,
                    POINTER_ID_VIRTUAL_FINGER
                ]
                .contains(&id),
                "pointer_id {id} should be in known pool"
            );
        }
    }

    // ── 多操作集成测试 ──────────────────────────────────────────────────

    #[test]
    fn test_multi_operation_realistic_workflow() {
        let (stream, handle) = setup_mock_server();
        let sender = ControlSender::new(stream, 1080, 2340);
        let mut engine = make_engine();

        engine.execute_tap(300, 200, &sender);
        engine.execute_swipe((300, 200), (500, 400), &sender);

        let (jx, jy) = engine.jitter_pos(500, 400);
        assert!(jx < 1080, "Jittered x {jx} should be < 1080");
        assert!(jy < 2340, "Jittered y {jy} should be < 2340");

        let msg = engine.build_touch_event(
            AndroidMotionEventAction::Move,
            400,
            300,
            POINTER_ID_GENERIC_FINGER,
            0.75,
        );
        match msg {
            ControlMessage::InjectTouchEvent {
                action,
                pointer_id,
                position,
                pressure,
                action_button,
                buttons,
            } => {
                assert_eq!(action, AndroidMotionEventAction::Move);
                assert_eq!(pointer_id, POINTER_ID_GENERIC_FINGER);
                assert!((pressure - 0.75).abs() < f32::EPSILON);
                assert_eq!(position.x, 400);
                assert_eq!(position.y, 300);
                assert_eq!(action_button, 0);
                assert_eq!(buttons, 0);
            }
            _ => panic!("Expected InjectTouchEvent variant"),
        }

        drop(sender);
        let data = handle.join().unwrap();
        assert!(
            data.len() >= 192,
            "Expected >= 192 bytes (6 events), got {}",
            data.len()
        );
    }
}
