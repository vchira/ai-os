# Going Public — License, Release Pipeline, Website, README

**Date:** 2026-03-18
**Status:** Approved
**Scope:** Prepare AiOS for public release: license, release script with secret scanning, GitHub README, project website on Cloudflare Pages, and deploy script.

## 1. License

### BSL 1.1 (Business Source License)

- **Copyright holder:** swIT.work GmbH, Austria
- **Change Date:** 5 years from each release date
- **Change License:** Apache License, Version 2.0
- **Additional Use Grant:** Use for personal, educational, internal business, and evaluation purposes is permitted without requiring approval from swIT.work GmbH.

**Restricted (requires written approval from swIT.work GmbH):**
- Commercial distribution or resale of AiOS
- Creating derivative Linux distributions based on AiOS
- Offering AiOS as a hosted or managed service
- Bundling AiOS with commercial hardware

### Files to create/change

| File | Action |
|------|--------|
| `LICENSE` | Create — full BSL 1.1 text with above parameters |
| `Cargo.toml` | Replace `license = "MIT"` with `license-file = "LICENSE"` in `[workspace.package]` (BSL 1.1 has no standard SPDX identifier) |
| `docs/user-guide/src/license.md` | Update to reference BSL 1.1 and explain the terms |

## 2. Versioning

### Version scheme

- Single source of truth: `VERSION` file in repo root
- Format: `MAJOR.MINOR.PATCH` for stable, `MAJOR.MINOR.PATCH-CHANNEL.N` for pre-release
- Channels: `alpha`, `beta`, stable (no suffix)
- Dev builds auto-bump the channel number (e.g., `alpha.1` → `alpha.2`) via `distro/build.sh`
- `distro/build.sh` must be updated to parse the new format: split on `-` first to separate `MAJOR.MINOR.PATCH` from `CHANNEL.N`, then increment `N`. For stable versions (no `-`), bump `PATCH` as before.
- Channel transitions (alpha → beta → stable) are manual edits to VERSION

### Initial version

Reset `VERSION` to `1.0.0-alpha.1` for the first public release.

### Version lifecycle example

```
1.0.0-alpha.1   ← manual set (starting work on 1.0.0)
1.0.0-alpha.2   ← dev build (auto-bump)
1.0.0-alpha.3   ← dev build
1.0.0-alpha.3   ← ./release.sh alpha (publishes)
1.0.0-alpha.4   ← dev build continues...

1.0.0-beta.1    ← manual set (promote to beta)
1.0.0-beta.2    ← dev build
1.0.0-beta.2    ← ./release.sh beta (publishes)

1.0.0           ← manual set (promote to stable)
1.0.0           ← ./release.sh (publishes stable)
1.0.1-alpha.1   ← auto-set by release.sh after stable publish
```

## 3. Release Script (`release.sh`)

### Usage

```bash
./release.sh alpha      # Publish alpha (VERSION must contain "alpha")
./release.sh beta       # Publish beta (VERSION must contain "beta")
./release.sh release    # Publish stable (VERSION must have no channel suffix)
./release.sh            # Publish stable (same as "release", fails if VERSION has alpha/beta)
```

### Safety gates

**Channel validation:**
- `./release.sh alpha` fails if VERSION doesn't contain "alpha"
- `./release.sh beta` fails if VERSION doesn't contain "beta"
- `./release.sh` (no arg) fails if VERSION contains "alpha" or "beta"
- Mismatch = hard fail with clear error message

**Secret scanning — the hard gate:**

Before building, the script scans the build tree for secrets. If ANY are found, the build aborts with a clear error listing what was found and where.

Scanned patterns:
- `.env` file with actual content (not `.env.example`)
- `autoconfig-*.json` files (not `*.sample.json`)
- `autoconfig.json` symlink
- Strings matching `sk-ant-` (Anthropic API keys — any version prefix)
- Strings matching `sk-` followed by 20+ alphanumeric chars (OpenAI keys)
- Any high-entropy string that looks like an API key (future-proofing)
- Any `vault.enc` files
- Any file containing `master_password` with a non-empty value

