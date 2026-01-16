use {
    super::{mapping::MappingCoordSys, scrcpy::ScrcpyCoordSys, types::*, visual::VisualCoordSys},
    eframe::egui::{Pos2, Rect},
};

#[test]
fn test_mapping_0_to_scrcpy_no_capture_lock() {
    // Portrait device, no capture lock
    let mapping_sys = MappingCoordSys::new(0);
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, None);

    let scrcpy_pos = mapping_sys.to_scrcpy(&MappingPos::new(0.2, 0.4), &scrcpy_sys);

    // Portrait device: direct scale
    assert_eq!(scrcpy_pos.x, 216);
    assert_eq!(scrcpy_pos.y, 960);
}

#[test]
fn test_mapping_1_to_scrcpy_no_capture_lock() {
    // Landscape device, no capture lock
    let mapping_sys = MappingCoordSys::new(1);
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, None);

    let scrcpy_pos = mapping_sys.to_scrcpy(&MappingPos::new(0.2, 0.4), &scrcpy_sys);

    // Landscape device: direct scale
    assert_eq!(scrcpy_pos.x, 480);
    assert_eq!(scrcpy_pos.y, 432);
}

#[test]
fn test_mapping_2_to_scrcpy_no_capture_lock() {
    // Portrait upside-down device, no capture lock
    let mapping_sys = MappingCoordSys::new(2);
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, None);

    let scrcpy_pos = mapping_sys.to_scrcpy(&MappingPos::new(0.2, 0.4), &scrcpy_sys);

    // Portrait upside-down device: direct scale
    assert_eq!(scrcpy_pos.x, 216);
    assert_eq!(scrcpy_pos.y, 960);
}

#[test]
fn test_mapping_3_to_scrcpy_no_capture_lock() {
    // Landscape upside-down device, no capture lock
    let mapping_sys = MappingCoordSys::new(3);
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, None);

    let scrcpy_pos = mapping_sys.to_scrcpy(&MappingPos::new(0.2, 0.4), &scrcpy_sys);

    // Landscape upside-down device: direct scale
    assert_eq!(scrcpy_pos.x, 480);
    assert_eq!(scrcpy_pos.y, 432);
}

#[test]
fn test_mapping_0_to_scrcpy_capture_lock_0() {
    // Portrait device (orient=0), capture locked to portrait (orient=0)
    let mapping_sys = MappingCoordSys::new(0);
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(0));

    let scrcpy_pos = mapping_sys.to_scrcpy(&MappingPos::new(0.2, 0.4), &scrcpy_sys);

    // No rotation: (0.2, 0.4) -> (0.2, 0.4)
    assert_eq!(scrcpy_pos.x, 216); // 0.2 * 1080
    assert_eq!(scrcpy_pos.y, 960); // 0.4 * 2400
}

#[test]
fn test_mapping_1_to_scrcpy_capture_lock_0() {
    // Landscape device (orient=1), capture locked to portrait (orient=0)
    let mapping_sys = MappingCoordSys::new(1);
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(0));

    let scrcpy_pos = mapping_sys.to_scrcpy(&MappingPos::new(0.2, 0.4), &scrcpy_sys);

    // 90° CW rotation: (0.2, 0.4) -> (1-0.4, 0.2) = (0.6, 0.2)
    assert_eq!(scrcpy_pos.x, 648); // 0.6 * 1080
    assert_eq!(scrcpy_pos.y, 480); // 0.2 * 2400
}

#[test]
fn test_mapping_2_to_scrcpy_capture_lock_0() {
    // Portrait upside-down device (orient=2), capture locked to portrait (orient=0)
    let mapping_sys = MappingCoordSys::new(2);
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(0));

    let scrcpy_pos = mapping_sys.to_scrcpy(&MappingPos::new(0.2, 0.4), &scrcpy_sys);

    // 180° rotation: (0.2, 0.4) -> (1-0.2, 1-0.4) = (0.8, 0.6)
    assert_eq!(scrcpy_pos.x, 864); // 0.8 * 1080
    assert_eq!(scrcpy_pos.y, 1440); // 0.6 * 2400
}

