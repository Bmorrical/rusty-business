# apps/desktop/src-tauri/icons

The app's icon, in each size and format macOS, Windows, and Linux ask for.

`tauri.conf.json` names the five the bundle uses: `32x32.png`, `128x128.png`,
`128x128@2x.png`, `icon.icns` (macOS), and `icon.ico` (Windows). The `Square`
and `StoreLogo` files are for the Windows Store.

These are still the icons Tauri ships with a new project. To replace them
all from one square image, run this in [../..](../..):

```bash
npm run tauri icon path/to/image.png
```