Scanned locations (operates on the **working tree**, not just git-tracked files):
- Repo root (`.env`, `autoconfig*.json`, `autoconfig.json` symlink)
- `distro/` build directory, including `distro/build/config/includes.chroot/` (files baked into ISO)
- Specifically check that `_inner_build.sh` does not copy any `autoconfig.json` with real API keys (the build script at line ~241 copies `autoconfig.json` into the ISO — release builds must not have this)

### Build process

1. Read `VERSION` file
2. Validate channel parameter matches VERSION
3. Run secret scan — abort if anything found
4. Save current VERSION before build
5. Build clean ISO via `distro/build.sh --no-bump` (Docker-based, isolated). `distro/build.sh` must accept a `--no-bump` flag that skips the auto-increment, so the release version matches exactly what was in VERSION.
6. Rename ISO to `aios-{VERSION}-amd64.iso` (ISO produced by live-build is typically `live-image-amd64.hybrid.iso`)
6. Generate SHA256 checksum file
7. Create git tag `v{VERSION}`
8. Push tag to GitHub
9. Create GitHub Release via `gh` CLI:
   - Alpha/beta: marked as "Pre-release"
   - Stable: marked as "Latest"
   - Attach ISO + checksum file
   - Auto-generate release notes from commits since last tag
10. After stable release: auto-set VERSION to next patch alpha (e.g., `1.0.0` → `1.0.1-alpha.1`)

### Prerequisites

- `gh` CLI authenticated (`gh auth login`)
- Docker running (for ISO build)
- Clean git working tree (no uncommitted changes)

### Architecture

amd64 (x86_64) only for now. ARM64 support is a future project.

## 4. README.md

### Structure

```markdown
# AiOS — The AI-Native Linux Distribution

[status badge] [license badge]

Brief: Humans interact through voice and text. The AI is the interface.

## What is AiOS?
2-3 sentences explaining the concept.

## Key Features
- Voice-first (wake word, STT, TTS — all local)
- Multi-channel (Desktop, Web, Signal)
- Built-in AI tool system (extensible)
- Local voice processing — all STT/TTS runs on your hardware
- VM compatible (QEMU, VirtualBox, VMware)
- Encrypted vault for API keys and secrets

## Quick Start
1. Download latest ISO from Releases
2. Boot in VM: qemu one-liner
3. Or write to USB: dd command

## Documentation
Link to aios.pages.dev/docs/

## Building from Source
Prerequisites + ./start.sh

## License
BSL 1.1 — free for personal, educational, and evaluation use.
Commercial use requires approval from swIT.work GmbH.
See LICENSE for full terms.

## Links
- Website: aios.pages.dev
- Releases: GitHub Releases
- Documentation: aios.pages.dev/docs/
```

**Rules:**
- No specific counts (tools, models, wake words) — these change frequently
- Links to full documentation for details
- VM compatibility highlighted as a first-class quick-start path

## 5. RELEASE.md

Human-readable checklist explaining the release process:

1. Verify VERSION file has the correct version and channel
2. Ensure all changes are committed and pushed
3. Run `./release.sh <channel>`
4. Verify the GitHub Release was created correctly
5. Verify the ISO download works
6. Deploy website if needed (`./deploy-website.sh`)

Includes troubleshooting section for common issues (gh not authenticated, Docker not running, secret scan failures).

## 6. Website (`website/`)

### Hosting

- **Platform:** Cloudflare Pages (free tier)
- **Domain:** `aios.pages.dev` (free subdomain, custom domain addable later)
- **Deploy:** via `wrangler` CLI from `deploy-website.sh`

### Pages

| Route | Content |
|-------|---------|
| `/` | Landing page — hero, features, screenshot, download CTA, VM quick-start |
| `/releases` | Release list — auto-fetched from GitHub API via client-side JS |
| `/docs/` | mdbook user guide (pre-built HTML copied from `docs/user-guide/book/`) |
| `/about` | Project vision, swIT.work GmbH info, license summary |

### Tech stack

- Pure static HTML/CSS/JS — no framework, no build system
- Dark theme matching AiOS aesthetic
- Responsive (mobile-friendly)
- Releases page: `fetch('https://api.github.com/repos/swit-work/ai-os/releases')` at page load
- No backend, no database, no server-side logic