#[test]
fn test_mapping_3_to_scrcpy_capture_lock_0() {
    // Landscape upside-down device (orient=3), capture locked to portrait (orient=0)
    let mapping_sys = MappingCoordSys::new(3);
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(0));

    let scrcpy_pos = mapping_sys.to_scrcpy(&MappingPos::new(0.2, 0.4), &scrcpy_sys);

    // 270° CW rotation: (0.2, 0.4) -> (0.4, 1-0.2) = (0.4, 0.8)
    assert_eq!(scrcpy_pos.x, 432); // 0.4 * 1080
    assert_eq!(scrcpy_pos.y, 1920); // 0.8 * 2400
}

#[test]
fn test_mapping_0_to_scrcpy_capture_lock_1() {
    // Portrait device (orient=0), capture locked to landscape (orient=1)
    let mapping_sys = MappingCoordSys::new(0);
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(1));

    let scrcpy_pos = mapping_sys.to_scrcpy(&MappingPos::new(0.2, 0.4), &scrcpy_sys);

    // 90° CW rotation: (0.2, 0.4) -> (1-0.4, 0.2) = (0.6, 0.2)
    assert_eq!(scrcpy_pos.x, 1440); // 0.6 * 2400
    assert_eq!(scrcpy_pos.y, 216); // 0.2 * 1080
}

#[test]
fn test_mapping_1_to_scrcpy_capture_lock_1() {
    // Landscape device (orient=1), capture locked to landscape (orient=1)
    let mapping_sys = MappingCoordSys::new(1);
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(1));

    let scrcpy_pos = mapping_sys.to_scrcpy(&MappingPos::new(0.2, 0.4), &scrcpy_sys);

    // 180° rotation: (0.2, 0.4) -> (1-0.2, 1-0.4) = (0.8, 0.6)
    assert_eq!(scrcpy_pos.x, 1920); // 0.8 * 2400
    assert_eq!(scrcpy_pos.y, 648); // 0.6 * 1080
}

#[test]
fn test_mapping_2_to_scrcpy_capture_lock_1() {
    // Portrait upside-down device (orient=2), capture locked to landscape (orient=1)
    let mapping_sys = MappingCoordSys::new(2);
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(1));

    let scrcpy_pos = mapping_sys.to_scrcpy(&MappingPos::new(0.2, 0.4), &scrcpy_sys);

    // 270° CW rotation: (0.2, 0.4) -> (0.4, 1-0.2) = (0.4, 0.8)
    assert_eq!(scrcpy_pos.x, 960); // 0.4 * 2400
    assert_eq!(scrcpy_pos.y, 864); // 0.8 * 1080
}

#[test]
fn test_mapping_3_to_scrcpy_capture_lock_1() {
    // Landscape upside-down device (orient=3), capture locked to landscape (orient=1)
    let mapping_sys = MappingCoordSys::new(3);
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(1));

    let scrcpy_pos = mapping_sys.to_scrcpy(&MappingPos::new(0.2, 0.4), &scrcpy_sys);

    // 0° rotation: (0.2, 0.4) -> (0.2, 0.4)
    assert_eq!(scrcpy_pos.x, 480); // 0.2 * 2400
    assert_eq!(scrcpy_pos.y, 432); // 0.4 * 1080
}

#[test]
fn test_mapping_0_to_scrcpy_capture_lock_2() {
    // Portrait device (orient=0), capture locked to 180° (orient=2)
    let mapping_sys = MappingCoordSys::new(0);
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(2));

    let scrcpy_pos = mapping_sys.to_scrcpy(&MappingPos::new(0.2, 0.4), &scrcpy_sys);

    // 180° rotation: (0.2, 0.4) -> (1-0.2, 1-0.4) = (0.8, 0.6)
    assert_eq!(scrcpy_pos.x, 864); // 0.7 * 1080
    assert_eq!(scrcpy_pos.y, 1440); // 0.3 * 2400
}

