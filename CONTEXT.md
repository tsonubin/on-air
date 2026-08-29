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

A Bluetooth audio destination (BlueZ/Pulse `bluez_output.*`). Distinct from generic ALSA/HDMI outputs. Listed and connected through the Bluetooth adapter seam.

## Pipeline PCM

Processed mono L16 at the input/pipeline rate, published on an in-process bus. Senders subscribe; the bus is not a public LAN stream.

## Sonos radio

HTTP WAV body at `/stream/audio.wav`. A Sonos delivery adapter on the pipeline PCM bus. Registered only while Sonos is the exclusive output so speakers can GET it without a pairing token.
