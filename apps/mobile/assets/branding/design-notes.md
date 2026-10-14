# On-air: live dial

Logo concept for the mobile audio remote. The ivory lowercase a combines an
audio dial with the app initial; the red dot references a studio ON AIR lamp.
Colors follow the desktop console palette: charcoal RGB 21,19,16, ivory
RGB 243,234,212, red RGB 255,59,42.

The original 1024 x 1024 render is preserved here as `on-air-dial-concept.png`;
the shipping icon is the same artwork resized to an opaque PNG.

## Icon Composer integration

The Expo and Xcode `AppIcon.icon` packages embed the same Live Dial artwork as
the flat fallback and desktop icons. The mark is one opaque image with glass,
translucency, and specular effects disabled; it does not claim separately
editable foreground layers. Both package copies and the fallback are checked by
the native-project tests.