#[test]
fn test_mapping_1_to_scrcpy_capture_lock_2() {
    // Landscape device (orient=1), capture locked to 180° (orient=2)
    let mapping_sys = MappingCoordSys::new(1);
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(2));

    let scrcpy_pos = mapping_sys.to_scrcpy(&MappingPos::new(0.2, 0.4), &scrcpy_sys);

    // 270° CW rotation: (0.2, 0.4) -> (0.4, 1-0.2) = (0.4, 0.8)
    assert_eq!(scrcpy_pos.x, 432); // 0.4 * 1080
    assert_eq!(scrcpy_pos.y, 1920); // 0.8 * 2400
}

#[test]
fn test_mapping_2_to_scrcpy_capture_lock_2() {
    // Portrait upside-down device (orient=2), capture locked to 180° (orient=2)
    let mapping_sys = MappingCoordSys::new(2);
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(2));

    let scrcpy_pos = mapping_sys.to_scrcpy(&MappingPos::new(0.2, 0.4), &scrcpy_sys);

    // 0° rotation: (0.2, 0.4) -> (0.2, 0.4)
    assert_eq!(scrcpy_pos.x, 216); // 0.2 * 1080
    assert_eq!(scrcpy_pos.y, 960); // 0.4 * 2400
}

#[test]
fn test_mapping_3_to_scrcpy_capture_lock_2() {
    // Landscape upside-down device (orient=3), capture locked to 180° (orient=2)
    let mapping_sys = MappingCoordSys::new(3);
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(2));

    let scrcpy_pos = mapping_sys.to_scrcpy(&MappingPos::new(0.2, 0.4), &scrcpy_sys);

    // 90° CCW rotation: (0.2, 0.4) -> (1-0.4, 0.2) = (0.6, 0.2)
    assert_eq!(scrcpy_pos.x, 648); // 0.6 * 1080
    assert_eq!(scrcpy_pos.y, 480); // 0.2 * 2400
}

#[test]
fn test_mapping_0_to_scrcpy_capture_lock_3() {
    // Portrait device (orient=0), capture locked to landscape 270° (orient=3)
    let mapping_sys = MappingCoordSys::new(0);
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(3));

    let scrcpy_pos = mapping_sys.to_scrcpy(&MappingPos::new(0.2, 0.4), &scrcpy_sys);

    // 270° CW rotation: (0.2, 0.4) -> (0.4, 1-0.2) = (0.4, 0.8)
    assert_eq!(scrcpy_pos.x, 960); // 0.4 * 2400
    assert_eq!(scrcpy_pos.y, 864); // 0.8 * 1080
}

#[test]
fn test_mapping_1_to_scrcpy_capture_lock_3() {
    // Landscape device (orient=1), capture locked to landscape 270° (orient=3)
    let mapping_sys = MappingCoordSys::new(1);
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(3));

    let scrcpy_pos = mapping_sys.to_scrcpy(&MappingPos::new(0.2, 0.4), &scrcpy_sys);

    // 0° rotation: (0.2, 0.4) -> (0.2, 0.4)
    assert_eq!(scrcpy_pos.x, 480); // 0.2 * 2400
    assert_eq!(scrcpy_pos.y, 432); // 0.4 * 1080
}

#[test]
fn test_mapping_2_to_scrcpy_capture_lock_3() {
    // Portrait upside-down device (orient=2), capture locked to landscape 270° (orient=3)
    let mapping_sys = MappingCoordSys::new(2);
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(3));

    let scrcpy_pos = mapping_sys.to_scrcpy(&MappingPos::new(0.2, 0.4), &scrcpy_sys);

    // 90° CCW rotation: (0.2, 0.4) -> (1-0.4, 0.2) = (0.6, 0.2)
    assert_eq!(scrcpy_pos.x, 1440); // 0.6 * 2400
    assert_eq!(scrcpy_pos.y, 216); // 0.2 * 1080
}

