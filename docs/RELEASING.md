# Releases

Pushing a version tag such as `v0.1.0` runs `.github/workflows/release.yml`. The tag must exactly match the package version in `Cargo.toml`. The workflow checks out that commit, runs tests, builds optimized binaries, packages every platform, and verifies their SHA-256 checksums before publishing a GitHub Release.

The publish job has the only write permission. It creates a draft first, attaches all five downloads plus `SHA256SUMS.txt`, and then publishes. A failed platform build prevents publication. A rerun can complete a draft but refuses to overwrite an already-published release. CI on `main` and pull requests uses the same build workflow without publishing.

## Creating a release

1. Update the package version in `Cargo.toml`, run `cargo check` to update `Cargo.lock`, and edit `docs/RELEASE_NOTES.md`. Once the new release is published, update the versioned download links in `README.md` to its assets so the buttons always point to available files.
2. Commit and push. Wait for CI to pass on all four targets.
3. Create an annotated tag matching that version and push it:

   ```sh
   git tag -a v0.1.0 -m 'Magpie 0.1.0'
   git push origin v0.1.0
   ```

4. Inspect the Release workflow and the resulting assets on [GitHub Releases](https://github.com/LachyFS/magpie/releases).

For recovery, the Release workflow can be dispatched manually with an existing tag. The workflow validates the tag and always builds that commit, rather than the current branch head.

Versions with a prerelease suffix (for example `v0.2.0-rc.1`) are marked as prereleases and do not replace the latest stable download.

## Packages

| Runner | Rust target | Artifacts |
| --- | --- | --- |
| Ubuntu 22.04 | `x86_64-unknown-linux-gnu` | `.tar.gz`, `.deb` |
| macOS 15, Apple Silicon | `aarch64-apple-darwin` | `.dmg` containing `Magpie.app` |
| macOS 15, Intel | `x86_64-apple-darwin` | `.dmg` containing `Magpie.app` |
| Windows Server 2022 | `x86_64-pc-windows-msvc` | portable `.zip` containing `magpie.exe` |

The Rust version, dependency lockfile, and action revisions are pinned. Every target runs the library tests and formatting checks. Linux also runs Clippy. A separate Ubuntu 24.04 job installs the Debian package and runs the native X11 interaction suite against the binary extracted from the portable archive. That job must pass before publication. macOS bundles are ad-hoc signed and verified with `codesign`; disk images are verified with `hdiutil`. Windows archives are checked for corruption. All packages include the README, license, and third-party notice.

The Windows build locates `fxc.exe` from the installed SDK and passes its path to GPUI. It uses a static MSVC runtime and a GUI subsystem, so launching from Explorer does not open a console window. The macOS app bundle contains the executable, icon, and bundle metadata. The `.deb` installs the executable, desktop launcher, icons, and documentation.

### Linux runtime

The official Linux packages are built on Ubuntu 22.04 to target glibc 2.35 and later. Building on a newer distribution can introduce a newer glibc requirement, so do not replace official artifacts with unverified local builds.

The `.deb` declares its runtime dependencies. For a portable archive on Ubuntu/Debian:

```sh
sudo apt install libxcb1 libxcb-xkb1 libxkbcommon0 libxkbcommon-x11-0 libfontconfig1 libfreetype6 libwayland-client0 libwayland-cursor0 libvulkan1 xdg-desktop-portal
```

Install your GPU's Vulkan driver as well (`mesa-vulkan-drivers` for supported Mesa GPUs). A desktop portal backend supplies native file-selection dialogs.

Use a current graphics driver. Older Mesa 23.x software Vulkan drivers can produce a blank GPUI window ([upstream report](https://github.com/zed-industries/zed/issues/16414)). The GUI test runs on Ubuntu 24.04 for its newer Mesa driver; compilation stays on Ubuntu 22.04 to retain the older glibc baseline.

## Local packaging

Build for a target, then package it on its native OS:

```sh
cargo build --release --locked --target x86_64-unknown-linux-gnu
python3 scripts/package.py --target x86_64-unknown-linux-gnu
```

`scripts/package.py` writes to `dist/` and accepts `--binary /path/to/magpie` and `--output /path/to/downloads`. It uses only Python's standard library plus `dpkg-deb`/`readelf` on Linux or `codesign`/`hdiutil` on macOS. The Debian package records the binary's actual minimum glibc version. Do not upload local build artifacts to a release unless they meet the platform baseline and have passed the same checks.

## Signing

Current macOS packages use an ad-hoc signature, not a Developer ID signature or notarization. Windows executables have no Authenticode signature. Distribution through a verified publisher requires the owner's Apple or Windows signing credentials; those are not stored in this repository. SHA-256 checksums detect download corruption but do not replace publisher signing.
