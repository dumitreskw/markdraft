---
title: MarkDraft User Guide
version: 0.3
---

# MarkDraft User Guide

MarkDraft is a quick Markdown reader and writer for people who keep their
notes, docs and drafts as plain text files.

## Installing

Download the installer for your platform from the releases page and run it.
On macOS, drag the app into the Applications folder.

```sh
markdraft --version
markdraft open ~/notes
```

## Opening a workspace

A workspace is any folder on disk. Open one with **File → Open Folder** or
drop a folder on the window.

- The sidebar lists every Markdown file in the folder.
- Hidden files are skipped.
- Files larger than 10 MB open read-only.

## Keyboard shortcuts

| Action        | Windows / Linux | macOS   |
|---------------|-----------------|---------|
| Quick open    | Ctrl+P          | Cmd+P   |
| Toggle preview| Ctrl+Shift+V    | Cmd+Shift+V |
| Find in files | Ctrl+Shift+F    | Cmd+Shift+F |

## Roadmap

- [x] Live preview
- [ ] Export to PDF
- [ ] Git integration

MarkDraft never uploads your files. Everything stays on your machine.

> **Note:** settings are stored per workspace in `.markdraft/`.
