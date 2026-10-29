# On-air: live dial

Logo concept for the mobile audio remote. The ivory lowercase a combines an
audio dial with the app initial; the red dot references a studio ON AIR lamp.
Colors follow the desktop console palette: charcoal RGB 21,19,16, ivory
RGB 243,234,212, red RGB 255,59,42.

The original raster concept is preserved here as `on-air-dial-concept.png`. The
shipping mark is a vector redraw of it on the 1024 pt icon canvas: a round bowl
(outer radius 271.5 pt, inner 162 pt) joined to a stem with a square top and a
rounded foot, cut by a 30 pt radial notch at about one o'clock that reads as the
dial's pointer, with the lamp above the stem. The geometry lives in
`apps/mobile/scripts/make-app-icon.mjs`.

## Icon Composer integration

`assets/AppIcon.icon` is a layered Liquid Glass icon:

- Background: the document fill, not an image. The default appearance is a
  charcoal gradient that lifts slightly toward the top; dark is a deeper charcoal.
- `Lamp` group (top): `on-air-dot.svg` in red, with glass and specular, a red
  layer-color shadow and light translucency, so it reads as a lit lamp.
- `Dial` group: `dial-glyph.svg` in ivory, with glass and specular, a neutral
  shadow and light translucency.
- Tinted and clear appearances fill both layers white so the whole mark stays
  legible in mono.

After changing the geometry or `icon.json`, run `node scripts/make-app-icon.mjs`
from `apps/mobile`. It rewrites the SVG layers, copies the package into the Xcode
project (`ios/OnAirMobile/AppIcon.icon`), and draws the flat, opaque 1024 x 1024
fallback (`assets/icon.png` and the Xcode marketing icon) from the same shapes.
Preview every appearance with Xcode's Icon Composer or its `ictool` renderer.
The native-project tests check that the two packages match, that both layers
are glass vectors with tinted fills, and that the two fallbacks are identical.
