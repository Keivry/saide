// SPDX-License-Identifier: MIT OR Apache-2.0

use {
    super::{mapping::MappingCoordSys, scrcpy::ScrcpyCoordSys, types::*, visual::VisualCoordSys},
    eframe::egui::{Pos2, Rect},
};

fn rotate_norm_cw(x: f32, y: f32, rotation: u32) -> (f32, f32) {
    match rotation % 4 {
        0 => (x, y),
        1 => (1.0 - y, x),
        2 => (1.0 - x, 1.0 - y),
        3 => (y, 1.0 - x),
        _ => unreachable!(),
    }
}

fn video_size_for_orientation(orientation: u32) -> (u16, u16) {
    if orientation.is_multiple_of(2) {
        (1080, 2400)
    } else {
        (2400, 1080)
    }
}

#[test]
fn test_mapping_to_scrcpy_rotation_matrix() {
    let source = MappingPos::new(0.2, 0.4);

    for display_rotation in 0..4 {
        for capture_orientation in [None, Some(0), Some(1), Some(2), Some(3)] {
            let effective_orientation = capture_orientation.unwrap_or(display_rotation);
            let (video_width, video_height) = video_size_for_orientation(effective_orientation);

            let mapping_sys = MappingCoordSys::new(display_rotation);
            let scrcpy_sys = ScrcpyCoordSys::new(video_width, video_height, capture_orientation);
            let actual = mapping_sys.to_scrcpy(&source, &scrcpy_sys);

            let rotation = capture_orientation
                .map(|capture| (capture + display_rotation) % 4)
                .unwrap_or(0);
            let (expected_x, expected_y) = rotate_norm_cw(source.x, source.y, rotation);

            assert_eq!(
                actual.x,
                (expected_x * video_width as f32) as u32,
                "display_rotation={display_rotation}, capture_orientation={capture_orientation:?}"
            );
            assert_eq!(
                actual.y,
                (expected_y * video_height as f32) as u32,
                "display_rotation={display_rotation}, capture_orientation={capture_orientation:?}"
            );
        }
    }
}

#[test]
fn test_scrcpy_to_mapping_rotation_matrix() {
    for display_rotation in 0..4 {
        for capture_orientation in [None, Some(0), Some(1), Some(2), Some(3)] {
            let effective_orientation = capture_orientation.unwrap_or(display_rotation);
            let (video_width, video_height) = video_size_for_orientation(effective_orientation);

            let mapping_sys = MappingCoordSys::new(display_rotation);
            let scrcpy_sys = ScrcpyCoordSys::new(video_width, video_height, capture_orientation);
            let source = ScrcpyPos::new(
                (0.2 * video_width as f32) as u32,
                (0.4 * video_height as f32) as u32,
            );
            let actual = mapping_sys.from_scrcpy(&source, &scrcpy_sys);

            let rotation = capture_orientation
                .map(|capture| (capture + display_rotation) % 4)
                .unwrap_or(0);
            let (expected_x, expected_y) = rotate_norm_cw(0.2, 0.4, (4 - rotation) % 4);

            assert!(
                (actual.x - expected_x).abs() < 0.001,
                "display_rotation={display_rotation}, capture_orientation={capture_orientation:?}, actual_x={}, expected_x={expected_x}",
                actual.x
            );
            assert!(
                (actual.y - expected_y).abs() < 0.001,
                "display_rotation={display_rotation}, capture_orientation={capture_orientation:?}, actual_y={}, expected_y={expected_y}",
                actual.y
            );
        }
    }
}

#[test]
fn test_visual_mapping_roundtrip_locked_capture_display_rotation_repro() {
    let visual_sys = VisualCoordSys::new(3);
    let video_rect = Rect::from_min_size(Pos2::new(0.0, 0.0), (2400.0, 1080.0).into());
    let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(0));
    let mapping_sys = MappingCoordSys::new(1);

    let original_pos = VisualPos::new(0.0, 1080.0);

    let mapping_pos = visual_sys
        .to_mapping(&original_pos, &video_rect, &scrcpy_sys, &mapping_sys)
        .unwrap();
    let result_pos = visual_sys.from_mapping(&mapping_pos, &video_rect, &scrcpy_sys, &mapping_sys);

    assert!((original_pos.x - result_pos.x).abs() < 1.0);
    assert!((original_pos.y - result_pos.y).abs() < 1.0);
}

