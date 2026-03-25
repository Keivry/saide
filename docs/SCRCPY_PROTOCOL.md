# SAide scrcpy Protocol Notes

This document describes the scrcpy protocol behavior that SAide currently relies on and the parts that are implemented in this repository. It is not a full upstream scrcpy specification.

Protocol facts below are aligned with:

- `3rd-party/scrcpy-rs/src/connection.rs`
- `3rd-party/scrcpy-rs/src/protocol/control.rs`
- `3rd-party/scrcpy-rs/src/protocol/video.rs`
- `3rd-party/scrcpy-rs/src/protocol/audio.rs`
- scrcpy server version `3.3.3`

## Connection model

SAide uses the standard scrcpy three-socket model:

1. video stream
2. audio stream (optional)
3. control stream

The current implementation in `ScrcpyConnection::connect()` accepts them in exactly that order.

### Handshake sequence used by SAide

1. Resolve or push `scrcpy-server-v3.3.3`.
2. Reserve a local TCP port from `DEFAULT_PORT_RANGE`.
3. Install an ADB reverse tunnel.
4. Start the Android server process.
5. Accept the video socket.
6. Accept the audio socket if audio is enabled.
7. Accept the control socket.
8. If requested, read device metadata from the video socket.
9. If requested, read video codec metadata from the video socket.
10. If requested and audio is enabled, read audio codec metadata from the audio socket.

### Audio gating

SAide checks the device Android API level before enabling audio. Devices below Android 11 / API 30 are downgraded to video + control only, and the reason is exposed as `AudioDisabledReason::UnsupportedAndroidVersion`.

## Device metadata

When `send_device_meta` is enabled, scrcpy sends a fixed 64-byte device name field.

- encoding: UTF-8
- size: 64 bytes
- padding: trailing `\0`
- sender: the first available media socket, which in SAide's flow is the video socket

## Video codec metadata

When `send_codec_meta` is enabled, SAide expects 12 bytes before normal video packets:

```text
0..4   codec_id   u32 big-endian
4..8   width      u32 big-endian
8..12  height     u32 big-endian
```

Common upstream codec ids include:

- `h264 = 0x68323634`
- `h265 = 0x68323635`
- `av1  = 0x00617631`

`3rd-party/scrcpy-rs/src/connection.rs` reads this metadata and stores the resolution on the connection object.

## Video packet format

`3rd-party/scrcpy-rs/src/protocol/video.rs` parses the current wire format used by SAide.

Each packet is:

```text
0..8   pts_and_flags  u64 big-endian
8..12  packet_size    u32 big-endian
12..   payload
```

### Flags layout

- bit 63: config packet
- bit 62: key frame
- low 62 bits: PTS in microseconds

This matches the packet parser constants:

- `PACKET_FLAG_CONFIG = 1 << 63`
- `PACKET_FLAG_KEY_FRAME = 1 << 62`

SAide also protects itself with `MAX_PACKET_SIZE = 10 * 1024 * 1024` from `src/constant.rs`.

## Audio codec metadata

When audio codec metadata is requested, the upstream format is 4 bytes:

```text
0..4  codec_id  u32 big-endian
```

Common upstream values include:

- `opus = 0x6f707573`
- `aac  = 0x00616163`
- `flac = 0x666c6163`
- `raw  = 0x00726177`

SAide's connection layer also treats two special values as explicit device-side failures:

- `0`: audio stream disabled by device
- `1`: audio configuration error on the device side

## Audio packet format in SAide

The audio data path in `3rd-party/scrcpy-rs/src/protocol/audio.rs` parses a 12-byte packet header followed by payload:

```text
0..8   pts_and_flags  u64 big-endian
8..12  packet_size    u32 big-endian
12..   payload
```

Current implementation details:

- `pts` is derived from the low 63 bits of `pts_and_flags`
- the high bit is exposed as a single generic flag value
- `codec_id` is currently hard-coded to Opus (`0x6f707573`) inside SAide's packet struct

