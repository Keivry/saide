/// Minimal H.264 NAL Unit Parser (Annex-B format)
///
/// Extract resolution from SPS without full decoding.
/// NAL Unit Types (ITU-T H.264 Table 7-1)
#[allow(dead_code)]
const NAL_TYPE_SPS: u8 = 7;
#[allow(dead_code)]
const NAL_TYPE_PPS: u8 = 8;
#[allow(dead_code)]
const NAL_TYPE_IDR: u8 = 5;

/// Parse Annex-B stream and find SPS NAL unit, extract resolution
///
/// Returns (width, height) or None if SPS not found
pub fn extract_resolution_from_stream(data: &[u8]) -> Option<(u32, u32)> {
    let nals = find_nal_units(data);
    tracing::trace!(
        "Found {} NAL units in stream (size={})",
        nals.len(),
        data.len()
    );

    for nal in nals {
        if nal.is_empty() {
            continue;
        }
        let nal_type = nal[0] & 0x1F;
        tracing::trace!("NAL type: {}, size: {}", nal_type, nal.len());

        if nal_type == NAL_TYPE_SPS {
            tracing::debug!("Found SPS NAL unit, parsing resolution...");
            let res = parse_sps_resolution(nal);
            if let Some((w, h)) = res {
                tracing::trace!("📐 Parsed SPS resolution: {}x{}", w, h);
            }
            return res;
        }
    }
    None
}

/// Find all NAL units in Annex-B bytestream
///
/// Annex-B format: 0x00 0x00 0x00 0x01 [NAL] 0x00 0x00 0x00 0x01 [NAL] ...
fn find_nal_units(data: &[u8]) -> Vec<&[u8]> {
    let mut nals = Vec::new();
    let mut i = 0;

    while i < data.len() {
        // Find start code (0x00 0x00 0x01 or 0x00 0x00 0x00 0x01)
        if let Some(start) = find_start_code(data, i) {
            // Find next start code
            if let Some(end) = find_start_code(data, start + 3) {
                nals.push(&data[start..end]);
                i = end;
            } else {
                nals.push(&data[start..]);
                break;
            }
        } else {
            break;
        }
    }

    nals
}

/// Find Annex-B start code position (returns position of first 0x00)
fn find_start_code(data: &[u8], offset: usize) -> Option<usize> {
    for i in offset..data.len().saturating_sub(2) {
        if data[i] == 0 && data[i + 1] == 0 {
            if data[i + 2] == 1 {
                return Some(i + 3); // Skip 0x00 0x00 0x01
            } else if i + 3 < data.len() && data[i + 2] == 0 && data[i + 3] == 1 {
                return Some(i + 4); // Skip 0x00 0x00 0x00 0x01
            }
        }
    }
    None
}

