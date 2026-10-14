#!/usr/bin/env bash
# Collects one target's build output into dist/ with consistent names:
#
#   MarkDraft-<version>-<os>-<arch>-setup.exe | .msi | -portable.zip   (Windows)
#   MarkDraft-<version>-macos-<arch>.dmg                               (macOS)
#   MarkDraft-<version>-linux-<arch>.AppImage | .deb | .rpm            (Linux)
#   markdraft-cli-<version>-<os>-<arch>.zip | .tar.gz                  (headless export)
#
# Usage: scripts/package-release.sh <rust-target> <windows|macos|linux> <arch-label>
# Run from the repository root after `tauri build --target <rust-target>`
# (and `cargo build --release -p cli --target <rust-target>` for the CLI).
set -euo pipefail

target=$1 os=$2 arch=$3
version=$(node -p "require('./src-tauri/tauri.conf.json').version")
release="target/$target/release"
bundle="$release/bundle"
out=dist
mkdir -p "$out"
name="MarkDraft-$version-$os-$arch"

# Copies the single file matching a glob; fails loudly if it is missing.
take() {
  local pattern=$1 dest=$2
  local matches=($pattern)
  if [[ ! -f ${matches[0]} ]]; then
    echo "missing build output: $pattern" >&2
    exit 1
  fi
  cp "${matches[0]}" "$out/$dest"
  echo "  $dest"
}

zip_dir() { # <folder to archive> <archive path>
  local src=$1 archive=$2
  if command -v 7z >/dev/null; then
    (cd "$(dirname "$src")" && 7z a -tzip -mx=9 "$OLDPWD/$archive" "$(basename "$src")" >/dev/null)
  else
    (cd "$(dirname "$src")" && zip -qr9 "$OLDPWD/$archive" "$(basename "$src")")
  fi
}

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

echo "Packaging $name"
case $os in
  windows)
    take "$bundle/nsis/*-setup.exe" "$name-setup.exe"
    if compgen -G "$bundle/msi/*.msi" >/dev/null; then
      take "$bundle/msi/*.msi" "$name.msi"
    fi
    # Portable: the app plus a marker that keeps all data next to it.
    mkdir -p "$stage/MarkDraft"
    cp "$release/markdraft.exe" LICENSE.md "$stage/MarkDraft/"
    cat > "$stage/MarkDraft/portable.txt" <<'TXT'
This copy of MarkDraft is portable.

Settings, sessions and browser data are kept in the "data" folder next to
markdraft.exe, so the whole folder can live on a USB stick or a synced drive.
Delete this file to keep them in your user profile instead.

MarkDraft needs the Microsoft Edge WebView2 Runtime, which comes with
Windows 11 and current Windows 10. If the app does not start, install it
from https://developer.microsoft.com/microsoft-edge/webview2/
TXT
    zip_dir "$stage/MarkDraft" "$out/$name-portable.zip"
    echo "  $name-portable.zip"
    if [[ -f $release/markdraft-cli.exe ]]; then
      mkdir -p "$stage/cli"
      cp "$release/markdraft-cli.exe" LICENSE.md "$stage/cli/"
      mv "$stage/cli" "$stage/markdraft-cli-$version-$os-$arch"
      zip_dir "$stage/markdraft-cli-$version-$os-$arch" "$out/markdraft-cli-$version-$os-$arch.zip"
      echo "  markdraft-cli-$version-$os-$arch.zip"
    fi
    ;;
  macos)
    take "$bundle/dmg/*.dmg" "$name.dmg"
    ;;
  linux)
    take "$bundle/appimage/*.AppImage" "$name.AppImage"
    take "$bundle/deb/*.deb" "$name.deb"
    take "$bundle/rpm/*.rpm" "$name.rpm"
    ;;
  *)
    echo "unknown OS: $os" >&2
    exit 1
    ;;
esac

if [[ $os != windows && -f $release/markdraft-cli ]]; then
  dir="$stage/markdraft-cli-$version-$os-$arch"
  mkdir -p "$dir"
  cp "$release/markdraft-cli" LICENSE.md "$dir/"
  tar -czf "$out/markdraft-cli-$version-$os-$arch.tar.gz" -C "$stage" "$(basename "$dir")"
  echo "  markdraft-cli-$version-$os-$arch.tar.gz"
fi