#[test]
fn test_mapping_3_to_scrcpy_capture_lock_3() {
    // Landscape upside-down device (orient=3), capture locked to landscape 270° (orient=3)
    let mapping_sys = MappingCoordSys::new(3);
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(3));

    let scrcpy_pos = mapping_sys.to_scrcpy(&MappingPos::new(0.2, 0.4), &scrcpy_sys);

    // 180° rotation: (0.2, 0.4) -> (1-0.2, 1-0.4) = (0.8, 0.6)
    assert_eq!(scrcpy_pos.x, 1920); // 0.8 * 2400
    assert_eq!(scrcpy_pos.y, 648); // 0.6 * 1080
}

#[test]
fn test_scrcpy_no_capture_lock_to_mapping_0() {
    // Portrait device, no capture lock
    let mapping_sys = MappingCoordSys::new(0);
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, None);

    // (0.2, 0.4) in scrcpy coords
    let mapping_pos = mapping_sys.from_scrcpy(&ScrcpyPos::new(216, 960), &scrcpy_sys);

    // Portrait device: direct scale
    assert!((mapping_pos.x - 0.2).abs() < 0.001);
    assert!((mapping_pos.y - 0.4).abs() < 0.001);
}

#[test]
fn test_scrcpy_no_capture_lock_to_mapping_1() {
    // Landscape device, no capture lock
    let mapping_sys = MappingCoordSys::new(1);
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, None);

    // (0.2, 0.4) in scrcpy coords
    let mapping_pos = mapping_sys.from_scrcpy(&ScrcpyPos::new(480, 432), &scrcpy_sys);

    // Landscape device: direct scale
    assert!((mapping_pos.x - 0.2).abs() < 0.001);
    assert!((mapping_pos.y - 0.4).abs() < 0.001);
}

#[test]
fn test_scrcpy_no_capture_lock_to_mapping_2() {
    // Portrait upside-down device, no capture lock
    let mapping_sys = MappingCoordSys::new(2);
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, None);

    // (0.2, 0.4) in scrcpy coords
    let mapping_pos = mapping_sys.from_scrcpy(&ScrcpyPos::new(216, 960), &scrcpy_sys);

    // Portrait upside-down device: direct scale
    assert!((mapping_pos.x - 0.2).abs() < 0.001);
    assert!((mapping_pos.y - 0.4).abs() < 0.001);
}

#[test]
fn test_scrcpy_no_capture_lock_to_mapping_3() {
    // Landscape upside-down device, no capture lock
    let mapping_sys = MappingCoordSys::new(3);
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, None);

    // (0.2, 0.4) in scrcpy coords
    let mapping_pos = mapping_sys.from_scrcpy(&ScrcpyPos::new(480, 432), &scrcpy_sys);

    // Landscape upside-down device: direct scale
    assert!((mapping_pos.x - 0.2).abs() < 0.001);
    assert!((mapping_pos.y - 0.4).abs() < 0.001);
}

#[test]
fn test_scrcpy_capture_lock_0_to_mapping_0() {
    // Portrait device (orient=0), capture locked to portrait (orient=0)
    let mapping_sys = MappingCoordSys::new(0);
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(0));

    // (0.2, 0.4) in scrcpy coords
    let mapping_pos = mapping_sys.from_scrcpy(&ScrcpyPos::new(216, 960), &scrcpy_sys);

    // No rotation
    assert!((mapping_pos.x - 0.2).abs() < 0.001);
    assert!((mapping_pos.y - 0.4).abs() < 0.001);
}

#[test]
fn test_scrcpy_capture_lock_0_to_mapping_1() {
    // Landscape device (orient=1), capture locked to portrait (orient=0)
    let mapping_sys = MappingCoordSys::new(1);
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(0));

    // (0.2, 0.4) in scrcpy coords
    let mapping_pos = mapping_sys.from_scrcpy(&ScrcpyPos::new(216, 960), &scrcpy_sys);

    // 90° CCW rotation: (0.2, 0.4) -> (0.4, 1-0.2) = (0.4, 0.8)
    assert!((mapping_pos.x - 0.4).abs() < 0.001);
    assert!((mapping_pos.y - 0.8).abs() < 0.001);
}

