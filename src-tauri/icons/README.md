# Tauri App Icons

This directory should contain app icons for different platforms.

## Required Icons

1. **32x32.png** - 32x32 pixel PNG icon
2. **128x128.png** - 128x128 pixel PNG icon
3. **128x128@2x.png** - 256x256 pixel PNG icon for retina displays
4. **icon.icns** - macOS icon file
5. **icon.ico** - Windows icon file

## Generating Icons

You can use the Tauri icon generator:

```bash
npm install -g @tauri-apps/cli
cargo tauri icon path/to/your/icon.png
```

Or use online tools like:
- https://www.icoconverter.com/
- https://iconverticons.com/online/

## Temporary Solution

For now, the build will fail without these icons. You can:
1. Use the Tauri CLI icon generator
2. Manually create placeholder icons
3. Download sample icons from Tauri examples
