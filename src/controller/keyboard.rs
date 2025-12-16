use {
    crate::{
        config::mapping::{Mappings, Modifiers, Profile},
        controller::control_sender::ControlSender,
    },
    anyhow::{Result, anyhow},
    arc_swap::ArcSwap,
    egui::Key,
    parking_lot::RwLock,
    std::{collections::HashMap, sync::Arc},
    tracing::{error, info, trace},
};

lazy_static::lazy_static! {
    /// Mapping from egui Key to Android keycode
    pub static ref EGUI_TO_ANDROID_KEY: HashMap<Key, u8> = {
        let mut m = HashMap::new();

        // ────── 方向 & 导航键 ──────
        m.insert(Key::ArrowUp,      19);   // KEYCODE_DPAD_UP
        m.insert(Key::ArrowDown,    20);   // KEYCODE_DPAD_DOWN
        m.insert(Key::ArrowLeft,    21);   // KEYCODE_DPAD_LEFT
        m.insert(Key::ArrowRight,   22);   // KEYCODE_DPAD_RIGHT

        m.insert(Key::Escape,       4);    // 强烈推荐映射为 Back 键（手机上最常用）
        m.insert(Key::Tab,          61);   // KEYCODE_TAB
        m.insert(Key::Space,        62);   // KEYCODE_SPACE
        m.insert(Key::Enter,        66);   // KEYCODE_ENTER
        m.insert(Key::Backspace,    67);   // KEYCODE_DEL

        m.insert(Key::Insert,       110);  // KEYCODE_INSERT
        m.insert(Key::Delete,       112);  // KEYCODE_FORWARD_DEL
        m.insert(Key::Home,         122);  // KEYCODE_MOVE_HOME
        m.insert(Key::End,          123);  // KEYCODE_MOVE_END
        m.insert(Key::PageUp,       92);   // KEYCODE_PAGE_UP
        m.insert(Key::PageDown,     93);   // KEYCODE_PAGE_DOWN

        // ────── 字母 A-Z ──────
        m.insert(Key::A, 29); m.insert(Key::B, 30); m.insert(Key::C, 31);
        m.insert(Key::D, 32); m.insert(Key::E, 33); m.insert(Key::F, 34);
        m.insert(Key::G, 35); m.insert(Key::H, 36); m.insert(Key::I, 37);
        m.insert(Key::J, 38); m.insert(Key::K, 39); m.insert(Key::L, 40);
        m.insert(Key::M, 41); m.insert(Key::N, 42); m.insert(Key::O, 43);
        m.insert(Key::P, 44); m.insert(Key::Q, 45); m.insert(Key::R, 46);
        m.insert(Key::S, 47); m.insert(Key::T, 48); m.insert(Key::U, 49);
        m.insert(Key::V, 50); m.insert(Key::W, 51); m.insert(Key::X, 52);
        m.insert(Key::Y, 53); m.insert(Key::Z, 54);

        // ────── 主键盘数字 0-9 ──────
        m.insert(Key::Num0, 7);  m.insert(Key::Num1, 8);  m.insert(Key::Num2, 9);
        m.insert(Key::Num3,10);  m.insert(Key::Num4,11);  m.insert(Key::Num5,12);
        m.insert(Key::Num6,13);  m.insert(Key::Num7,14);  m.insert(Key::Num8,15);
        m.insert(Key::Num9,16);

        // ────── 标点符号（完整覆盖 egui 新增键） ──────
        m.insert(Key::Comma,            55);  // KEYCODE_COMMA
        m.insert(Key::Period,           56);  // KEYCODE_PERIOD
        m.insert(Key::Slash,            76);  // KEYCODE_SLASH
        m.insert(Key::Backslash,        73);  // KEYCODE_BACKSLASH
        m.insert(Key::Semicolon,        74);  // KEYCODE_SEMICOLON
        m.insert(Key::Quote,            75);  // KEYCODE_APOSTROPHE
        m.insert(Key::OpenBracket,      71);  // KEYCODE_LEFT_BRACKET  [
        m.insert(Key::CloseBracket,     72);  // KEYCODE_RIGHT_BRACKET ]
        m.insert(Key::Minus,            69);  // KEYCODE_MINUS
        m.insert(Key::Equals,           70);  // KEYCODE_EQUALS

        // ────── 功能键 F1~F12 ──────
        for i in 1..=12 {
            if let Some(key) = Key::from_name(&format!("F{i}")) {
                m.insert(key, 130 + i);
            }
        }

        m
    };

    /// Mapping from egui Key to Android shifted keycode
    pub static ref EGUI_TO_ANDROID_SHIFT_KEY: HashMap<Key, u8> = {
        let mut m = HashMap::new();
        m.insert(Key::Exclamationmark,   8);        // KEYCODE_1
        m.insert(Key::Pipe,              73);       // KEYCODE_BACKSLASH
        m.insert(Key::OpenCurlyBracket,  71);       // KEYCODE_LEFT_BRACKET
        m.insert(Key::CloseCurlyBracket, 72);       // KEYCODE_RIGHT_BRACKET
        m.insert(Key::Colon,             74);       // KEYCODE_SEMICOLON
        m.insert(Key::Questionmark,      76);       // KEYCODE_SLASH
        m
    };

    /// Keys that should not be handled, handled via text input instead
    pub static ref SHOULD_NOT_HANDLED_KEYS: Vec<Key> = vec![
        Key::Backtick
    ];

    // Text input special character mappings before sending to adb shell
    pub static ref TEXT_MAPPINGS: HashMap<String, String > = {
        let mut m = HashMap::new();
        m.insert("`".to_owned(), "\\`".to_owned());
        m
    };
}

