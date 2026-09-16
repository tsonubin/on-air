# On-air: live dial

Logo concept for the mobile audio remote. The ivory lowercase a combines an audio dial with the app initial; the red dot references a studio ON AIR lamp. Colors follow the desktop console palette.

Generated with the built-in image generation tool. Selected for mobile version 0.1.0 (2); the original is preserved here and the shipping icon is resized to an opaque 1024 by 1024 PNG.

## Final generation prompt

Create a clean final app logo, 1024x1024 square. Entire image MUST be fully opaque, no transparency or alpha cutouts anywhere. Solid flat charcoal background RGB 21,19,16 fills the entire canvas. On it draw a solid warm ivory geometric lowercase a emblem: a thick circular ring joined to a straight vertical stem on the right, with a narrow diagonal notch cut across the ring at its upper-right one-o'clock position. Above the stem, a separate solid red circular indicator light. The mark represents an audio dial and the red ON AIR lamp for a sophisticated audio mixer app. Ring outside diameter about 550px, stroke 100px, stem width 100px. Total emblem including red dot optically centered, around 630px high with generous margins. Red dot diameter 110px. Match the provided image's core design idea but FIX ALL rendering flaws. Absolutely smooth clean filled shapes and crisp edges. Ivory is RGB 243,234,212, red is RGB 255,59,42. No speckles, no noise, no distressed edges, no scratches, no texture, no gradient, no shadows, no text, no mockup, no border. A polished minimalist professional app icon. Render the background and emblem as opaque colored pixels throughout.

## Icon Composer integration

The Expo and Xcode `AppIcon.icon` packages embed the same Live Dial artwork as the
flat fallback and desktop icons. This supersedes the earlier microphone artwork
from PR #2. The current mark is one opaque image with glass, translucency, and
specular effects disabled; it does not claim separately editable foreground layers.
Both package copies and the fallback are checked by the native-project tests.