That means the current audio parser is intentionally narrower than upstream scrcpy's full multi-codec behavior. The production path is effectively an Opus-first implementation.

## Frame metadata mode

When scrcpy frame metadata is enabled upstream, media packets use a 12-byte header where:

- 8 bytes are `pts_and_flags`
- 4 bytes are `packet_size`

For video, SAide fully models the known config/keyframe bits. For audio, the current implementation only keeps the highest-bit flag and does not mirror all upstream codec-specific handling.

## Control message framing

Control messages use the standard scrcpy layout:

```text
[type:1][payload...]
```

All integer payloads are serialized big-endian.

### Message type enum present in the repository

`3rd-party/scrcpy-rs/src/protocol/control.rs` keeps the upstream type numbers 0 through 17:

| Type | Name |
| --- | --- |
| 0 | InjectKeycode |
| 1 | InjectText |
| 2 | InjectTouchEvent |
| 3 | InjectScrollEvent |
| 4 | BackOrScreenOn |
| 5 | ExpandNotificationPanel |
| 6 | ExpandSettingsPanel |
| 7 | CollapsePanels |
| 8 | GetClipboard |
| 9 | SetClipboard |
| 10 | SetDisplayPower |
| 11 | RotateDevice |
| 12 | UhidCreate |
| 13 | UhidInput |
| 14 | UhidDestroy |
| 15 | OpenHardKeyboardSettings |
| 16 | StartApp |
| 17 | ResetVideo |

### Messages currently serialized by SAide

The current `ControlMessage` implementation actively serializes these commands:

- `InjectKeycode`
- `InjectText`
- `InjectTouchEvent`
- `InjectScrollEvent`
- `BackOrScreenOn`
- `ExpandNotificationPanel`
- `ExpandSettingsPanel`
- `CollapsePanels`
- `SetDisplayPower`
- `RotateDevice`

The enum keeps the other upstream type numbers for protocol alignment, but SAide does not currently expose or serialize all of them.

### Current control-message details worth knowing

#### InjectText

- UTF-8 only
- text is truncated to at most 300 bytes

#### InjectTouchEvent

- serialized size: 32 bytes
- pressure is clamped to `0.0..=1.0` and converted to `u16`
- special pointer ids defined in code:
  - `POINTER_ID_MOUSE = u64::MAX`
  - `POINTER_ID_GENERIC_FINGER = u64::MAX - 1`
  - `POINTER_ID_VIRTUAL_FINGER = u64::MAX - 2`

Position serialization is 12 bytes total:

```text
x            u32
y            u32
screen_width u16
screen_height u16
```

#### InjectScrollEvent

- scroll values are first divided by 16
- normalized to `-1.0..=1.0`
- then converted to signed 16-bit fixed-point values

## What SAide does not currently implement

This repository does not currently provide a complete client-side mirror of every scrcpy protocol feature.

Examples of upstream capabilities that are not fully implemented in SAide today:

- clipboard sync messages
- UHID device lifecycle messages
- start-app control message support
- reset-video control message support
- a general device-message parser module
- full upstream audio codec special-case parsing for Opus / FLAC config packets

The absence of those features is a code-level fact, not a compatibility score. This document intentionally avoids percentage-based compliance claims.

## Practical implications for contributors

- If you are working on input or gesture behavior, start with `3rd-party/scrcpy-rs/src/protocol/control.rs` and `src/controller/control_sender.rs`.
- If you are debugging resolution or handshake issues, inspect `3rd-party/scrcpy-rs/src/connection.rs`.
- If you are debugging stutter or malformed media packets, inspect `3rd-party/scrcpy-rs/src/protocol/video.rs` and `3rd-party/scrcpy-rs/src/protocol/audio.rs`.
- If you want to extend protocol coverage, add the missing message model first, then update this document to describe the new real behavior.