/// Keyboard mapping state
pub struct KeyboardMapper {
    config: Arc<Mappings>,
    sender: ControlSender,
    avail_profiles: RwLock<Vec<Arc<Profile>>>,
    active_profile: ArcSwap<Option<Arc<Profile>>>,
    /// Pixel-converted mappings for active profile (百分比 → 像素)
    pixel_mappings: RwLock<HashMap<Key, crate::config::mapping::AdbAction>>,
}

impl KeyboardMapper {
    /// Create a new keyboard mapper
    pub fn new(config: Arc<Mappings>, sender: ControlSender) -> Result<Self> {
        Ok(Self {
            config,
            sender,
            avail_profiles: RwLock::new(Vec::new()),
            active_profile: ArcSwap::from_pointee(None),
            pixel_mappings: RwLock::new(HashMap::new()),
        })
    }

    /// Refresh available profiles based on device ID and rotation
    ///
    /// Profile 中保持百分比坐标不变，转换后的像素坐标存储在 pixel_mappings 中
    ///
    /// # Parameters
    /// - `device_id`: 设备 ID
    /// - `device_rotation`: 当前设备旋转角度 (0-3, CCW)
    /// - `capture_orientation_locked`: capture-orientation 是否被锁定 (NVDEC 模式)
    pub fn refresh_profiles(
        &self,
        device_id: &str,
        device_rotation: u32,
        capture_orientation_locked: bool,
    ) -> Result<()> {
        let avail_profiles = self.config.filter_profiles(device_id, device_rotation);

        if avail_profiles.is_empty() {
            info!(
                "No matching profiles found for device ID '{}' with rotation {}.",
                device_id, device_rotation
            );
            info!("Disable custom key mappings for this device/rotation.");

            self.active_profile.store(Arc::new(None));
            self.pixel_mappings.write().clear();
        } else {
            info!(
                "Found {} matching profiles for device ID '{}' with rotation {}.",
                avail_profiles.len(),
                device_id,
                device_rotation
            );

            // Set active profile (keeping percentage coordinates)
            let profile = avail_profiles[0].clone();
            self.active_profile.store(Arc::new(Some(profile.clone())));
            info!("Active profile set to: {}", profile.name);

            // Convert percentage to pixels for runtime use
            let (video_width, video_height) = self.sender.get_screen_size();
            self.update_pixel_mappings(
                &profile,
                video_width as u32,
                video_height as u32,
                capture_orientation_locked,
            );
        }

        *self.avail_profiles.write() = avail_profiles;

        Ok(())
    }