/// Parse SPS NAL unit and extract resolution
///
/// Simplified parser (ignores cropping, chroma format, etc.)
fn parse_sps_resolution(sps: &[u8]) -> Option<(u32, u32)> {
    if sps.is_empty() {
        return None;
    }

    // Skip NAL header byte
    let mut reader = BitReader::new(&sps[1..]);

    // Skip profile_idc (8 bits)
    reader.skip(8)?;

    // Skip constraint flags (8 bits)
    reader.skip(8)?;

    // Skip level_idc (8 bits)
    reader.skip(8)?;

    // Skip seq_parameter_set_id (ue(v))
    reader.read_ue()?;

    // For High profiles (profile_idc >= 100), parse chroma_format_idc
    let profile_idc = sps[1];
    if [100, 110, 122, 244, 44, 83, 86, 118, 128].contains(&profile_idc) {
        let chroma_format_idc = reader.read_ue()?;

        if chroma_format_idc == 3 {
            reader.skip(1)?; // separate_colour_plane_flag
        }

        reader.read_ue()?; // bit_depth_luma_minus8
        reader.read_ue()?; // bit_depth_chroma_minus8
        reader.skip(1)?; // qpprime_y_zero_transform_bypass_flag

        let seq_scaling_matrix_present_flag = reader.read_bit()?;
        if seq_scaling_matrix_present_flag == 1 {
            let count = if chroma_format_idc != 3 { 8 } else { 12 };
            for i in 0..count {
                let seq_scaling_list_present_flag = reader.read_bit()?;
                if seq_scaling_list_present_flag == 1 {
                    // Skip scaling list (simplified)
                    let size = if i < 6 { 16 } else { 64 };
                    let mut last_scale = 8;
                    let mut next_scale = 8;
                    for _ in 0..size {
                        if next_scale != 0 {
                            let delta_scale = reader.read_se()?;
                            next_scale = (last_scale + delta_scale + 256) % 256;
                        }
                        last_scale = if next_scale == 0 {
                            last_scale
                        } else {
                            next_scale
                        };
                    }
                }
            }
        }
    }

    // Skip log2_max_frame_num_minus4 (ue(v))
    reader.read_ue()?;

    // Skip pic_order_cnt_type (ue(v))
    let pic_order_cnt_type = reader.read_ue()?;
    if pic_order_cnt_type == 0 {
        // Skip log2_max_pic_order_cnt_lsb_minus4 (ue(v))
        reader.read_ue()?;
    } else if pic_order_cnt_type == 1 {
        // Skip delta_pic_order_always_zero_flag, offset_for_non_ref_pic,
        // offset_for_top_to_bottom_field
        reader.skip(1)?;
        reader.read_se()?;
        reader.read_se()?;
        let num_ref_frames_in_pic_order_cnt_cycle = reader.read_ue()?;
        for _ in 0..num_ref_frames_in_pic_order_cnt_cycle {
            reader.read_se()?;
        }
    }

    // Skip max_num_ref_frames (ue(v))
    reader.read_ue()?;

    // Skip gaps_in_frame_num_value_allowed_flag (1 bit)
    reader.skip(1)?;

    // Read resolution (in macroblocks - 1)
    let pic_width_in_mbs_minus1 = reader.read_ue()?;
    let pic_height_in_map_units_minus1 = reader.read_ue()?;

    // Skip frame_mbs_only_flag (1 bit)
    let frame_mbs_only_flag = reader.read_bit()?;

    if frame_mbs_only_flag == 0 {
        // Skip mb_adaptive_frame_field_flag (1 bit)
        reader.skip(1)?;
    }

    // Calculate actual resolution (16x16 macroblocks)
    let width = (pic_width_in_mbs_minus1 + 1) * 16;
    let height = (pic_height_in_map_units_minus1 + 1) * 16 * (2 - frame_mbs_only_flag as u32);

    Some((width, height))
}

/// Simplified bit-level reader for H.264 syntax elements
struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u8, // 0-7 (MSB first)
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    fn read_bit(&mut self) -> Option<u8> {
        if self.byte_pos >= self.data.len() {
            return None;
        }

        let bit = (self.data[self.byte_pos] >> (7 - self.bit_pos)) & 1;
        self.bit_pos += 1;

        if self.bit_pos == 8 {
            self.bit_pos = 0;
            self.byte_pos += 1;
        }

        Some(bit)
    }

    fn skip(&mut self, n: usize) -> Option<()> {
        for _ in 0..n {
            self.read_bit()?;
        }
        Some(())
    }

    /// Read unsigned Exp-Golomb coded value
    fn read_ue(&mut self) -> Option<u32> {
        let mut leading_zeros = 0;
        while self.read_bit()? == 0 {
            leading_zeros += 1;
        }

        if leading_zeros == 0 {
            return Some(0);
        }

        let mut value = 1u32;
        for _ in 0..leading_zeros {
            value = (value << 1) | self.read_bit()? as u32;
        }

        Some(value - 1)
    }

    /// Read signed Exp-Golomb coded value
    fn read_se(&mut self) -> Option<i32> {
        let code_num = self.read_ue()?;
        let sign = if code_num & 1 == 1 { 1 } else { -1 };
        Some(sign * ((code_num + 1) >> 1) as i32)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_parse_sps_720x1600() {
        // Example SPS NAL for 720x1600 (Baseline Profile)
        // This is a synthetic example, real SPS would be device-specific
        let _sps_data = vec![
            0x00, 0x00, 0x00, 0x01, // Start code
            0x67, // NAL header (SPS type 7)
            0x42, // profile_idc (Baseline)
            0x00, // constraints
            0x1f, // level_idc
            0xe0, /* seq_parameter_set_id (ue(v) = 0)
                   * ... rest of SPS ... */
        ];

        // This is just a placeholder test
        // Real test would need actual SPS data from device
    }
}