#[test]
fn test_scrcpy_capture_lock_0_to_mapping_2() {
    // Portrait upside-down device (orient=2), capture locked to portrait (orient=0)
    let mapping_sys = MappingCoordSys::new(2);
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(0));

    // (0.2, 0.4) in scrcpy coords
    let mapping_pos = mapping_sys.from_scrcpy(&ScrcpyPos::new(216, 960), &scrcpy_sys);

    // 180° rotation: (0.2, 0.4) -> (1-0.2, 1-0.4) = (0.8, 0.6)
    assert!((mapping_pos.x - 0.8).abs() < 0.001);
    assert!((mapping_pos.y - 0.6).abs() < 0.001);
}

#[test]
fn test_scrcpy_capture_lock_0_to_mapping_3() {
    // Landscape upside-down device (orient=3), capture locked to portrait (orient=0)
    let mapping_sys = MappingCoordSys::new(3);
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(0));

    // (0.2, 0.4) in scrcpy coords
    let mapping_pos = mapping_sys.from_scrcpy(&ScrcpyPos::new(216, 960), &scrcpy_sys);

    // 270° CCW rotation: (0.2, 0.4) -> (1-0.4, 0.2) = (0.6, 0.2)
    assert!((mapping_pos.x - 0.6).abs() < 0.001);
    assert!((mapping_pos.y - 0.2).abs() < 0.001);
}

#[test]
fn test_scrcpy_capture_lock_1_to_mapping_0() {
    // Portrait device (orient=0), capture locked to landscape (orient=1)
    let mapping_sys = MappingCoordSys::new(0);
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(1));

    // (0.2, 0.4) in scrcpy coords
    let mapping_pos = mapping_sys.from_scrcpy(&ScrcpyPos::new(480, 432), &scrcpy_sys);

    // 90° CCW rotation: (0.2, 0.4) -> (0.4, 1-0.2) = (0.4, 0.8)
    assert!((mapping_pos.x - 0.4).abs() < 0.001);
    assert!((mapping_pos.y - 0.8).abs() < 0.001);
}

#[test]
fn test_scrcpy_capture_lock_1_to_mapping_1() {
    // Landscape device (orient=1), capture locked to landscape (orient=1)
    let mapping_sys = MappingCoordSys::new(1);
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(1));

    // (0.2, 0.4) in scrcpy coords
    let mapping_pos = mapping_sys.from_scrcpy(&ScrcpyPos::new(480, 432), &scrcpy_sys);

    // 180° rotation: (0.2, 0.4) -> (1-0.2, 1-0.4) = (0.8, 0.6)
    assert!((mapping_pos.x - 0.8).abs() < 0.001);
    assert!((mapping_pos.y - 0.6).abs() < 0.001);
}

#[test]
fn test_scrcpy_capture_lock_1_to_mapping_2() {
    // Portrait upside-down device (orient=2), capture locked to landscape (orient=1)
    let mapping_sys = MappingCoordSys::new(2);
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(1));

    // (0.2, 0.4) in scrcpy coords
    let mapping_pos = mapping_sys.from_scrcpy(&ScrcpyPos::new(480, 432), &scrcpy_sys);

    // 270° CCW rotation: (0.2, 0.4) -> (1-0.4, 0.2) = (0.6, 0.2)
    assert!((mapping_pos.x - 0.6).abs() < 0.001);
    assert!((mapping_pos.y - 0.2).abs() < 0.001);
}

#[test]
fn test_scrcpy_capture_lock_1_to_mapping_3() {
    // Landscape upside-down device (orient=3), capture locked to landscape (orient=1)
    let mapping_sys = MappingCoordSys::new(3);
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(1));

    // (0.2, 0.4) in scrcpy coords
    let mapping_pos = mapping_sys.from_scrcpy(&ScrcpyPos::new(480, 432), &scrcpy_sys);

    // 0° rotation: (0.2, 0.4) -> (0.2, 0.4)
    assert!((mapping_pos.x - 0.2).abs() < 0.001);
    assert!((mapping_pos.y - 0.4).abs() < 0.001);
}