    /// Convert profile percentage coordinates to pixel mappings
    ///
    /// 坐标转换说明：
    /// 1. Profile 坐标：百分比 (0.0-1.0)，基于 profile.rotation 对应的设备方向
    /// 2. 视频坐标：像素，基于当前视频分辨率
    ///
    /// 当 capture-orientation 未锁定时：
    /// - 视频坐标系跟随设备旋转，profile.rotation == device_orientation
    /// - 直接乘以视频尺寸即可
    ///
    /// 当 capture-orientation=@0 锁定时（NVDEC 模式）：
    /// - 视频坐标系固定为 0°（设备自然方向）
    /// - Profile 坐标是基于 profile.rotation 的坐标系
    /// - 需要将坐标从 profile.rotation 转换到 0° 坐标系
    ///
    /// # Parameters
    /// - `profile`: 配置文件（rotation 字段记录坐标系方向）
    /// - `video_width/height`: 视频分辨率（锁定时为设备自然方向分辨率）
    /// - `capture_orientation_locked`: 是否锁定 capture-orientation
    fn update_pixel_mappings(
        &self,
        profile: &Profile,
        video_width: u32,
        video_height: u32,
        capture_orientation_locked: bool,
    ) {
        use crate::config::mapping::AdbAction;

        let mut pixel_map = HashMap::new();

        // Helper: 转换单个坐标点
        let transform_coord = |x_percent: f32, y_percent: f32| -> (f32, f32) {
            if !capture_orientation_locked {
                // 未锁定：视频坐标系跟随设备，直接缩放
                (
                    x_percent * video_width as f32,
                    y_percent * video_height as f32,
                )
            } else {
                // 锁定：需要旋转变换
                // Profile.rotation 是 CCW (Android Display Rotation)
                // rotation=0: 0°, rotation=1: 90° CCW, rotation=2: 180°, rotation=3: 270° CCW
                //
                // 视频坐标系固定为 0°（竖屏），需要将 profile 坐标转换到 0°
                match profile.rotation {
                    0 => {
                        // Profile 坐标已经是 0°，直接缩放
                        (
                            x_percent * video_width as f32,
                            y_percent * video_height as f32,
                        )
                    }
                    1 => {
                        // Profile 坐标是 rotation=1（设备横屏，90° CCW）
                        // 视频坐标是 rotation=0（竖屏，capture locked）
                        //
                        // Profile 坐标系：横屏 W_profile x H_profile（如 2800x1260）
                        // 视频坐标系：竖屏 W_video x H_video（如 576x1280，注意 video 是竖屏所以 W
                        // < H）
                        //
                        // 变换逻辑：
                        // rotation=1 设备横屏时，屏幕内容也是横屏的
                        // capture=@0 时，视频捕获的是竖屏画面
                        // scrcpy-server 会自动旋转画面，使得设备横屏内容在竖屏视频中正确显示
                        //
                        // 但坐标需要转换：
                        // - Profile 的 (x%, y%) 是基于横屏坐标系
                        // - 需要转换到竖屏坐标系
                        //
                        // 设备从竖屏（0°）逆时针转90°到横屏（rotation=1）
                        // 坐标系变换：竖屏的 X 轴 → 横屏的 Y 轴（反向）
                        //            竖屏的 Y 轴 → 横屏的 X 轴
                        // 反向变换：横屏的 (x, y) → 竖屏的 (1-y, x)
                        // 不对，让我从物理位置推导...
                        //
                        // 横屏左上角 (0, 0) → 竖屏的哪里？
                        // 设备逆时针转90°，左上角移到了物理上的右上角
                        // 但竖屏坐标系，右上角是 (W-1, 0)
                        //
                        // 让我用百分比：
                        // 横屏 (0%, 0%) → 竖屏 (100%, 0%)？不对...
                        //
                        // 简单推导：
                        // rotation=1: X轴2800向右，Y轴1260向下
                        // rotation=0: X轴1080向右，Y轴2400向下
                        // rotation=1 的 X 方向对应 rotation=0 的 Y 方向
                        // rotation=1 的 Y 方向对应 rotation=0 的 -X 方向（反向）
                        //
                        // 所以：(x_r1, y_r1) → (1 - y_r1, x_r1)
                        (
                            (1.0 - y_percent) * video_width as f32,
                            x_percent * video_height as f32,
                        )
                    }
                    2 => {
                        // Profile 坐标是 180°
                        // 转换到 0°：(x', y') -> (1-x', 1-y')
                        (
                            (1.0 - x_percent) * video_width as f32,
                            (1.0 - y_percent) * video_height as f32,
                        )
                    }
                    3 => {
                        // Profile 坐标是 270° CCW（横屏，设备向右转）
                        // rotation=3: 横屏 2800x1260, rotation=0: 竖屏 1080x2400
                        // rotation=3 的 X 对应 rotation=0 的 Y
                        // rotation=3 的 Y 对应 rotation=0 的 X（反向）
                        // 转换：(x_r3, y_r3) -> (1-y_r3, x_r3)
                        (
                            (1.0 - y_percent) * video_width as f32,
                            x_percent * video_height as f32,
                        )
                    }
                    _ => {
                        trace!(
                            "Invalid rotation {}, fallback to no transform",
                            profile.rotation
                        );
                        (
                            x_percent * video_width as f32,
                            y_percent * video_height as f32,
                        )
                    }
                }
            }
        };

        for (key, action) in profile.mappings.read().iter() {
            let pixel_action = match action {
                AdbAction::Tap { x, y } => {
                    let (px, py) = transform_coord(*x, *y);
                    AdbAction::Tap { x: px, y: py }
                }
                AdbAction::TouchDown { x, y } => {
                    let (px, py) = transform_coord(*x, *y);
                    AdbAction::TouchDown { x: px, y: py }
                }
                AdbAction::TouchMove { x, y } => {
                    let (px, py) = transform_coord(*x, *y);
                    AdbAction::TouchMove { x: px, y: py }
                }
                AdbAction::TouchUp { x, y } => {
                    let (px, py) = transform_coord(*x, *y);
                    AdbAction::TouchUp { x: px, y: py }
                }
                AdbAction::Scroll { x, y, direction } => {
                    let (px, py) = transform_coord(*x, *y);
                    AdbAction::Scroll {
                        x: px,
                        y: py,
                        direction: direction.clone(),
                    }
                }
                AdbAction::Swipe {
                    x1,
                    y1,
                    x2,
                    y2,
                    duration,
                } => {
                    let (px1, py1) = transform_coord(*x1, *y1);
                    let (px2, py2) = transform_coord(*x2, *y2);
                    AdbAction::Swipe {
                        x1: px1,
                        y1: py1,
                        x2: px2,
                        y2: py2,
                        duration: *duration,
                    }
                }
                other => other.clone(),
            };
            pixel_map.insert(*key, pixel_action);
        }

        if capture_orientation_locked {
            trace!(
                "Converted {} mappings from percentage (rotation={}) to {}x{} pixels (capture locked to 0°)",
                pixel_map.len(),
                profile.rotation,
                video_width,
                video_height
            );
        } else {
            trace!(
                "Converted {} mappings from percentage to {}x{} pixels (no transform)",
                pixel_map.len(),
                video_width,
                video_height
            );
        }

        *self.pixel_mappings.write() = pixel_map;
    }