#[test]
fn test_visual_mapping_roundtrip_locked_capture_odd_orientation_cases() {
    for (capture_orientation, display_rotation, video_rotation, video_size) in [
        (1, 1, 2, (2400.0, 1080.0)),
        (3, 1, 0, (2400.0, 1080.0)),
        (1, 3, 0, (2400.0, 1080.0)),
        (3, 3, 2, (2400.0, 1080.0)),
    ] {
        let visual_sys = VisualCoordSys::new(video_rotation);
        let video_rect = Rect::from_min_size(Pos2::new(0.0, 0.0), video_size.into());
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(capture_orientation));
        let mapping_sys = MappingCoordSys::new(display_rotation);

        for original_pos in [
            VisualPos::new(0.0, 0.0),
            VisualPos::new(video_rect.width(), 0.0),
            VisualPos::new(0.0, video_rect.height()),
            VisualPos::new(video_rect.width(), video_rect.height()),
            VisualPos::new(video_rect.width() * 0.37, video_rect.height() * 0.61),
        ] {
            let mapping_pos = visual_sys
                .to_mapping(&original_pos, &video_rect, &scrcpy_sys, &mapping_sys)
                .unwrap();
            let result_pos =
                visual_sys.from_mapping(&mapping_pos, &video_rect, &scrcpy_sys, &mapping_sys);

            assert!(
                (original_pos.x - result_pos.x).abs() < 1.0,
                "capture={capture_orientation}, display={display_rotation}, video={video_rotation}, original_x={}, result_x={}",
                original_pos.x,
                result_pos.x
            );
            assert!(
                (original_pos.y - result_pos.y).abs() < 1.0,
                "capture={capture_orientation}, display={display_rotation}, video={video_rotation}, original_y={}, result_y={}",
                original_pos.y,
                result_pos.y
            );
        }
    }
}

#[test]
fn test_visual_mapping_roundtrip_all_locked_capture_combinations() {
    for display_rotation in 0..4 {
        for capture_orientation in 0..4 {
            let video_rotation = (4 - ((capture_orientation + display_rotation) % 4)) % 4;
            let effective_orientation = capture_orientation;
            let (video_width, video_height) = video_size_for_orientation(effective_orientation);

            let visual_size = if video_rotation % 2 == 0 {
                (video_width as f32, video_height as f32)
            } else {
                (video_height as f32, video_width as f32)
            };

            let visual_sys = VisualCoordSys::new(video_rotation);
            let video_rect = Rect::from_min_size(Pos2::new(0.0, 0.0), visual_size.into());
            let scrcpy_sys =
                ScrcpyCoordSys::new(video_width, video_height, Some(capture_orientation));
            let mapping_sys = MappingCoordSys::new(display_rotation);

            for original_pos in [
                VisualPos::new(0.0, 0.0),
                VisualPos::new(video_rect.width(), 0.0),
                VisualPos::new(0.0, video_rect.height()),
                VisualPos::new(video_rect.width(), video_rect.height()),
                VisualPos::new(video_rect.width() * 0.25, video_rect.height() * 0.75),
                VisualPos::new(video_rect.width() * 0.625, video_rect.height() * 0.375),
            ] {
                let mapping_pos = visual_sys
                    .to_mapping(&original_pos, &video_rect, &scrcpy_sys, &mapping_sys)
                    .unwrap();
                let result_pos =
                    visual_sys.from_mapping(&mapping_pos, &video_rect, &scrcpy_sys, &mapping_sys);

                assert!(
                    (original_pos.x - result_pos.x).abs() < 1.0,
                    "capture={capture_orientation}, display={display_rotation}, video={video_rotation}, original_x={}, result_x={}",
                    original_pos.x,
                    result_pos.x
                );
                assert!(
                    (original_pos.y - result_pos.y).abs() < 1.0,
                    "capture={capture_orientation}, display={display_rotation}, video={video_rotation}, original_y={}, result_y={}",
                    original_pos.y,
                    result_pos.y
                );
            }
        }
    }
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
