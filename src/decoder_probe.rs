// SPDX-License-Identifier: MIT OR Apache-2.0

use {
    crate::{
        constant,
        decoder::{
            extract_resolution_from_stream,
            AutoDecoder,
            DecoderPreference as AppDecoderPreference,
            VideoDecoder,
        },
        error::Result,
        scrcpy::codec_probe::{
            probe_device as scrcpy_probe_device,
            DecoderPreference,
            DecoderProbe,
            ProbeStep,
        },
    },
    crossbeam_channel::Sender,
    ffmpeg_next as ffmpeg,
};

pub struct SaideDecoderProbe;

impl DecoderProbe for SaideDecoderProbe {
    fn hardware_candidates(&self) -> &'static [DecoderPreference] {
        #[cfg(target_os = "windows")]
        {
            const CANDIDATES: &[DecoderPreference] =
                &[DecoderPreference::Nvdec, DecoderPreference::D3d11va];
            CANDIDATES
        }

        #[cfg(not(target_os = "windows"))]
        {
            const CANDIDATES: &[DecoderPreference] =
                &[DecoderPreference::Nvdec, DecoderPreference::Vaapi];
            CANDIDATES
        }
    }

    fn validate(&self, candidate: DecoderPreference, packets: &[(Vec<u8>, i64)]) -> bool {
        let Some((width, height)) = packets
            .iter()
            .find_map(|(packet, _)| extract_resolution_from_stream(packet))
        else {
            return false;
        };

        let preferred = match candidate {
            DecoderPreference::Nvdec => AppDecoderPreference::Nvdec,
            #[cfg(target_os = "windows")]
            DecoderPreference::D3d11va => AppDecoderPreference::D3d11va,
            #[cfg(not(target_os = "windows"))]
            DecoderPreference::Vaapi => AppDecoderPreference::Vaapi,
        };

        let mut decoder = match AutoDecoder::new_exact(width, height, preferred) {
            Ok(decoder) => decoder,
            Err(_) => return false,
        };

        // Suppress FFmpeg log output during probe to avoid spurious messages
        // like "no frame!" (AV_LOG_ERROR in FFmpeg 8.1) and "non-existing PPS"
        // from NVDEC when the bitstream is being initialized.  Restore the level
        // after the probe so normal decoding keeps its configured verbosity.
        let prev_log_level = unsafe { ffmpeg::sys::av_log_get_level() };
        unsafe { ffmpeg::sys::av_log_set_level(ffmpeg::sys::AV_LOG_FATAL) };

        let validated = (|| {
            for (packet, pts) in packets {
                match decoder.decode(packet, *pts) {
                    Ok(Some(_)) => return true,
                    // Treat decode errors the same as "no frame yet": the only
                    // Err from NvdecDecoder::decode is the consecutive-empty-frames
                    // heuristic, which fires during bitstream initialization when
                    // SPS/PPS and IDR arrive in separate packets.  Keep feeding
                    // all packets before giving up; the flush below is the final verdict.
                    Ok(None) | Err(_) => continue,
                }
            }

            // Hardware decoders (e.g. AMD VAAPI) maintain an internal frame buffer and
            // may not emit any frame until the pipeline is flushed, even after receiving
            // a full IDR + several P-frames.  Flush here to drain any buffered output
            // before concluding that this decoder candidate is unsupported.
            match decoder.flush() {
                Ok(frames) => !frames.is_empty(),
                Err(_) => false,
            }
        })();

        unsafe { ffmpeg::sys::av_log_set_level(prev_log_level) };

        validated
    }
}

pub fn probe_device(
    serial: &str,
    server_jar: &str,
    progress_tx: Option<&Sender<ProbeStep>>,
) -> Result<Option<String>> {
    let decoder_probe = SaideDecoderProbe;
    let config_dir = constant::config_dir();
    scrcpy_probe_device(&decoder_probe, serial, server_jar, &config_dir, progress_tx)
}