#[test]
fn test_scrcpy_capture_lock_2_to_mapping_0() {
    // Portrait device (orient=0), capture locked to 180° (orient=2)
    let mapping_sys = MappingCoordSys::new(0);
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(2));

    // (0.2, 0.4) in scrcpy coords
    let mapping_pos = mapping_sys.from_scrcpy(&ScrcpyPos::new(216, 960), &scrcpy_sys);

    // 180° rotation: (0.2, 0.4) -> (1-0.2, 1-0.4) = (0.8, 0.6)
    assert!((mapping_pos.x - 0.8).abs() < 0.001);
    assert!((mapping_pos.y - 0.6).abs() < 0.001);
}

#[test]
fn test_scrcpy_capture_lock_2_to_mapping_1() {
    // Landscape device (orient=1), capture locked to 180° (orient=2)
    let mapping_sys = MappingCoordSys::new(1);
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(2));

    // (0.2, 0.4) in scrcpy coords
    let mapping_pos = mapping_sys.from_scrcpy(&ScrcpyPos::new(216, 960), &scrcpy_sys);

    // 270° CCW rotation: (0.2, 0.4) -> (1-0.4, 0.2) = (0.6, 0.2)
    assert!((mapping_pos.x - 0.6).abs() < 0.001);
    assert!((mapping_pos.y - 0.2).abs() < 0.001);
}

#[test]
fn test_scrcpy_capture_lock_2_to_mapping_2() {
    // Portrait upside-down device (orient=2), capture locked to 180° (orient=2)
    let mapping_sys = MappingCoordSys::new(2);
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(2));

    // (0.2, 0.4) in scrcpy coords
    let mapping_pos = mapping_sys.from_scrcpy(&ScrcpyPos::new(216, 960), &scrcpy_sys);

    // 0° rotation: (0.2, 0.4) -> (0.2, 0.4)
    assert!((mapping_pos.x - 0.2).abs() < 0.001);
    assert!((mapping_pos.y - 0.4).abs() < 0.001);
}

#[test]
fn test_scrcpy_capture_lock_2_to_mapping_3() {
    // Landscape upside-down device (orient=3), capture locked to 180° (orient=2)
    let mapping_sys = MappingCoordSys::new(3);
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(2));

    // (0.2, 0.4) in scrcpy coords
    let mapping_pos = mapping_sys.from_scrcpy(&ScrcpyPos::new(216, 960), &scrcpy_sys);

    // 90° CCW rotation: (0.2, 0.4) -> (0.4, 1-0.2) = (0.4, 0.8)
    assert!((mapping_pos.x - 0.4).abs() < 0.001);
    assert!((mapping_pos.y - 0.8).abs() < 0.001);
}

#[test]
fn test_scrcpy_capture_lock_3_to_mapping_0() {
    // Portrait device (orient=0), capture locked to landscape 270° (orient=3)
    let mapping_sys = MappingCoordSys::new(0);
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(3));

    // (0.2, 0.4) in scrcpy coords
    let mapping_pos = mapping_sys.from_scrcpy(&ScrcpyPos::new(480, 432), &scrcpy_sys);

    // 270° CCW rotation: (0.2, 0.4) -> (1-0.4, 0.2) = (0.6, 0.2)
    assert!((mapping_pos.x - 0.6).abs() < 0.001);
    assert!((mapping_pos.y - 0.2).abs() < 0.001);
}

#[test]
fn test_scrcpy_capture_lock_3_to_mapping_1() {
    // Landscape device (orient=1), capture locked to landscape 270° (orient=3)
    let mapping_sys = MappingCoordSys::new(1);
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(3));

    // (0.2, 0.4) in scrcpy coords
    let mapping_pos = mapping_sys.from_scrcpy(&ScrcpyPos::new(480, 432), &scrcpy_sys);

    // 0° rotation: (0.2, 0.4) -> (0.2, 0.4)
    assert!((mapping_pos.x - 0.2).abs() < 0.001);
    assert!((mapping_pos.y - 0.4).abs() < 0.001);
}