    /// Load profile by name
    #[allow(dead_code)]
    pub fn load_profile(&mut self, name: &str) -> Result<()> {
        let profile = self
            .avail_profiles
            .read()
            .iter()
            .find(|p| p.name == name)
            .cloned()
            .ok_or_else(|| {
                error!("Profile '{}' not found.", name);
                anyhow!("Profile not found: {}.", name)
            })?;

        self.active_profile.store(Arc::new(Some(profile.clone())));
        info!("Active profile set to: {}", profile.name);

        Ok(())
    }

    /// Get active profile name
    pub fn get_active_profile_name(&self) -> Option<String> {
        self.active_profile
            .load()
            .as_ref()
            .as_ref()
            .map(|p| p.name.clone())
    }

    /// Handle keyboard event
    pub fn handle_standard_key_event(&self, key: &Key) -> Result<bool> {
        if SHOULD_NOT_HANDLED_KEYS.contains(key) {
            return Ok(false);
        }

        if let Some(&keycode) = EGUI_TO_ANDROID_KEY.get(key) {
            trace!(
                "Handling standard key event: {:?} -> keycode {}",
                key, keycode
            );
            self.sender.send_key_press(keycode as u32, 0)?;
            return Ok(true);
        }

        Ok(false)
    }

    /// Handle shifted key event, returns true if handled
    pub fn handle_shifted_key_event(&self, key: &Key) -> Result<bool> {
        if let Some(&keycode) = EGUI_TO_ANDROID_SHIFT_KEY.get(key) {
            trace!(
                "Handling shifted key event: {:?} -> keycode {}",
                key, keycode
            );
            // SHIFT metastate = 1 (AMETA_SHIFT_ON)
            self.sender.send_key_press(keycode as u32, 1)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Handle text input event
    pub fn handle_text_input_event(&self, text: &str) -> Result<bool> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(false);
        }

        let text = TEXT_MAPPINGS
            .iter()
            .fold(text.to_owned(), |acc, (k, v)| acc.replace(k, v));

        trace!("Handling text input event: {}", text);
        self.sender.send_text(&text)?;
        Ok(true)
    }

