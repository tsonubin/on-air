# Speaker setup and platform support

Reviewed 2026-09-16 against this checkout. This is a source audit and a UX plan;
it is not certification on Windows, Linux, or physical speakers.

## Current behavior

| Desktop host | Bluetooth | AirPlay | Phone remote |
| --- | --- | --- | --- |
| macOS | Lists Bluetooth output endpoints exposed by CoreAudio. Pair/connect new speakers in system Bluetooth settings, return, refresh, select. The app does not currently enumerate every paired-but-disconnected device. | LAN discovery and a native picker window exist, but the picker has no player/audio connection and `build_airplay` rejects activation on macOS. Mixer playback is not implemented through this picker. | Controls the Mac's endpoints. Opening Bluetooth settings opens them on the Mac, not the phone. Cannot operate a Mac picker from the phone. |
| Windows | Attempts WASAPI endpoint enumeration; opens system Bluetooth settings. Detection and state have correctness gaps listed below. | Sender attempts OwnTone, then a Python/pyatv RAOP helper. This requires a working backend and receiver compatibility. No native Windows AirPlay picker is implemented. | Uses the desktop's transport support; does not add phone Bluetooth or AirPlay capabilities. |
| Linux | Reads BlueZ device information and PulseAudio/PipeWire audio sinks; can invoke pairing/connect commands. Opens GNOME, Blueman, or KDE settings for setup. Listing cached devices is not an active scan. | Same OwnTone/pyatv sender path. AirPlay PIN pairing goes through OwnTone; fallback streaming is not equivalent to full pairing support. | Same desktop API, including commands that may take much longer than a network request. |

Sonos is a separate network transport with discovery/group handling and an exclusive
output session. Switching speakers must preserve the one-output-at-a-time invariant.
A successful request to open settings or a picker is never proof of pairing or playback.

## Findings and priorities

1. **P0 — macOS picker ownership crash (fixed in this change).** The supplied report
   crashes in `objc_release` while dropping the thread-local vector of retained
   NSWindows during termination. NSWindow defaults to release-on-close. Rust must
   own the release: set `releasedWhenClosed = false`, reuse a single window, and
   exercise close/reopen/final release on AppKit's main thread. The new regression
   first failed the ownership assertion against the old implementation, then passed
   after the fix. This validates the lifecycle without claiming the original crash
   report was reproduced byte for byte.
2. **P1 — macOS AirPlay is incomplete, not just a different button.**
   `apps/desktop/src-tauri/src/macos_airplay.rs` creates an AVRoutePickerView without
   attaching a player. `packages/core/src/session.rs::build_airplay` rejects real
   macOS activation. Complete an audio/player adapter and observe route changes
   before advertising playback. Until then, explain the limitation explicitly.
3. **P1 — Windows discovery must stop parsing opaque endpoint IDs.**
   `bluetooth_host.rs::windows::audio_devices` filters IMMDevice IDs for BTH substrings.
   Microsoft explicitly says those IDs are opaque. Enumerate Bluetooth devices with
   supported device APIs, correlate device/container properties to audio endpoints,
   and use actual endpoint state. The current ACTIVE | UNPLUGGED enumeration marks
   both as connected. A Windows runner and real paired/connected/disconnected devices
   are needed before claiming this works reliably.
4. **P1 — distinguish pairing, connection, endpoint readiness, and playback.**
   On macOS/Windows `pair()` currently just opens settings, yet the HTTP endpoint
   returns 204. Linux now requires an actual Pulse audio endpoint before connection/pairing succeeds; native settings handoffs still need explicit operation state.
   Return `awaiting_user` for a settings handoff and only mark readiness after an
   output endpoint exists. Desktop now shows setup guidance, refreshes on focus,
   offers manual refresh, and keeps activation visibly pending. Desktop Bluetooth
   pairing and activation now allow 60 seconds instead of aborting after 4 seconds.
   The mobile control client's shared 5-second timeout still needs an operation model.
5. **P1 — detect installed backends rather than infer support from OS.**
   `airplay::platform_mode()` reports `owntone` on every non-Mac host even when only
   pyatv or neither is installed. Pairing is OwnTone-specific. Probe usable backends
   and report playback and pairing capabilities separately, with setup instructions.
