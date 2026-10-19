# on-air domain

LAN-only desktop audio: capture, process, and send to live outputs — one per
session, several at once inside a sync group.

## Exclusive output

One live sender per output **session**, and no two sessions may drive the same
receiver: a second claim is refused with "this speaker is in use by another
on-air session". The older rule — that only one transport may play at all —
is retired, because playing several kinds of speaker together is the point of
a sync group. What is still guaranteed is that two clients cannot fight over
one speaker. Owned by the output **session** module.

## Rate bridge

Capture lands on the **input** sample rate (pipeline). Each transport then converts that stream to its **output** sample rate (and channel layout). Catalogs and conversion live in the DSP rate-bridge module.

## Stereo pair

Two ZonePlayers or two HomePods bonded as one room (left/right). Cast to the **coordinator** only; bonded satellites are not separate destinations. Pairing *setup* for HomePods is an AirPlay2 PIN; Sonos stereo bonding is already done on the speakers and is discovered via ZoneGroupTopology.

## ZonePlayer

A Sonos speaker on the LAN, identified by USN/`uuid` and a UPnP `location` URL. Found by the Sonos **finder** (SSDP and `_sonos._tcp`), not by the exclusive-output session.

## A2DP sink

A Bluetooth audio destination discovered through the platform Bluetooth adapter (BlueZ, CoreAudio/IOBluetooth, or WASAPI/WinRT) and played as an OS audio sink. Distinct from generic ALSA/HDMI outputs. Paired-but-disconnected speakers are still A2DP sinks. Readiness requires a playable audio destination exposed by the operating system; pairing alone does not imply readiness. Starting, stopping, and resuming playback belong to one A2DP sink lifetime. Its app volume changes this audio stream without changing the operating system’s master volume.

## Audio CD

A compact disc of CDDA tracks in an attached optical drive. Inserting one takes over as the live **input** and starts playback from track one. Switching to another capture pauses the disc and keeps its playhead. Ejecting it leaves the input empty and clears the saved input. Distinct from capture devices listed by CPAL/Pulse.

## Pipeline PCM

Processed interleaved stereo (left, right) L16 at the input/pipeline rate, published on an in-process bus. Senders subscribe; the bus is not a public LAN stream.

## Sonos radio

HTTP WAV body at `/stream/<nonce>/audio.wav`, where the 32-hex nonce is minted per activation. A Sonos and AirPlay delivery adapter on the pipeline PCM bus. Served only while that output is live, so speakers can GET it without a pairing token and an old URL stops working on the next switch.

## Control surface

A client of the core's control plane that shows and changes state but owns
no playback: the terminal UI (`apps/tui`), the Tauri window, the phone
remote. Closing any of them changes nothing about what is playing.

## Service

The core process (`on-air-core`, or the core embedded in the Tauri app). It
owns playback, discovery and the sleep inhibitor, and it is what autostart
starts: on Linux the `on-air-core` systemd user unit, on macOS and Windows
the desktop app's login item.

## Error envelope

Every non-2xx API response is JSON `{"error", "code"}`: clients map the stable snake_case `code` (`ApiErrorCode` in `packages/api-types`) to their own copy and never match on the `error` text.

## Sync group

Outputs joined to play the same instant of audio together. Members share one
presentation epoch and each holds a delay line, so a transport that would
otherwise arrive early waits for the slowest member. Joining is exclusive: an
output already in another group is refused rather than silently moved. A group
is *more* constrained than two ungrouped outputs, never less — the ungrouped
ones are free to run at their own pace and drift.

## Presentation epoch

The wall-clock instant every member of a sync group begins at. Starting a group
pre-rolls each member's delay line and then releases them together, rather than
telling each transport to start as soon as it happens to be ready.

## Delay line

Per-output buffer holding a feed back by a whole number of frames, so the output
is the input delayed and nothing is dropped or duplicated. A fast transport's
delay line is what makes it wait for a slow one. It opens with silence, so the
feed rate is unchanged and a live stream keeps flowing.

## Latency trim

A per-output correction in milliseconds, applied on top of that transport's
latency default. It is a **manual** correction: every default in the tree is an
uncalibrated estimate and the trim exists because of that. A trim is not a
measurement, and the UI must never present one as such.

## Calibration

Measuring a transport's real latency by playing a known probe signal through it
and cross-correlating what a microphone hears against what was played. A
calibrated value replaces the default and is marked as measured. Until one is
taken, that transport's latency is a documented guess.

## Channel mode

Per-output choice of `auto`, `stereo` or `mono`. `auto` follows the device;
`mono` is a true `(L+R)/2` downmix to one sample per frame; `stereo` on a device
that only plays mono is **refused** rather than silently losing a channel.

## DSP quality

Per-machine resampler choice — `economy`, `balanced` or `high` — trading CPU for
conversion quality. on-air targets old hardware, so the audio check measures
each profile on the machine and recommends one rather than assuming.
