//! Android keycode and metastate constants
//!
//! Definitions from Android KeyEvent class:
//! https://developer.android.com/reference/android/view/KeyEvent

/// Android keycode constants (subset of KEYCODE_*)
#[allow(dead_code)]
pub mod keycode {
    pub const HOME: u8 = 3;
    pub const BACK: u8 = 4;
    pub const NUM_0: u8 = 7;
    pub const NUM_1: u8 = 8;
    pub const NUM_2: u8 = 9;
    pub const NUM_3: u8 = 10;
    pub const NUM_4: u8 = 11;
    pub const NUM_5: u8 = 12;
    pub const NUM_6: u8 = 13;
    pub const NUM_7: u8 = 14;
    pub const NUM_8: u8 = 15;
    pub const NUM_9: u8 = 16;
    pub const DPAD_UP: u8 = 19;
    pub const DPAD_DOWN: u8 = 20;
    pub const DPAD_LEFT: u8 = 21;
    pub const DPAD_RIGHT: u8 = 22;
    pub const POWER: u8 = 26;
    pub const A: u8 = 29;
    pub const B: u8 = 30;
    pub const C: u8 = 31;
    pub const D: u8 = 32;
    pub const E: u8 = 33;
    pub const F: u8 = 34;
    pub const G: u8 = 35;
    pub const H: u8 = 36;
    pub const I: u8 = 37;
    pub const J: u8 = 38;
    pub const K: u8 = 39;
    pub const L: u8 = 40;
    pub const M: u8 = 41;
    pub const N: u8 = 42;
    pub const O: u8 = 43;
    pub const P: u8 = 44;
    pub const Q: u8 = 45;
    pub const R: u8 = 46;
    pub const S: u8 = 47;
    pub const T: u8 = 48;
    pub const U: u8 = 49;
    pub const V: u8 = 50;
    pub const W: u8 = 51;
    pub const X: u8 = 52;
    pub const Y: u8 = 53;
    pub const Z: u8 = 54;
    pub const COMMA: u8 = 55;
    pub const PERIOD: u8 = 56;
    pub const TAB: u8 = 61;
    pub const SPACE: u8 = 62;
    pub const ENTER: u8 = 66;
    pub const DEL: u8 = 67;
    pub const MINUS: u8 = 69;
    pub const EQUALS: u8 = 70;
    pub const LEFT_BRACKET: u8 = 71;
    pub const RIGHT_BRACKET: u8 = 72;
    pub const BACKSLASH: u8 = 73;
    pub const SEMICOLON: u8 = 74;
    pub const APOSTROPHE: u8 = 75;
    pub const SLASH: u8 = 76;
    pub const MENU: u8 = 82;
    pub const PAGE_UP: u8 = 92;
    pub const PAGE_DOWN: u8 = 93;
    pub const INSERT: u8 = 110;
    pub const FORWARD_DEL: u8 = 112;
    pub const MOVE_HOME: u8 = 122;
    pub const MOVE_END: u8 = 123;
    pub const F1: u8 = 131;
    pub const F2: u8 = 132;
    pub const F3: u8 = 133;
    pub const F4: u8 = 134;
    pub const F5: u8 = 135;
    pub const F6: u8 = 136;
    pub const F7: u8 = 137;
    pub const F8: u8 = 138;
    pub const F9: u8 = 139;
    pub const F10: u8 = 140;
    pub const F11: u8 = 141;
    pub const F12: u8 = 142;
}

/// Android metastate constants (subset of AMETA_*)
#[allow(dead_code)]
pub mod metastate {
    pub const SHIFT_ON: u32 = 1;
    pub const ALT_ON: u32 = 2;
    pub const CTRL_ON: u32 = 4096;
    pub const META_ON: u32 = 65536;
}
