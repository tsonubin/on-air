# on-air M1 — Linux Audio Path (Design)

Parent spec: [2026-08-26-on-air-design.md](2026-08-26-on-air-design.md) — see its
"MVP milestones" section for M1's one-line scope: input capture (PipeWire
monitor) → Sonos sender → volume/EQ → local HTTP/WS API → mDNS advertisement,
with sender exclusivity enforced in `core` even though Sonos is the only
transport implemented this milestone.

This document is the concrete design for that milestone: everything needed to
turn it into an implementation plan. It touches `packages/core` only — no
`apps/desktop` or `apps/mobile` changes. Desktop GUI wiring is M2; mobile is
M3.

## Purpose

Prove the real audio path — capture, DSP, and a real network transport — end
to end on Linux, against a real Sonos speaker, before any GUI exists to drive
it. M0 proved the plumbing (empty core, HTTP round-trip); M1 proves the
product actually plays audio somewhere.

## Scope boundary

In scope: Linux PipeWire capture, biquad EQ + `rubato` resampling, a working
`SonosSender` (SSDP discovery, UPnP/SOAP control, audio delivery over HTTP),
the REST/WS API surface M1 itself produces, and real-hardware validation
against a live Sonos speaker.

Out of scope (later milestones per the parent spec): desktop GUI, mobile app,
AirPlay, Bluetooth, macOS/Windows capture, pairing (app-level or AirPlay2).

## Architecture: pipeline & threading

```
cpal capture callback (real-time thread)
        │  raw PCM frames
        ▼
  ring buffer (lock-free, bounded — `ringbuf` crate)
        │
        ▼
async processing task (Tokio)
  biquad EQ → rubato resample
        │  processed PCM
        ▼
  broadcast channel
        │
        ▼
axum HTTP route: GET /stream/audio.wav
  (chunked transfer-encoding, only registered/active while a
   Sonos sender is running)
```

The cpal callback thread is real-time-constrained: no locks that can block
against the async side, no allocation, no I/O. It only pushes frames into the
ring buffer. Everything else — EQ, resampling, SOAP control, SSDP, the HTTP/WS
API — lives in the async Tokio world.

`AudioSender::start` for `SonosSender`:
1. Ensures `/stream/audio.wav` is serving the live processed PCM.
2. Sends `SetAVTransportURI` pointing the speaker at that endpoint.
3. Sends `Play`.

`AudioSender::stop` sends `Stop` and deregisters the audio endpoint.
`AudioSender::set_volume` calls `RenderingControl::SetVolume` directly against
the speaker — independent of the PCM path (see Volume model below).

`core`'s pipeline holds `Option<Box<dyn AudioSender>>`. Activating any sender
always calls `.stop()` on whatever is currently `Some` first — this is what
enforces the parent spec's transport-exclusivity rule, even with only
`SonosSender` implemented this milestone.

## `SonosSender`

- **Discovery**: SSDP M-SEARCH multicast (`239.255.255.250:1900`,
  `ST: urn:schemas-upnp-org:device:ZonePlayer:1`) on a background task.
  Discovered devices populate the list `GET /api/outputs` reads from. A
  device not re-confirmed by a periodic re-search within a timeout expires
  from the list (simple TTL, no persistent state across restarts).
- **Control**: a small hand-rolled SOAP client (`reqwest` POST with
  hand-built XML envelopes) implementing exactly the four actions this
  milestone needs: `SetAVTransportURI`, `Play`, `Stop`,
  `RenderingControl::SetVolume`. UPnP SOAP for these actions is a handful of
  fixed-shape requests — not enough surface to justify a generic UPnP client
  dependency.
- **Audio delivery**: `GET /stream/audio.wav` streams chunked `audio/wav`
  (PCM with a synthesized streaming WAV header) from the broadcast channel.
  Raw PCM was chosen over an MP3-encoded stream: DSP output is already PCM,
  so this is a near-direct passthrough with no encoder dependency or encode
  latency; LAN bandwidth for uncompressed audio is a non-issue.
- **Volume model**: the user-facing volume control maps to Sonos's own
  native `RenderingControl::SetVolume`, not a software gain stage in the
  PCM path. This keeps our PCM at full scale (no digital attenuation) and
  matches why the parent spec puts `set_volume` on the `AudioSender` trait
  itself rather than as a shared pipeline stage — each transport controls
  volume the way that's natural for it. The DSP "gain/amplifier" stage from
  the parent spec becomes a separate trim/boost control, distinct from the
  main volume slider, not built out further in M1 beyond what the biquad EQ
  chain already provides for gain-per-band.

## HTTP/WS API surface (M1 scope only)

REST (extends the `/api/status` router from M0):

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/inputs` | List capture sources (PipeWire monitor sources via `cpal`) |
| POST | `/api/inputs/active` | Select the active input |
| GET | `/api/outputs` | List discovered outputs (Sonos only this milestone; AirPlay/Bluetooth arrays reserved empty for M4) |
| POST | `/api/outputs/active` | Activate `{transport, device_id}` — drives `AudioSender` exclusivity |
| POST | `/api/outputs/active/volume` | `{volume: 0-100}` |
| GET/PUT | `/api/eq` | Fixed 5-band graphic EQ gains (60/250/1000/4000/12000 Hz centers, ±12dB) |
| GET/PUT | `/api/sample-rate` | Target sample rate for `rubato` |

WebSocket (`/api/ws`): `output_state_changed`, `level_meter` (periodic
RMS/peak from the DSP pipeline), `device_joined` / `device_left` (SSDP
discovery changes). No pairing-PIN or AirPlay2-PIN events yet — those belong
to M3/M4; M1's WebSocket only carries what M1 itself produces.

## Testing & validation

- **Unit**: biquad EQ correctness (known input → expected filtered output),
  `rubato` resampler correctness, SOAP envelope construction (given inputs →
  exact XML), SSDP response parsing.
- **Integration**: a fake SSDP responder (UDP socket in-test) and a fake
  UPnP/SOAP HTTP responder (`axum` test server standing in for a Sonos
  speaker) drive `SonosSender` through discover → activate → set volume →
  deactivate, asserting the right SOAP calls happen in the right order. The
  parent spec's `NullSender` fake lets pipeline/exclusivity tests run with no
  transport at all.
- **Real-hardware validation** (manual, end-of-milestone gate): point a
  running `core` at a real Sonos speaker on the LAN — confirm discovery finds
  it, audio actually plays and sounds correct after EQ/resample, volume
  changes reach the physical speaker, and stop/switch behaves cleanly. This
  is M1's actual "done" gate, the same pattern M0 used for its
  mobile↔desktop LAN check.

## Explicitly out of scope for M1

- AirPlay, Bluetooth (M4).
- macOS/Windows capture (M5).
- Desktop GUI, mobile app (M2, M3).
- App-level pairing, AirPlay2 pairing (M3, M4).
- Multi-room / simultaneous multi-speaker grouping (not in MVP scope at all
  per the parent spec).
