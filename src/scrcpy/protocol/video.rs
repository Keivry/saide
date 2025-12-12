/// Scrcpy Video Stream Protocol Implementation
///
/// Reference: scrcpy/server/src/main/java/com/genymobile/scrcpy/device/Streamer.java
/// Protocol version: 3.3.3
use anyhow::{Context, Result};
use {
    byteorder::{BigEndian, ReadBytesExt},
    std::io::Read,
};

/// Packet flags in pts_and_flags field (as per Streamer.java)
pub const PACKET_FLAG_CONFIG: u64 = 1 << 63; // Configuration packet (SPS/PPS)
pub const PACKET_FLAG_KEY_FRAME: u64 = 1 << 62; // Keyframe (IDR)

/// Video packet structure
///
/// Binary format:
/// ```text
/// [0-7]   u64 BE  pts_and_flags
/// [8-11]  u32 BE  packet_size
/// [12..]  [u8]    payload (H.264/H.265 NAL units)
/// ```
#[derive(Debug, Clone)]
pub struct VideoPacket {
    /// Presentation timestamp in microseconds (lower 62 bits of pts_and_flags)
    pub pts_us: u64,

    /// True if this is a configuration packet (SPS/PPS/VPS)
    pub is_config: bool,

    /// True if this is a keyframe (IDR frame)
    pub is_keyframe: bool,

    /// Encoded video data (NAL units)
    pub data: Vec<u8>,
}

impl VideoPacket {
    /// Parse video packet from stream
    ///
    /// # Protocol
    /// 1. Read 12-byte header (pts_and_flags + packet_size)
    /// 2. Read payload of specified size
    /// 3. Extract flags and PTS from pts_and_flags
    pub fn read_from<R: Read>(reader: &mut R) -> Result<Self> {
        // Read 12-byte header
        let pts_and_flags = reader
            .read_u64::<BigEndian>()
            .context("Failed to read pts_and_flags")?;
        let packet_size = reader
            .read_u32::<BigEndian>()
            .context("Failed to read packet_size")? as usize;

        // Read payload
        let mut data = vec![0u8; packet_size];
        reader
            .read_exact(&mut data)
            .context("Failed to read packet payload")?;

        // Extract flags (as per Streamer.java writeFrameMeta)
        let is_config = (pts_and_flags & PACKET_FLAG_CONFIG) != 0;
        let is_keyframe = (pts_and_flags & PACKET_FLAG_KEY_FRAME) != 0;

        // Extract PTS (lower 62 bits)
        let pts_mask = !(PACKET_FLAG_CONFIG | PACKET_FLAG_KEY_FRAME);
        let pts_us = pts_and_flags & pts_mask;

        Ok(VideoPacket {
            pts_us,
            is_config,
            is_keyframe,
            data,
        })
    }

    /// Check if packet is empty (zero-length payload)
    pub fn is_empty(&self) -> bool { self.data.is_empty() }

    /// Get packet size in bytes (header + payload)
    pub fn total_size(&self) -> usize { 12 + self.data.len() }
}

#[cfg(test)]
mod tests {
    use {super::*, byteorder::WriteBytesExt, std::io::Cursor};

    fn create_test_packet(
        pts_us: u64,
        is_config: bool,
        is_keyframe: bool,
        payload: &[u8],
    ) -> Vec<u8> {
        let mut buf = Vec::new();

        // Build pts_and_flags
        let mut pts_and_flags = pts_us;
        if is_config {
            pts_and_flags |= PACKET_FLAG_CONFIG;
        }
        if is_keyframe {
            pts_and_flags |= PACKET_FLAG_KEY_FRAME;
        }

        buf.write_u64::<BigEndian>(pts_and_flags).unwrap();
        buf.write_u32::<BigEndian>(payload.len() as u32).unwrap();
        buf.extend_from_slice(payload);

        buf
    }

    #[test]
    fn test_parse_config_packet() {
        let payload = vec![0x67, 0x42, 0x00, 0x1f]; // Fake SPS header
        let packet_bytes = create_test_packet(0, true, false, &payload);

        let mut cursor = Cursor::new(packet_bytes);
        let packet = VideoPacket::read_from(&mut cursor).unwrap();

        assert!(packet.is_config, "Should be config packet");
        assert!(!packet.is_keyframe, "Should not be keyframe");
        assert_eq!(packet.pts_us, 0);
        assert_eq!(packet.data, payload);
    }

    #[test]
    fn test_parse_keyframe_packet() {
        let payload = vec![0x65, 0x88, 0x84]; // Fake IDR slice
        let pts = 1_000_000; // 1 second
        let packet_bytes = create_test_packet(pts, false, true, &payload);

        let mut cursor = Cursor::new(packet_bytes);
        let packet = VideoPacket::read_from(&mut cursor).unwrap();

        assert!(!packet.is_config, "Should not be config packet");
        assert!(packet.is_keyframe, "Should be keyframe");
        assert_eq!(packet.pts_us, pts);
        assert_eq!(packet.data, payload);
    }

    #[test]
    fn test_parse_p_frame_packet() {
        let payload = vec![0x61, 0x9a]; // Fake P-frame slice
        let pts = 2_500_000; // 2.5 seconds
        let packet_bytes = create_test_packet(pts, false, false, &payload);

        let mut cursor = Cursor::new(packet_bytes);
        let packet = VideoPacket::read_from(&mut cursor).unwrap();

        assert!(!packet.is_config);
        assert!(!packet.is_keyframe);
        assert_eq!(packet.pts_us, pts);
        assert_eq!(packet.data, payload);
    }

    #[test]
    fn test_pts_and_flags_masking() {
        // Test that flags don't interfere with PTS value
        let max_pts = (1u64 << 62) - 1; // Maximum 62-bit value
        let packet_bytes = create_test_packet(max_pts, true, true, &[0xFF]);

        let mut cursor = Cursor::new(packet_bytes);
        let packet = VideoPacket::read_from(&mut cursor).unwrap();

        assert_eq!(packet.pts_us, max_pts);
        assert!(packet.is_config);
        assert!(packet.is_keyframe);
    }

    #[test]
    fn test_empty_payload() {
        let packet_bytes = create_test_packet(0, false, false, &[]);

        let mut cursor = Cursor::new(packet_bytes);
        let packet = VideoPacket::read_from(&mut cursor).unwrap();

        assert!(packet.is_empty());
        assert_eq!(packet.total_size(), 12);
    }

    #[test]
    fn test_large_payload() {
        let payload = vec![0xAB; 65536]; // 64KB payload
        let packet_bytes = create_test_packet(12345, false, false, &payload);

        let mut cursor = Cursor::new(packet_bytes);
        let packet = VideoPacket::read_from(&mut cursor).unwrap();

        assert_eq!(packet.data.len(), 65536);
        assert_eq!(packet.total_size(), 12 + 65536);
    }
}
