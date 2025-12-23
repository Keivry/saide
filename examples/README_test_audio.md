# Audio Streaming Example

Tests real-time audio capture and playback from Android device.

## Requirements

- **Android 11+ (API 30+)** - Audio capture is only supported on Android 11 and higher
- Device screen must be unlocked when starting (Android 11 only)
- Audio output device on PC

## Android Version Support

| Android Version       | API Level | Support Status              |
| --------------------- | --------- | --------------------------- |
| Android 12+           | 31+       | ✅ Works out-of-the-box     |
| Android 11            | 30        | ⚠️ Requires unlocked screen |
| Android 10 or earlier | ≤29       | ❌ Not supported            |

## Usage

```bash
# Auto-detect connected device
cargo run --example test_audio

# Specify device serial
cargo run --example test_audio <serial>
```

## Expected Output

### Android 11+ Device

```
🎵 Scrcpy Audio Streaming Test
📱 Device: ABC123
🔌 Establishing connection...
✅ Connection established!
🎧 Initializing audio...
✅ Audio initialized: 48kHz stereo
🎵 Streaming audio (10 seconds)...
  📊 Packets: 50, Decoded: 50, Buffer: 82.1%
  📊 Packets: 100, Decoded: 99, Buffer: 78.5%
  ...
📊 Statistics:
  Total packets: 495
  Decoded frames: 493
  Duration: 10.0s
✅ Test completed!
```

### Android 10 or Earlier

```
🎵 Scrcpy Audio Streaming Test
📱 Device: ELE-AL00
⚠️  Audio capture requires Android 11+ (API 30+), but device is Android 10 (API 29). Disabling audio.
✅ Connection established!
⚠️  Audio not available:
   - Device requires Android 11+ (API 30+)
💡 Tip: Use a device with Android 11+ to test audio streaming
```

## Technical Details

- **Codec**: Opus (48kHz, stereo)
- **Buffer**: 100ms ring buffer
- **Latency**: ~50-150ms (depends on network and device)
- **Sample Format**: f32 PCM (IEEE 754)

## Troubleshooting

### "Audio stream not available"

- Check Android version: `adb shell getprop ro.build.version.sdk`
- Must be ≥ 30 (Android 11+)

### "Failed to accept audio connection"

- Ensure device screen is unlocked (Android 11)
- Try restarting ADB: `adb kill-server && adb start-server`

### No sound output

- Check PC audio volume
- Verify device is playing audio (open music player)
- Check buffer fill: should be 50-95%

## See Also

- [Scrcpy Audio Documentation](../../3rd-party/scrcpy/doc/audio.md)
- [Audio Implementation](../../src/decoder/audio/)
