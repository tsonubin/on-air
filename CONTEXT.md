# on-air domain

LAN-only desktop audio: capture, process, and send to **one** live output at a time.

## Exclusive output

The product invariant. Exactly one of Sonos, AirPlay, or Bluetooth may be playing. Switching tears down the previous sender first. Owned by the output **session** module.

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

Processed mono L16 at the input/pipeline rate, published on an in-process bus. Senders subscribe; the bus is not a public LAN stream.

## Sonos radio

HTTP WAV body at `/stream/audio.wav`. A Sonos delivery adapter on the pipeline PCM bus. Registered only while Sonos is the exclusive output so speakers can GET it without a pairing token.
