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

Edit only `assets/AppIcon.icon` (in Icon Composer or by hand) and the geometry in
the script. After any change, run `node scripts/make-app-icon.mjs` from
`apps/mobile`. It rewrites the SVG layers, replaces the Xcode project's copy
(`ios/OnAirMobile/AppIcon.icon`, so never edit that one), and draws the flat,
opaque 1024 x 1024 fallback (`assets/icon.png` and the Xcode marketing icon) from
the same shapes.

Preview every appearance in Icon Composer, or render them with the command-line
tool inside it, `Icon Composer.app/Contents/Executables/ictool` (the `ictool` on
the PATH is a different tool):

```sh
ictool assets/AppIcon.icon --export-image --output-file Dark.png --platform iOS \
  --rendition Dark --width 1024 --height 1024 --scale 1
```

The renditions are Default, Dark, TintedLight, TintedDark, ClearLight and
ClearDark; the tinted ones also take `--tint-color` and `--tint-strength`.

The native-project tests regenerate the layers and the fallback and fail if the
committed files are stale. They also check that both packages match and that
both layers are glass with tinted fills.
