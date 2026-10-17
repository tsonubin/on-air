# Stereo pipeline

Status: accepted, 2026-10-17. Supersedes "network PCM remains mono" in
[the patchbay record](2026-10-17-quickshell-patchbay.md).

## Context

Since M1 every capture is folded to mono before it reaches the bus: the cpal
callback averages the device's channels, `parec` asks Pulse for one channel,
and the CD deck averages left and right. The bus, the rate bridge and the
radio WAV all carry mono L16.

Every destination is stereo already. Sonos plays a stereo WAV, and a bonded
stereo pair splits left and right on its coordinator. AirPlay RAOP is two
channels, so OwnTone and pyatv upmix our mono today. A2DP is stereo, and
`pacat` already opens two channels and duplicates mono into both. The
patchbay's null sinks are `channels=2`. Listeners lose the stereo image on
every transport, and a Sonos or HomePod stereo pair plays the same signal on
both sides.

## Decision

Pipeline PCM becomes interleaved stereo (left, right) L16 LE at the pipeline
rate, always. There is no channel setting.

- **Capture maps to stereo.** cpal prefers a 2-channel device config at the
  pipeline rate, so the OS does any surround downmix. Without one, a 1-channel
  frame is duplicated and a frame of 3+ channels keeps its first two (front
  left, front right). `parec` asks for 2 channels, and Pulse remixes them
  using the source's channel map. The CD pushes its native stereo.
- **Ring writes are whole frames.** On overrun the producer drops whole
  frames, never a single sample. Dropping one sample would swap left and
  right for the rest of the session.
- **DSP keeps channels apart.** The EQ holds one biquad state per channel.
  rubato resamples 2 channels from interleaved buffers.
- **Meter unchanged.** `LevelMeter { rms, peak }` stays one value computed
  over both channels, so the wire types and the clients don't change.
- **Radio WAV** declares 2 channels, block align 4 and a byte rate of
  rate × 4. The silence fill stays frame-aligned.
- **The rate bridge fits the destination.** 1 channel gets (L + R) / 2,
  2 channels pass through, and 3+ channels get L, R, then silence.

## Cost

Each radio stream doubles to about 1.5 Mbit/s at 48 kHz. That is 6 Mbit/s
at the 4-client cap. M1 already judged uncompressed PCM on a LAN a non-issue.
The capture ring doubles to about 768 kB at 48 kHz. DSP work per chunk
doubles. Latency is unchanged because chunks stay 1024 frames.

## Non-goals

- A mono/stereo setting, per-channel meters, or a pipeline of more than
  2 channels.
- Changes to `packages/api`, `packages/api-types` or any client.
- Synchronized playback across speakers.
