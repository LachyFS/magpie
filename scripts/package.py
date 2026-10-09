#!/usr/bin/env python3
"""Package a prebuilt Magpie release using only Python's standard library."""
import argparse
import hashlib
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import tarfile
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
TARGETS = {
    "x86_64-unknown-linux-gnu": "linux-x86_64",
    "aarch64-apple-darwin": "macos-aarch64",
    "x86_64-apple-darwin": "macos-x86_64",
    "x86_64-pc-windows-msvc": "windows-x86_64",
}


def version():
    package = (ROOT / "Cargo.toml").read_text().split("[package]", 1)[1].split("\n[", 1)[0]
    return re.search(r'^version\s*=\s*"([^"]+)"', package, re.MULTILINE).group(1)


def copy_docs(destination):
    for name in ["README.md", "LICENSE", "NOTICE"]:
        shutil.copy2(ROOT / name, destination / name)
    shutil.copytree(ROOT / "docs", destination / "docs")


def package(target, binary, output):
    release = version()
    stem = f"magpie-{release}-{TARGETS[target]}"
    output.mkdir(parents=True, exist_ok=True)
    artifacts = []
    with tempfile.TemporaryDirectory(prefix="magpie-package-") as temporary:
        work = Path(temporary)
        bundle = work / stem
        bundle.mkdir()
        if "linux" in target:
            shutil.copy2(binary, bundle / "magpie")
            (bundle / "magpie").chmod(0o755)
            copy_docs(bundle)
            for name in ["magpie.svg", "magpie.png", "magpie.desktop"]:
                shutil.copy2(ROOT / "assets" / name, bundle / name)
            archive = output / f"{stem}.tar.gz"
            with tarfile.open(archive, "w:gz") as tar:
                tar.add(bundle, arcname=stem, filter=normalize_tar_owner)
            artifacts.append(archive)

            deb = work / "deb"
            for relative in ["DEBIAN", "usr/bin", "usr/share/applications", "usr/share/icons/hicolor/256x256/apps", "usr/share/icons/hicolor/scalable/apps", "usr/share/doc/magpie"]:
                (deb / relative).mkdir(parents=True)
            shutil.copy2(bundle / "magpie", deb / "usr/bin/magpie")
            shutil.copy2(ROOT / "assets/magpie.desktop", deb / "usr/share/applications/app.magpie.Magpie.desktop")
            shutil.copy2(ROOT / "assets/magpie-256.png", deb / "usr/share/icons/hicolor/256x256/apps/magpie.png")
            shutil.copy2(ROOT / "assets/magpie.svg", deb / "usr/share/icons/hicolor/scalable/apps/magpie.svg")
            copy_docs(deb / "usr/share/doc/magpie")
            installed_kib = (sum(p.stat().st_size for p in (deb / "usr").rglob("*") if p.is_file()) + 1023) // 1024
            versions = re.findall(r"GLIBC_(\d+)\.(\d+)", subprocess.check_output(["readelf", "--version-info", str(binary)], text=True))
            minimum_glibc = ".".join(map(str, max((int(a), int(b)) for a, b in versions)))
            (deb / "DEBIAN/control").write_text(
                f"Package: magpie\nVersion: {release.replace('-', '~')}\nArchitecture: amd64\n"
                "Maintainer: LachyFS <100457804+LachyFS@users.noreply.github.com>\n"
                f"Installed-Size: {installed_kib}\nSection: graphics\nPriority: optional\n"
                f"Depends: libc6 (>= {minimum_glibc}), libgcc-s1, libstdc++6, libxcb1, libxcb-xkb1, libxkbcommon0, libxkbcommon-x11-0, libfontconfig1, libfreetype6, libwayland-client0, libwayland-cursor0, libvulkan1\n"
                "Recommends: xdg-desktop-portal\nHomepage: https://github.com/LachyFS/magpie\n"
                "Description: A native infinite mood board\n Collect what catches your eye with image drag and drop, a dark infinite\n canvas, quick boards, and local autosave. Built with Rust and GPUI.\n",
                encoding="utf-8",
            )
            archive = output / f"{stem}.deb"
            subprocess.run(["dpkg-deb", "--build", "--root-owner-group", str(deb), str(archive)], check=True)
            artifacts.append(archive)
        elif "apple" in target:
            app = bundle / "Magpie.app/Contents"
            (app / "MacOS").mkdir(parents=True)
            (app / "Resources").mkdir()
            shutil.copy2(binary, app / "MacOS/magpie")
            (app / "MacOS/magpie").chmod(0o755)
            shutil.copy2(ROOT / "assets/magpie.icns", app / "Resources/magpie.icns")
            with (app / "Info.plist").open("wb") as info:
                plistlib.dump({
                    "CFBundleName": "Magpie", "CFBundleDisplayName": "Magpie",
                    "CFBundleIdentifier": "app.magpie.Magpie", "CFBundleExecutable": "magpie",
                    "CFBundlePackageType": "APPL", "CFBundleIconFile": "magpie.icns",
                    "CFBundleShortVersionString": release.split("-")[0],
                    "CFBundleVersion": release.split("-")[0], "LSMinimumSystemVersion": "11.0",
                    "NSHighResolutionCapable": True,
                    "NSHumanReadableCopyright": "Magpie contributors. Apache-2.0.",
                }, info)
            subprocess.run(["codesign", "--force", "--sign", "-", str(app.parent)], check=True)
            subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app.parent)], check=True)
            copy_docs(bundle)
            (bundle / "Applications").symlink_to("/Applications")
            archive = output / f"{stem}.dmg"
            subprocess.run(["hdiutil", "create", "-volname", "Magpie", "-srcfolder", str(bundle), "-ov", "-format", "UDZO", str(archive)], check=True)
            subprocess.run(["hdiutil", "verify", str(archive)], check=True)
            artifacts.append(archive)
        else:
            shutil.copy2(binary, bundle / "magpie.exe")
            copy_docs(bundle)
            archive = output / f"{stem}.zip"
            with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zip_file:
                for path in sorted(bundle.rglob("*")):
                    zip_file.write(path, f"{stem}/{path.relative_to(bundle).as_posix()}")
            with zipfile.ZipFile(archive) as zip_file:
                if zip_file.testzip() is not None:
                    raise RuntimeError("Archive integrity check failed")
            artifacts.append(archive)
    for path in artifacts:
        with path.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest() if hasattr(hashlib, "file_digest") else hashlib.sha256(stream.read()).hexdigest()
        # GNU sha256sum treats CRLF's CR as part of the filename on older Linux
        # releases, so checksums must use LF even when packaged on Windows.
        path.with_name(path.name + ".sha256").write_bytes(f"{digest}  {path.name}\n".encode("utf-8"))
        print(path)


def normalize_tar_owner(info):
    info.uid = info.gid = 0
    info.uname = info.gname = "root"
    return info


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--print-version", action="store_true")
    parser.add_argument("--validate-tag")
    parser.add_argument("--target", choices=TARGETS)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--output", type=Path, default=ROOT / "dist")
    args = parser.parse_args()
    if args.print_version:
        print(version())
        return
    if args.validate_tag:
        if args.validate_tag != f"v{version()}":
            parser.error(f"Tag must match Cargo.toml: v{version()}")
        return
    if not args.target:
        parser.error("--target is required")
    filename = "magpie.exe" if "windows" in args.target else "magpie"
    binary = args.binary or ROOT / "target" / args.target / "release" / filename
    if not binary.is_file():
        parser.error(f"Build the release binary first: {binary}")
    package(args.target, binary.resolve(), args.output.resolve())


if __name__ == "__main__":
    main()
