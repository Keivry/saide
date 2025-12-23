//! Audio packet parsing for Scrcpy protocol

use {
    crate::error::{Result, SAideError},
    byteorder::{BigEndian, ReadBytesExt},
    std::io::Cursor,
};

/// Audio packet with metadata
#[derive(Debug, Clone)]
pub struct AudioPacket {
    /// Presentation timestamp (microseconds)
    pub pts: i64,

    /// Audio codec (0x6f707573 = "opus")
    pub codec_id: u32,

    /// Packet flags (reserved, currently unused)
    pub flags: u64,

    /// Audio payload (Opus/AAC/FLAC/RAW data)
    pub payload: Vec<u8>,
}

impl AudioPacket {
    /// Parse audio packet from raw bytes
    ///
    /// Format (from scrcpy server DesktopConnection.java):
    /// ```text
    /// [0-7]   pts_and_flags (u64, Big Endian)
    /// [8-11]  packet_size (u32, Big Endian)
    /// [12..]  payload
    /// ```
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 12 {
            return Err(SAideError::Decode(format!(
                "Audio packet too short: expected at least 12 bytes, got {}",
                data.len()
            )));
        }

        let mut cursor = Cursor::new(data);

        // Read header (12 bytes)
        let pts_and_flags = cursor
            .read_u64::<BigEndian>()
            .map_err(|e| SAideError::Decode(format!("Failed to read pts_and_flags: {}", e)))?;
        let packet_size = cursor
            .read_u32::<BigEndian>()
            .map_err(|e| SAideError::Decode(format!("Failed to read packet_size: {}", e)))?;

        // Extract PTS (lower 63 bits)
        let pts = (pts_and_flags & 0x7FFF_FFFF_FFFF_FFFF) as i64;
        let flags = pts_and_flags >> 63;

        // Validate packet size
        let payload_start = 12;
        let expected_total = payload_start + packet_size as usize;

        if data.len() != expected_total {
            return Err(SAideError::Decode(format!(
                "Audio packet size mismatch: expected {}, got {}",
                expected_total,
                data.len()
            )));
        }

        // Extract payload
        let payload = data[payload_start..].to_vec();

        Ok(Self {
            pts,
            codec_id: 0x6f707573, // "opus" in ASCII
            flags,
            payload,
        })
    }

    /// Get packet size
    pub fn size(&self) -> usize { 12 + self.payload.len() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_packet_parsing() {
        // Create dummy audio packet
        let mut data = Vec::new();

        // pts_and_flags (PTS=1000000 microseconds)
        data.extend_from_slice(&1000000u64.to_be_bytes());

        // packet_size (10 bytes payload)
        data.extend_from_slice(&10u32.to_be_bytes());

        // payload
        data.extend_from_slice(b"0123456789");

        let packet = AudioPacket::from_bytes(&data).unwrap();

        assert_eq!(packet.pts, 1000000);
        assert_eq!(packet.payload.len(), 10);
        assert_eq!(packet.size(), 22); // 12 header + 10 payload
    }

    #[test]
    fn test_audio_packet_too_short() {
        let data = vec![0u8; 8]; // Only 8 bytes
        let result = AudioPacket::from_bytes(&data);
        assert!(result.is_err());
    }
}
