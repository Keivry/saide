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
            match decoder.decode(packet, *pts) {
                Ok(Some(_)) => return true,
                Ok(None) => continue,
                Err(_) => return false,
            }
        }

        false
    }
}

pub fn probe_device(
    serial: &str,
    server_jar: &str,
    progress_tx: Option<&Sender<ProbeStep>>,
) -> Result<Option<String>> {
    let decoder_probe = SaideDecoderProbe;
    let base_dir = constant::data_dir().unwrap_or_else(constant::fallback_data_path);
    scrcpy_probe_device(&decoder_probe, serial, server_jar, &base_dir, progress_tx)
}