#[test]
fn test_scrcpy_capture_lock_3_to_mapping_2() {
    // Portrait upside-down device (orient=2), capture locked to landscape 270° (orient=3)
    let mapping_sys = MappingCoordSys::new(2);
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(3));

    // (0.2, 0.4) in scrcpy coords
    let mapping_pos = mapping_sys.from_scrcpy(&ScrcpyPos::new(480, 432), &scrcpy_sys);

    // 90° CCW rotation: (0.2, 0.4) -> (0.4, 1-0.2) = (0.4, 0.8)
    assert!((mapping_pos.x - 0.4).abs() < 0.001);
    assert!((mapping_pos.y - 0.8).abs() < 0.001);
}

#[test]
fn test_scrcpy_capture_lock_3_to_mapping_3() {
    // Landscape upside-down device (orient=3), capture locked to landscape 270° (orient=3)
    let mapping_sys = MappingCoordSys::new(3);
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(3));

    // (0.2, 0.4) in scrcpy coords
    let mapping_pos = mapping_sys.from_scrcpy(&ScrcpyPos::new(480, 432), &scrcpy_sys);

    // 180° rotation: (0.2, 0.4) -> (1-0.2, 1-0.4) = (0.8, 0.6)
    assert!((mapping_pos.x - 0.8).abs() < 0.001);
    assert!((mapping_pos.y - 0.6).abs() < 0.001);
}

#[test]
fn test_visual_rotation_0_to_scrcpy_no_capture_lock() {
    let visual_sys = VisualCoordSys::new(0);
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, None);
    let video_rect = Rect::from_min_size(Pos2::new(0.0, 0.0), (1080.0, 2400.0).into());

    // (0.2, 0.4) in visual coords
    // No rotation: (0.2, 0.4) -> (0.2, 0.4)
    let scrcpy_pos = visual_sys
        .to_scrcpy(&VisualPos::new(216.0, 960.0), &video_rect, &scrcpy_sys)
        .unwrap();

    assert_eq!(scrcpy_pos.x, 216);
    assert_eq!(scrcpy_pos.y, 960);
}

#[test]
fn test_visual_rotation_1_to_scrcpy_capture_lock_1() {
    let visual_sys = VisualCoordSys::new(1);
    let video_rect = Rect::from_min_size(Pos2::new(20.0, 30.0), (1080.0, 2400.0).into());
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, None);

    // (0.2, 0.4) in visual coords
    // 90° CCW rotation: (0.2, 0.4) -> (0.4, 1-0.2) = (0.4, 0.8)
    let scrcpy_pos = visual_sys
        .to_scrcpy(&VisualPos::new(236.0, 990.0), &video_rect, &scrcpy_sys)
        .unwrap();
    assert_eq!(scrcpy_pos.x, 960);
    assert_eq!(scrcpy_pos.y, 864);
}

#[test]
fn test_visual_rotation_2_to_scrcpy_capture_lock_2() {
    let visual_sys = VisualCoordSys::new(2);
    let video_rect = Rect::from_min_size(Pos2::new(40.0, 60.0), (1080.0, 2400.0).into());
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, None);

    // (0.2, 0.4) in visual coords
    // 180° rotation: (0.2, 0.4) -> (1-0.2, 1-0.4) = (0.8, 0.6)
    let scrcpy_pos = visual_sys
        .to_scrcpy(&VisualPos::new(256.0, 1020.0), &video_rect, &scrcpy_sys)
        .unwrap();

    assert_eq!(scrcpy_pos.x, 864);
    assert_eq!(scrcpy_pos.y, 1440);
}

#[test]
fn test_visual_rotation_3_to_scrcpy_capture_lock_3() {
    let visual_sys = VisualCoordSys::new(3);
    let video_rect = Rect::from_min_size(Pos2::new(0.0, 0.0), (1080.0, 2400.0).into());
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, None);

    // (0.2, 0.4) in visual coords
    // 270° CCW rotation: (0.2, 0.4) -> (1-0.4, 0.2) = (0.6, 0.2)
    let scrcpy_pos = visual_sys
        .to_scrcpy(&VisualPos::new(216.0, 960.0), &video_rect, &scrcpy_sys)
        .unwrap();

    assert_eq!(scrcpy_pos.x, 1440);
    assert_eq!(scrcpy_pos.y, 216);
}

