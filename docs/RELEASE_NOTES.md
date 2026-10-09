Collect what catches your eye. Magpie is a native mood board built with Rust and GPUI.

- Smooth, pointer-anchored zoom with an editable percentage menu and quick presets.
- Zoom far beyond the old 8%–600% range, with double-precision coordinates and an adaptive grid. Numerical safety range: 0.0000001%–100,000,000,000%.
- Native macOS trackpad pinch support. Ctrl/Cmd+scroll zooms; ordinary scrolling pans, matching familiar design tools.
- Shift+1 fits all images, Shift+2 fits the selection, and Shift+0 restores 100%. Plus/minus shortcuts work with or without Ctrl/Cmd.
- Updated README screenshots and native interaction tests, including image rendering at extreme zoom.

### Downloads

| Platform | Package |
| --- | --- |
| Linux x86-64 | `.deb` for Ubuntu 22.04+/Debian 12+, or `.tar.gz` for compatible Linux distributions |
| macOS Apple Silicon | `macos-aarch64.dmg` |
| macOS Intel | `macos-x86_64.dmg` |
| Windows x86-64 | `windows-x86_64.zip` — extract and run `magpie.exe` |

Linux requires a Vulkan-capable graphics driver. macOS packages have an ad-hoc signature and are not Apple-notarized. Windows packages are not Authenticode-signed. Your OS may require approval to open a downloaded app. See the README for installation details.

SHA-256 checksums for every download are included in `SHA256SUMS.txt`.
