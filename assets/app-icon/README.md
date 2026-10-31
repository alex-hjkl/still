# Application icon

Original artwork for Still: a sound wave settling into calm water. `still.svg`
is the source artwork. The system icon uses a fixed Macchiato tile and blue wave.
Inside the app, `still-wave.svg` and `still-ripples.svg` provide monochrome SVG
masks; the tile, wave and ripples use the active theme’s colors.

- `still.png`: 1024 × 1024 RGBA export.
- `still.icns`: macOS bundle icon, including Retina sizes.
- `still.ico`: Windows resource icon (16, 24, 32, 48, 64, 128 and 256 pixels).

The macOS packager copies `still.icns` into the app's Resources directory.
`build.rs` embeds `still.ico` as resource ID 1, which GPUI uses for its windows.
Linux packages include the SVG in the hicolor icon theme and a desktop entry
matching the GPUI app ID, `com.still.player`.

The exports were generated from the SVG using resvg 2.6.2 and Pillow 13.0.0. They are
checked in, so ordinary builds need neither tool. To update them, export the
SVG to a 1024-pixel PNG and use Pillow's ICNS/ICO writers with the sizes above.
Keep the wave and ripple masks synchronized with the original SVG paths and view box.
Visually inspect all exports and update their exact SHA-256 values in
`manifest.json`; the publication guard only accepts those reviewed binaries.
