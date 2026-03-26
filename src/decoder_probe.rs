// SPDX-License-Identifier: MIT OR Apache-2.0

use {
    crate::{
        constant,
        decoder::{
            AutoDecoder,
            DecoderPreference as AppDecoderPreference,
            VideoDecoder,
            extract_resolution_from_stream,
        },
        error::Result,
        scrcpy::codec_probe::{
            DecoderPreference,
            DecoderProbe,
            ProbeStep,
            probe_device as scrcpy_probe_device,
        },
    },
    crossbeam_channel::Sender,
};

fn has_video_nal(data: &[u8]) -> bool {
    let mut i = 0;
    while i < data.len() {
        let nal_offset = if data[i..].starts_with(&[0, 0, 0, 1]) {
            i + 4
        } else if data[i..].starts_with(&[0, 0, 1]) {
            i + 3
        } else {
            i += 1;
            continue;
        };
        if nal_offset < data.len() {
            let nal_type = data[nal_offset] & 0x1f;
            if nal_type == 1 || nal_type == 5 {
                return true;
            }
        }
        i = nal_offset;
    }
    false
}

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

        for (packet, pts) in packets {
            // Skip parameter-set-only packets (SPS/PPS).  Re-feeding them to an
            // already-initialised decoder triggers AVERROR_INVALIDDATA on NVDEC
            // and a spurious "no frame!" log from the software H.264 decoder.
            // The SPS is still used above for resolution extraction.
            if !has_video_nal(packet) {
                continue;
            }
            match decoder.decode(packet, *pts) {
                Ok(Some(_)) => return true,
                Ok(None) => continue,
                Err(_) => return false,
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