### Design style

- Product/marketing focused (like elementary OS, Pop!_OS)
- Dark background, clean typography
- Hero section with tagline: "The AI Is The Interface"
- Feature cards with icons
- Screenshot/mockup of AiOS desktop
- Clear download button → latest stable release
- "Try in a VM" quick-start section
- Footer: swIT.work GmbH, license, links

### File structure

```
website/
├── index.html          # Landing page
├── releases.html       # Releases page (fetches GitHub API)
├── about.html          # About page
├── css/
│   └── style.css       # Shared styles
├── js/
│   └── releases.js     # GitHub API fetch + render
├── img/
│   ├── logo.svg        # AiOS logo
│   ├── screenshot.png  # Desktop screenshot (placeholder initially)
│   └── favicon.ico
└── docs/               # mdbook HTML output (copied by deploy script)
    └── (all mdbook files)
```

## 7. Deploy Script (`deploy-website.sh`)

### What it does

1. Rebuild mdbook docs: `mdbook build docs/user-guide`
2. Copy mdbook output to `website/docs/`: `cp -r docs/user-guide/book/* website/docs/`
3. Deploy to Cloudflare Pages: `wrangler pages deploy website/ --project-name=aios`

### Prerequisites

- `wrangler` CLI installed (`npm install -g wrangler`)
- Authenticated: `wrangler login` (one-time, opens browser)
- Cloudflare Pages project created: `wrangler pages project create aios` (one-time)
- `mdbook` installed (for docs rebuild)

### Usage

```bash
./deploy-website.sh             # Full deploy (rebuild docs + deploy)
./deploy-website.sh --no-docs   # Deploy website only (skip mdbook rebuild)
```

## 8. GitHub Organization

The repo should be transferred to a GitHub organization for the company:

- **Organization:** `swit-work` on GitHub (to be created by the user)
- **Repo:** `swit-work/ai-os`
- **Transfer:** done manually via GitHub Settings → Transfer repository

All scripts and website references use `swit-work/ai-os` as the repo path. The `release.sh` script creates GitHub Releases on this repo.

This is a manual step done before the first public release — not automated.

**Repo URL migration:** The current codebase references `AiOS-Project/ai-os` and `vchira/ai-os` in multiple files. All references must be updated to `swit-work/ai-os`:
- `docs/user-guide/book.toml`
- `docs/user-guide/src/license.md`
- `docs/user-guide/src/support.md`
- `docs/user-guide/src/getting-started/download.md`
- Any other markdown files referencing the old GitHub URLs

## 9. Deliverables Summary

| File | Type | Description |
|------|------|-------------|
| `LICENSE` | Create | BSL 1.1 full text |
| `README.md` | Create | Project overview for GitHub |
| `RELEASE.md` | Create | Release process checklist |
| `release.sh` | Create | Build + secret scan + tag + publish to GitHub Releases |
| `deploy-website.sh` | Create | Deploy website to Cloudflare Pages |
| `website/index.html` | Create | Landing page |
| `website/releases.html` | Create | Releases page (GitHub API) |
| `website/about.html` | Create | About page |
| `website/css/style.css` | Create | Shared stylesheet |
| `website/js/releases.js` | Create | GitHub Releases fetch + render |
| `website/img/` | Create | Logo, screenshot, favicon |
| `VERSION` | Modify | Reset to `1.0.0-alpha.1` |
| `Cargo.toml` | Modify | License field → `license-file = "LICENSE"` |
| `distro/build.sh` | Modify | Add `--no-bump` flag, update version parsing for channel suffix format |
| `.gitignore` | Modify | Add `website/docs/` (generated mdbook output) |
| `CLAUDE.md` | Modify | Add `release.sh` and `deploy-website.sh` to build commands and key files |
| `docs/user-guide/src/license.md` | Modify | Update license info to BSL 1.1 |
| `docs/user-guide/book.toml` | Modify | Update repo URL to swit-work/ai-os |
| `docs/user-guide/src/support.md` | Modify | Update repo URL to swit-work/ai-os |
| `docs/user-guide/src/getting-started/download.md` | Modify | Update repo URL to swit-work/ai-os |
