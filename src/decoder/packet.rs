use {
    super::error::{Result, VideoError},
    ffmpeg_next as ffmpeg,
};

pub(super) fn send_av_packet(
    decoder: &mut ffmpeg::decoder::Video,
    data: &[u8],
    pts: i64,
) -> Result<()> {
    let mut packet = ffmpeg::Packet::new(data.len());
    packet.data_mut().unwrap().copy_from_slice(data);
    packet.set_pts(Some(pts));
    packet.set_dts(Some(pts));

    decoder.send_packet(&packet).map_err(VideoError::from)?;

    Ok(())
}