#[test]
fn test_scrcpy_to_visual_rotaion_0() {
    let visual_sys = VisualCoordSys::new(0);
    let video_rect = Rect::from_min_size(Pos2::new(10.0, 20.0), (1080.0, 2400.0).into());
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, None);

    // (0.2, 0.4) in scrcpy coords
    let original_pos = ScrcpyPos::new(216, 960);

    // Scrcpy -> Visual
    // No rotation: (0.2, 0.4) -> (0.2, 0.4)
    let visual_pos = visual_sys.from_scrcpy(&original_pos, &video_rect, &scrcpy_sys);
    assert!((visual_pos.x - 216.0 - 10.0).abs() < 1.0);
    assert!((visual_pos.y - 960.0 - 20.0).abs() < 1.0);
}

#[test]
fn test_scrcpy_to_visual_rotaion_1() {
    let visual_sys = VisualCoordSys::new(1);
    let video_rect = Rect::from_min_size(Pos2::new(20.0, 30.0), (1080.0, 2400.0).into());
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(1));

    // (0.2, 0.4) in scrcpy coords
    let original_pos = ScrcpyPos::new(480, 432);

    // Scrcpy -> Visual
    // 90° CW rotation: (0.2, 0.4) -> (1-0.4, 0.2) = (0.6, 0.2)
    let visual_pos = visual_sys.from_scrcpy(&original_pos, &video_rect, &scrcpy_sys);
    assert!((visual_pos.x - 648.0 - 20.0).abs() < 1.0);
    assert!((visual_pos.y - 480.0 - 30.0).abs() < 1.0);
}

#[test]
fn test_scrcpy_to_visual_rotaion_2() {
    let visual_sys = VisualCoordSys::new(2);
    let video_rect = Rect::from_min_size(Pos2::new(40.0, 60.0), (1080.0, 2400.0).into());
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(2));

    // (0.2, 0.4) in scrcpy coords
    let original_pos = ScrcpyPos::new(216, 960);

    // Scrcpy -> Visual
    // 180° rotation: (0.2, 0.4) -> (1-0.2, 1-0.4) = (0.8, 0.6)
    let visual_pos = visual_sys.from_scrcpy(&original_pos, &video_rect, &scrcpy_sys);
    assert!((visual_pos.x - 864.0 - 40.0).abs() < 1.0);
    assert!((visual_pos.y - 1440.0 - 60.0).abs() < 1.0);
}

#[test]
fn test_scrcpy_to_visual_rotaion_3() {
    let visual_sys = VisualCoordSys::new(3);
    let video_rect = Rect::from_min_size(Pos2::new(0.0, 0.0), (1080.0, 2400.0).into());
    let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(3));

    // (0.2, 0.4) in scrcpy coords
    let original_pos = ScrcpyPos::new(480, 432);

    // Scrcpy -> Visual
    // 270° CW rotation: (0.2, 0.4) -> (0.4, 1-0.2) = (0.4, 0.8)
    let visual_pos = visual_sys.from_scrcpy(&original_pos, &video_rect, &scrcpy_sys);
    assert!((visual_pos.x - 432.0).abs() < 1.0);
    assert!((visual_pos.y - 1920.0).abs() < 1.0);
}

#[test]
fn test_visual_mapping_roundtrip() {
    let visual_sys = VisualCoordSys::new(0);
    let video_rect = Rect::from_min_size(Pos2::new(10.0, 20.0), (1080.0, 2400.0).into());
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, None);
    let mapping_sys = MappingCoordSys::new(0);

    let original_pos = VisualPos::new(550.0, 1220.0);

    // Visual -> Mapping -> Visual
    let mapping_pos = visual_sys
        .to_mapping(&original_pos, &video_rect, &scrcpy_sys, &mapping_sys)
        .unwrap();
    let result_pos = visual_sys.from_mapping(&mapping_pos, &video_rect, &scrcpy_sys, &mapping_sys);

    assert!((original_pos.x - result_pos.x).abs() < 1.0);
    assert!((original_pos.y - result_pos.y).abs() < 1.0);
}
