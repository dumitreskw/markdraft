## Which file do I need?

| System | Recommended | Other options |
|---|---|---|
| **Windows 10/11 (most PCs)** | `MarkDraft-…-windows-x64-setup.exe` | `…-windows-x64.msi` (for IT deployment) · `…-windows-x64-portable.zip` (no install) |
| Windows on ARM (Surface Pro X, Snapdragon laptops) | `…-windows-arm64-setup.exe` | `…-windows-arm64-portable.zip` |
| Windows 32-bit | `…-windows-x86-setup.exe` | `…-windows-x86.msi` · `…-windows-x86-portable.zip` |
| **macOS** (any Mac) | `MarkDraft-…-macos-universal.dmg` | `…-macos-arm64.dmg` (Apple silicon, smaller) · `…-macos-x64.dmg` (Intel, smaller) |
| **Linux x64** | `…-linux-x64.AppImage` (any distribution, no install) | `…-linux-x64.deb` (Debian, Ubuntu, Mint) · `…-linux-x64.rpm` (Fedora, openSUSE) |
| Linux ARM64 (Raspberry Pi 4/5, ARM servers) | `…-linux-arm64.AppImage` | `…-linux-arm64.deb` · `…-linux-arm64.rpm` |

**Headless export** (servers, CI, scripts): `markdraft-cli-…-<os>-<arch>` turns Markdown into PDF without a window:
`markdraft-cli export notes.md -o notes.pdf --theme academic --toc`.

**Portable (Windows):** unzip anywhere and run `markdraft.exe`. Settings stay in the `data` folder next to it; delete `portable.txt` to use your user profile instead.

**Updates:** installed copies (setup.exe, MSI, macOS, AppImage) offer new versions in the app and update themselves when you click *Restart to update* (Settings → Updates). Portable, deb and rpm copies show a download link instead.

**Verify a download:** compare with `SHA256SUMS.txt`, for example `certutil -hashfile file SHA256` (Windows) or `shasum -a 256 file` (macOS/Linux).

> Until MarkDraft has paid code-signing certificates, Windows SmartScreen may say "Windows protected your PC" (choose **More info → Run anyway**) and macOS may say the developer cannot be verified (right-click the app → **Open**). See `docs/SIGNING.md`.

MarkDraft is free for personal and other noncommercial use under the PolyForm Noncommercial License 1.0.0. Commercial use needs a licence — see `COMMERCIAL.md`.
