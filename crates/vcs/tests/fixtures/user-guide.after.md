---
title: MarkDraft User Guide
version: 0.4
---

# MarkDraft User Guide

MarkDraft is a fast Markdown reader and writer for people who keep their
notes, docs and drafts as plain text files.

MarkDraft never uploads your files. Everything stays on your machine.

## Installation

Download the installer for your platform from the releases page and run it.
On macOS, drag the app into the Applications folder.

```sh
markdraft --version
markdraft open ~/Documents/notes
```

## Opening a workspace

A workspace is any folder on disk. Open one with **File → Open Folder** or
drop a folder on the window.

- The sidebar lists every Markdown file in the folder.
- Files larger than 10 MB open read-only.

## Keyboard shortcuts

| Action        | Windows / Linux | macOS   |
|---------------|-----------------|---------|
| Quick open    | Ctrl+P          | Cmd+P   |
| Toggle preview| Ctrl+Shift+V    | Cmd+Shift+V |
| Find in files | Ctrl+Shift+F    | Cmd+Option+F |
| Show changes  | Ctrl+Alt+D      | Cmd+Option+D |

## Reviewing changes

When a document lives in a Git repository, MarkDraft shows what changed
since the last commit, like tracked changes in a word processor.

## Roadmap

- [x] Live preview
- [x] Export to PDF
- [ ] Git integration

> **Note:** settings are stored per workspace in `.markdraft/`.
