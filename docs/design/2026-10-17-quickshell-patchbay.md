# Compact Quickshell patchbay

The agreed surface is a small Omarchy-native popup beside the centered
clock: input and output rows, collapsed combos, one contextual volume
slider and a narrow cable gutter. Device defaults and routing are separate
operations. Body dragging combines items on one side; port dragging connects
opposite sides. Super is reserved for the compositor's gestures.

The previous core owns one input and one exclusive output. The new additive
`GET/POST /api/patchbay` adapter owns a separate graph rather than making
existing clients silently share several sessions. Its wire types live in
both API packages. All endpoints use the existing authorization boundary.

On Linux, pipewire-pulse/PulseAudio supplies null sinks, monitor sources
and loopbacks. Input mixes and output groups are buses; group membership
is flat and bounded. A directed cycle check uses the underlying sink
identity of monitor sources, so feedback through several buses is rejected.
Input contributions receive `1/N` gain; output groups fan out independently.
The core retains the graph and retries transiently unavailable devices.

Each discovered AirPlay/Sonos endpoint gets a selectable virtual sink.
Demand comes from patch links and uncorked application streams. Each active
network sink gets an isolated PCM/input/output session sharing discovery
and the root service flag, with its own nonce and stream-health counters.
No sender may displace an exclusive session using the same receiver.
Independent AirPlay uses direct pyatv sessions because OwnTone has one
global queue. Network formats and pairing support follow the existing
transports; network PCM remains mono. Cross-device synchronization is not
promised by a visual group.

Patchbay initialization is opt-in: a client opens the endpoint or a saved
patchbay exists. The periodic core task, not the shell, owns reconciliation.
Tagged modules can be recovered after a core restart. Service pause stops
loopbacks/capture/senders while preserving intent. No desktop or mobile
client is required to keep audio alive.

QML's cable drawing is driven by a bounded Verlet simulation with gravity,
fixed anchors and damping. Simulation sleeps at rest and while hidden;
reduced motion settles directly to a static hanging curve. Audio has no
dependency on the renderer or animation timing.

Validation includes real core API scenarios in mock mode, physics tests,
actual offscreen Qt input/render tests, and an optional real PulseAudio
module recovery/gain test against an isolated server. Native compositor
placement and physical-speaker latency still need review on Omarchy with
the target receivers.
