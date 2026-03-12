// SPDX-License-Identifier: MIT OR Apache-2.0

//! Test Planar to Packed conversion correctness

use saide::decoder::{AudioDecoder, OpusDecoder};

fn main() {
    println!("Testing Planar interleaving...\n");

    // Initialize decoder
    let mut decoder = OpusDecoder::new(48000, 2).expect("Failed to create decoder");

    // Create a simple Opus silence frame (this should decode to something)
    let silence_frame = vec![0xF8, 0xFF, 0xFE]; // Opus silence

    match decoder.decode(&silence_frame, 0) {
        Ok(Some(audio)) => {
            println!("✅ Decoded successfully!");
            println!("   Samples: {}", audio.samples.len());
            println!("   Expected: {} (480 per channel × 2 channels)", 480 * 2);
            println!("   Sample rate: {}", audio.sample_rate);
            println!("   Channels: {}", audio.channels);

            // Check if samples are interleaved correctly
            if audio.samples.len() == 960 {
                // Sample a few frames to verify L/R pattern
                println!("\n  First 10 samples (L R L R ...):");
                for i in 0..5 {
                    let l = audio.samples[i * 2];
                    let r = audio.samples[i * 2 + 1];
                    println!("    Frame {}: L={:.6}, R={:.6}", i, l, r);
                }

                // Check for obvious errors
                let (min, max) = audio
                    .samples
                    .iter()
                    .fold((f32::MAX, f32::MIN), |(min, max), &s| {
                        (min.min(s), max.max(s))
                    });

                println!("\n  Sample range: [{:.6}, {:.6}]", min, max);

                if max > 1.0 || min < -1.0 {
                    println!("  ⚠️  WARNING: Samples outside [-1.0, 1.0] range!");
                    println!("     This could cause clipping/distortion!");
                }

                // Check for NaN or Inf
                let has_nan = audio.samples.iter().any(|s| s.is_nan());
                let has_inf = audio.samples.iter().any(|s| s.is_infinite());

                if has_nan {
                    println!("  ❌ ERROR: NaN detected in samples!");
                }
                if has_inf {
                    println!("  ❌ ERROR: Inf detected in samples!");
                }

                if !has_nan && !has_inf && min >= -1.0 && max <= 1.0 {
                    println!("\n✅ All checks passed!");
                } else {
                    println!("\n❌ Issues detected!");
                }
            } else {
                println!("\n❌ ERROR: Wrong number of samples!");
            }
        }
        Ok(None) => {
            println!("⚠️  Decoder returned None (needs more data)");
        }
        Err(e) => {
            println!("❌ Decode error: {}", e);
        }
    }
}