6. **P2 — discovery, identity, and actionable failures.** Linux listing does not
   start BlueZ discovery; macOS uses device names as IDs; low-level Bluetooth failures
   often collapse to HTTP 400/404. Add bounded scan operations, stable platform IDs,
   and errors such as adapter_off, needs_system_pairing, endpoint_not_ready, and
   backend_missing. Reopening system settings is recovery, not a successful connection.

## A2DP playback lifecycle

`BluetoothSender` owns one playback lifetime. Construction does no I/O; after the
previous output stops, start connects once and receives a typed Pulse or native
endpoint. A reopenable PCM adapter negotiates the actual format and creates a new
stream, queue, and Rate bridge. Successful rollback opens a fresh lifetime and
restores the prior app volume before consuming audio.

Volume scales this app's audio; it does not change OS master volume or rediscover
devices. Native gain is applied in the audio callback, including already queued
samples. Pulse gain is applied before writing PCM to pacat. OS/device volume
continues to limit the maximum audible level.

Stop explicitly closes playback before joining the writer. Closing pacat kills
its process without waiting for the stdin lock; closing CPAL drops the stream.
The opened handle also closes on cancellation, even if another owner retains the
sink. Failed cleanup retains ownership and blocks playback on another output.
Format polling reads a published snapshot instead of waiting for connection.

Linux preserves the original default sink before pairing/connecting and restores
it on success or failure. Already-ready Pulse endpoints do not require another
Bluetooth connection command. Real speaker behavior still needs platform QA;
Windows discovery and macOS AirPlay limitations below remain unchanged.

Regression coverage includes blocked writes and a real child-process pipe,
stop/reopen, rollback and cleanup failure, retained/failed volume changes, endpoint
readiness, backend selection, rate changes, canceled preparation, and nonblocking
format polling. Hardware-independent tests do not establish physical playback.

## Recommended shared experience

Use one visible sequence on desktop and phone:

**Find speaker → Pair if needed → Connect → Ready → Play**

Each step has its own state and explicit next action. A system handoff says where
it opens, what the user should do, and how the app will resume. When focus returns,
refresh and reconcile actual device state. If it remains unavailable, keep the
instruction and a Retry action; do not silently select another output.

Have the desktop publish runtime capabilities: discovery mode, pairing mode,
connection mode, playback support, backend readiness, and a user-facing reason for
unavailability. The phone consumes these capabilities from its paired desktop,
not from the phone's OS. Keep capability information separate from the current
connection and playback state.

Long operations should return an operation ID with phases and a bounded deadline.
Both clients observe progress via events plus polling fallback. Retry should
reconcile the operation instead of starting overlapping connections. Only offer
Cancel once the backend can actually cancel; aborting an HTTP request is insufficient.
Do not report Playing until the audio session confirms it.

Implement next in this order: capability/operation contract; Windows device mapping
and accurate Bluetooth state; macOS AirPlay audio integration; Linux scan/pairing
agent and backend setup; mobile parity. Keep the existing working transports usable
throughout rather than forcing a single OS-specific workflow onto all hosts.

## Acceptance checks

- Headers and list starts align at 800, 1024, and 1440 px; narrow windows wrap actions.
- Opening Bluetooth settings never claims playback; returning refreshes devices.
- Unavailable AirPlay explains why; it does not issue a doomed activation request.
- Repeated AirPlay open/close/reopen and final release do not crash or accumulate windows.
- Real platform QA: Bluetooth off/on, pairing rejected, paired but disconnected,
  duplicate speaker names, endpoint disappears, 30-second pairing, return from settings.
- Real transport QA: supported AirPlay receiver with/without PIN, backend missing,
  volume changes, route removal, and exclusive switching to/from Sonos/Bluetooth.
- Phone QA against each desktop OS: handoff location, reconnect, operation progress,
  retry after backgrounding, and no claim that the phone can open desktop-native UI locally.

## Primary references

- [Apple NSWindow ownership](https://developer.apple.com/documentation/appkit/nswindow/isreleasedwhenclosed)
- [Apple AVRoutePickerView](https://developer.apple.com/documentation/avkit/avroutepickerview)
- [Microsoft IMMDevice::GetId: opaque identifiers](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdevice-getid)
- [Microsoft device enumeration and pairing sample](https://learn.microsoft.com/en-us/samples/microsoft/windows-universal-samples/deviceenumerationandpairing/)
- [BlueZ device API: Pair and Connect](https://bluez.readthedocs.io/en/latest/device-api/)
- [BlueZ adapter API: discovery](https://bluez.readthedocs.io/en/latest/adapter-api/)