    /// Handle key combo event, returns true if handled
    pub fn handle_keycombo_event(&self, modifiers: Modifiers, key: &Key) -> Result<bool> {
        if let Some(&keycode) = EGUI_TO_ANDROID_KEY.get(key) {
            trace!("Handling key combo event: {:?} + {:?}", modifiers, key);

            // Convert modifiers to Android metastate
            // AMETA_SHIFT_ON = 1, AMETA_ALT_ON = 2, AMETA_CTRL_ON = 4096, AMETA_META_ON = 65536
            let mut metastate = 0u32;
            if modifiers.shift {
                metastate |= 1;
            }
            if modifiers.alt {
                metastate |= 2;
            }
            if modifiers.ctrl || modifiers.command {
                metastate |= 4096;
            }

            self.sender.send_key_press(keycode as u32, metastate)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Handle custom keyboard event using legacy ADB actions
    ///
    /// Uses pixel_mappings (converted from percentage) for actual control
    pub fn handle_custom_keymapping_event(&self, key: &Key) -> Result<bool> {
        // Use pixel-converted mappings for control
        if let Some(action) = self.pixel_mappings.read().get(key) {
            trace!(
                "Handling custom key mapping event: {:?} -> {:?}",
                key, action
            );
            self.send_adb_action(action)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Convert AdbAction to control messages (temporary bridge)
    fn send_adb_action(&self, action: &crate::config::mapping::AdbAction) -> Result<()> {
        use crate::config::mapping::AdbAction;

        match action {
            AdbAction::Key { keycode } => {
                self.sender.send_key_press(*keycode as u32, 0)?;
            }
            AdbAction::KeyCombo { modifiers, keycode } => {
                let mut metastate = 0u32;
                if modifiers.shift {
                    metastate |= 1;
                }
                if modifiers.alt {
                    metastate |= 2;
                }
                if modifiers.ctrl || modifiers.command {
                    metastate |= 4096;
                }
                self.sender.send_key_press(*keycode as u32, metastate)?;
            }
            AdbAction::Text { text } => {
                self.sender.send_text(text)?;
            }
            AdbAction::Back => {
                self.sender.send_key_press(4, 0)?; // KEYCODE_BACK
            }
            AdbAction::Home => {
                self.sender.send_key_press(3, 0)?; // KEYCODE_HOME
            }
            AdbAction::Menu => {
                self.sender.send_key_press(82, 0)?; // KEYCODE_MENU
            }
            AdbAction::Power => {
                self.sender.send_key_press(26, 0)?; // KEYCODE_POWER
            }
            AdbAction::Tap { x, y } => {
                self.sender.send_touch_down(*x as u32, *y as u32)?;
                self.sender.send_touch_up(*x as u32, *y as u32)?;
            }
            AdbAction::TouchDown { x, y } => {
                self.sender.send_touch_down(*x as u32, *y as u32)?;
            }
            AdbAction::TouchMove { x, y } => {
                self.sender.send_touch_move(*x as u32, *y as u32)?;
            }
            AdbAction::TouchUp { x, y } => {
                self.sender.send_touch_up(*x as u32, *y as u32)?;
            }
            AdbAction::Scroll { x, y, direction } => {
                use crate::config::mapping::WheelDirection;
                let (h, v) = match direction {
                    WheelDirection::Up => (0.0, -5.0),
                    WheelDirection::Down => (0.0, 5.0),
                };
                self.sender.send_scroll(*x as u32, *y as u32, h, v)?;
            }
            AdbAction::Swipe { x1, y1, x2, y2, .. } => {
                // Simulate swipe with touch down + move + up
                self.sender.send_touch_down(*x1 as u32, *y1 as u32)?;
                self.sender.send_touch_move(*x2 as u32, *y2 as u32)?;
                self.sender.send_touch_up(*x2 as u32, *y2 as u32)?;
            }
            AdbAction::Ignore => {}
        }
        Ok(())
    }

    /// Get list of available profiles
    pub fn get_avail_profiles(&self) -> Vec<String> {
        let avail_profiles = self.avail_profiles.read();
        avail_profiles.iter().map(|p| p.name.clone()).collect()
    }

    /// Get active profile (for read-only access)
    pub fn get_active_profile(&self) -> Option<Arc<Profile>> {
        self.active_profile.load().as_ref().clone()
    }
}
