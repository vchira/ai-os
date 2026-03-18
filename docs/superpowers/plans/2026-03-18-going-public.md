# Going Public Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prepare AiOS for public release — license, release pipeline with secret scanning, README, project website, and deploy script.

**Architecture:** BSL 1.1 license for the code, `release.sh` builds clean ISOs with mandatory secret scanning and publishes to GitHub Releases, static website on Cloudflare Pages auto-fetches releases from GitHub API, `deploy-website.sh` pushes the site via wrangler CLI.

**Tech Stack:** Bash (scripts), HTML/CSS/JS (website), GitHub Releases API, Cloudflare Pages, wrangler CLI, gh CLI

**Spec:** `docs/superpowers/specs/2026-03-18-going-public-design.md`

---

## File Map

### New Files
| File | Responsibility |
|------|---------------|
| `LICENSE` | BSL 1.1 full text |
| `README.md` | Project overview for GitHub |
| `RELEASE.md` | Release process checklist |
| `release.sh` | Secret scan + build + tag + publish to GitHub Releases |
| `deploy-website.sh` | Deploy website to Cloudflare Pages |
| `website/index.html` | Landing page |
| `website/releases.html` | Releases page (GitHub API) |
| `website/about.html` | About page |
| `website/css/style.css` | Shared dark-theme stylesheet |
| `website/js/releases.js` | Fetch + render GitHub releases |
| `website/img/logo.svg` | AiOS logo |
| `website/img/favicon.ico` | Favicon |

### Modified Files
| File | What Changes |
|------|-------------|
| `VERSION` | Reset to `1.0.0-alpha.1` |
| `aios-app-rs/Cargo.toml:16` | `license = "MIT"` to `license-file = "LICENSE"` |
| `distro/build.sh:31-52` | Add `--no-bump` flag, update version parsing for channel suffix |
| `.gitignore` | Add `website/docs/` |
| `CLAUDE.md` | Add release.sh and deploy-website.sh |
| `docs/user-guide/book.toml:14` | Update repo URL |
| `docs/user-guide/src/license.md` | Rewrite for BSL 1.1 |
| `docs/user-guide/src/support.md:38,60,66,67` | Update GitHub URLs |
| `docs/user-guide/src/getting-started/download.md:7,54` | Update GitHub URLs |

---

## Task 1: LICENSE File (BSL 1.1)

**Files:**
- Create: `LICENSE`
- Modify: `aios-app-rs/Cargo.toml:16`

- [ ] **Step 1: Create the LICENSE file**

Create `/home/vchira/work/ai-os/LICENSE` with the full BSL 1.1 text. Use the official template from https://mariadb.com/bsl11/ with these parameters:

- **Licensor:** swIT.work GmbH
- **Licensed Work:** AiOS (The AI-Native Linux Distribution)
- **Additional Use Grant:** You may use the Licensed Work for personal, educational, internal business, and evaluation purposes without requiring approval from the Licensor. You may not use the Licensed Work for commercial distribution, creating derivative Linux distributions, offering it as a hosted or managed service, or bundling it with commercial hardware without written approval from swIT.work GmbH.
- **Change Date:** Five years from the date of each release of the Licensed Work
- **Change License:** Apache License, Version 2.0

The full BSL 1.1 text with parameters filled in.

- [ ] **Step 2: Update Cargo.toml license field**

In `/home/vchira/work/ai-os/aios-app-rs/Cargo.toml`, line 16, change:
```toml
license = "MIT"
```
to:
```toml
license-file = "LICENSE"
```

- [ ] **Step 3: Verify Cargo.toml is valid**

Run: `cd /home/vchira/work/ai-os/aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo check --workspace 2>&1 | tail -3`
Expected: compiles (Cargo accepts `license-file`)

- [ ] **Step 4: Commit**

```bash
git add LICENSE aios-app-rs/Cargo.toml
git commit -m "feat: add BSL 1.1 license — swIT.work GmbH, 5yr change date"
```

---

## Task 2: Update Version and Build System

**Files:**
- Modify: `VERSION`
- Modify: `distro/build.sh:31-52`

- [ ] **Step 1: Reset VERSION to 1.0.0-alpha.1**

```bash
echo "1.0.0-alpha.1" > /home/vchira/work/ai-os/VERSION
```

- [ ] **Step 2: Update distro/build.sh version parsing**

Read `/home/vchira/work/ai-os/distro/build.sh`. Replace the version parsing block (lines ~31-52) with logic that handles the new `MAJOR.MINOR.PATCH-CHANNEL.N` format. The new logic must:

1. Split on `-` first to separate base version from channel suffix
2. Parse `MAJOR.MINOR.PATCH` from the base
3. Handle `--bump-major` and `--bump-minor` (reset channel suffix)
4. Handle `--no-bump` (skip all incrementing)
5. For pre-release versions (has channel suffix): increment the channel number (e.g., `alpha.1` to `alpha.2`)
6. For stable versions (no suffix): increment patch as before
7. Write the result back to VERSION

- [ ] **Step 3: Test version parsing**

Manually test each scenario:
- `echo "1.0.0-alpha.1" > VERSION` then build without --no-bump: should become `1.0.0-alpha.2`
- `echo "1.0.0-alpha.3" > VERSION` with `--no-bump`: should stay `1.0.0-alpha.3`
- `echo "1.0.0" > VERSION` then build: should become `1.0.1`
- `echo "1.0.0-beta.5" > VERSION` then build: should become `1.0.0-beta.6`

Reset VERSION after testing: `echo "1.0.0-alpha.1" > VERSION`

- [ ] **Step 4: Commit**

```bash
git add VERSION distro/build.sh
git commit -m "feat: update versioning — 1.0.0-alpha.1, channel support, --no-bump flag"
```

---

## Task 3: Release Script

**Files:**
- Create: `release.sh`
- Create: `RELEASE.md`

- [ ] **Step 1: Create release.sh**

Create `/home/vchira/work/ai-os/release.sh` (make it executable with `chmod +x`).

The script must implement:

1. **Read VERSION** and parse channel (alpha/beta/stable)
2. **Channel validation** — match parameter to VERSION content:
   - `./release.sh alpha` fails if VERSION doesn't contain "alpha"
   - `./release.sh beta` fails if VERSION doesn't contain "beta"
   - `./release.sh` (no arg) fails if VERSION has alpha/beta
   - `./release.sh release` works same as no arg
3. **Prerequisites check** — verify `gh`, `docker`, `gh auth status`, clean git tree
4. **Secret scan (HARD GATE)** — scan the working tree (not just git-tracked):
   - `.env` with real content
   - `autoconfig-*.json` (not `*.sample.json`)
   - `autoconfig.json` symlink
   - `sk-ant-` pattern (Anthropic keys)
   - `sk-` followed by 20+ alphanumeric chars (OpenAI keys)
   - `vault.enc` files
   - `master_password` with non-empty values
   - Scan: repo root, `distro/`, `distro/build/config/includes.chroot/`
   - If ANY found: abort with clear listing of what and where
5. **Build** — `distro/build.sh --no-bump`
6. **Rename ISO** — `aios-{VERSION}-amd64.iso`
7. **Generate SHA256** checksum
8. **Git tag** — `v{VERSION}`, push
9. **GitHub Release** — via `gh release create`, pre-release flag for alpha/beta, attach ISO + checksum
10. **Post-release** (stable only) — bump VERSION to next patch alpha, commit + push

- [ ] **Step 2: Create RELEASE.md**

Create `/home/vchira/work/ai-os/RELEASE.md` — human-readable release process documentation:
- Quick reference (the 4 commands)
- Prerequisites checklist
- Step-by-step release checklist
- Version lifecycle diagram
- Troubleshooting (gh auth, dirty git, secret scan failures, Docker not running)

- [ ] **Step 3: Test release.sh validation (no actual build)**

```bash
# Channel validation tests:
echo "1.0.0-alpha.1" > VERSION
./release.sh 2>&1 | head -5         # Should FAIL (alpha version, no channel)
./release.sh beta 2>&1 | head -5    # Should FAIL (version is alpha, not beta)
./release.sh alpha 2>&1 | head -10  # Should pass validation, fail at build step (no Docker/etc)
```

- [ ] **Step 4: Commit**

```bash
git add release.sh RELEASE.md
git commit -m "feat: add release.sh — secret scan + build + GitHub Release publishing"
```

---

## Task 4: README.md

**Files:**
- Create: `README.md`

- [ ] **Step 1: Create README.md**

Create `/home/vchira/work/ai-os/README.md` following the spec structure:

- Title: `# AiOS — The AI-Native Linux Distribution`
- Shields.io badges: license (BSL-1.1), status (alpha)
- What is AiOS: 2-3 sentences (AI is the primary interface, voice + text, tools)
- Key Features: 6 bullets (no specific counts):
  - Voice-first interaction (wake word, STT, TTS — all local)
  - Multi-channel (Desktop, Web, Signal)
  - Built-in AI tool system (extensible via plugins)
  - Local voice processing — all STT/TTS runs on your hardware
  - VM compatible (QEMU, VirtualBox, VMware)
  - Encrypted vault for API keys and secrets
- Quick Start: download link, QEMU one-liner, USB dd
- Documentation: link to aios.pages.dev/docs/
- Building from Source: prerequisites + `./start.sh`
- License: BSL 1.1 summary, link to LICENSE
- Links: website, releases, docs

NO hardcoded version numbers. Use shields.io dynamic badges.

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "feat: add README.md — project overview for public GitHub"
```

---

## Task 5: Update Docs and URLs

**Files:**
- Modify: `docs/user-guide/book.toml:14`
- Modify: `docs/user-guide/src/license.md`
- Modify: `docs/user-guide/src/support.md:38,60,66,67`
- Modify: `docs/user-guide/src/getting-started/download.md:7,54`
- Modify: `.gitignore`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update book.toml**

Line 14: change `AiOS-Project/ai-os` to `swit-work/ai-os`

- [ ] **Step 2: Rewrite license.md**

Replace content with BSL 1.1 information:
- Plain-language explanation of BSL 1.1
- What's allowed (personal, educational, internal, evaluation)
- What needs approval (commercial, derivatives, hosting, hardware)
- 5-year change date to Apache 2.0
- Link to LICENSE file on GitHub
- Keep third-party components table

- [ ] **Step 3: Update support.md URLs**

Replace all 4 instances of `AiOS-Project/ai-os` with `swit-work/ai-os`

- [ ] **Step 4: Update download.md URLs**

Replace both instances of `AiOS-Project/ai-os` with `swit-work/ai-os`

- [ ] **Step 5: Search for remaining old URLs**

```bash
grep -r "AiOS-Project/ai-os\|vchira/ai-os" docs/ --include="*.md" --include="*.toml" --include="*.html"
```

Fix any remaining references.

- [ ] **Step 6: Add website/docs/ to .gitignore**

Add at the end of `.gitignore`:
```
# Website generated docs
website/docs/
```

- [ ] **Step 7: Update CLAUDE.md**

Add `release.sh` and `deploy-website.sh` to the Build section and Key Files section.

- [ ] **Step 8: Commit**

```bash
git add docs/ .gitignore CLAUDE.md
git commit -m "feat: update docs — BSL 1.1 license, swit-work URLs, release scripts"
```

---

## Task 6: Website — Landing Page

**Files:**
- Create: `website/index.html`
- Create: `website/css/style.css`
- Create: `website/img/logo.svg`
- Create: `website/img/favicon.ico`

- [ ] **Step 1: Create directory structure**

```bash
mkdir -p website/css website/js website/img
```

- [ ] **Step 2: Create style.css**

Dark-theme, responsive stylesheet:
- Background: `#0d1117`, text: `#e6edf3`, accent: `#4a9eff`
- System fonts, clean typography
- Mobile-first responsive layout, max-width ~1200px
- Components: nav bar, hero section, feature cards (grid), footer
- No external dependencies

- [ ] **Step 3: Create logo.svg**

Simple text-based SVG: "AiOS" with "Ai" in accent color. Placeholder — can be replaced later.

- [ ] **Step 4: Create favicon**

Minimal favicon. Can use an SVG favicon reference in HTML.

- [ ] **Step 5: Create index.html**

Self-contained HTML page:
- Meta tags, Open Graph, favicon
- Nav: logo, Releases, Docs, About, GitHub links
- Hero: title, "The AI Is The Interface" tagline, description, Download + VM buttons
- Features: 6 cards (no counts)
- Quick Start: download, QEMU command, USB command
- Footer: swIT.work GmbH copyright, BSL 1.1, links

No CDN dependencies, no external frameworks.

- [ ] **Step 6: Test locally**

```bash
cd website && python3 -m http.server 8080
# Open http://localhost:8080
```

- [ ] **Step 7: Commit**

```bash
git add website/
git commit -m "feat: add website landing page — dark theme, features, quick start"
```

---

## Task 7: Website — Releases Page

**Files:**
- Create: `website/releases.html`
- Create: `website/js/releases.js`

- [ ] **Step 1: Create releases.js**

JavaScript that fetches `https://api.github.com/repos/swit-work/ai-os/releases` and renders release cards. Each card shows: version, date, pre-release badge, release notes, download links with file sizes.

Handle errors gracefully: show "No releases yet" or link to GitHub Releases as fallback.

For markdown rendering in release notes: either use plain text (strip markdown) or include a local copy of a minimal markdown parser. No CDN dependencies.

**Security note:** Use safe DOM methods (`document.createElement`, `textContent`) instead of string interpolation into HTML to prevent XSS from release note content. Or sanitize with a library like DOMPurify (local copy).

- [ ] **Step 2: Create releases.html**

Same nav/footer as index.html. Content: title, description, `<div id="releases-list">` container, fallback link to GitHub.

- [ ] **Step 3: Commit**

```bash
git add website/releases.html website/js/
git commit -m "feat: add releases page — auto-fetches from GitHub Releases API"
```

---

## Task 8: Website — About Page

**Files:**
- Create: `website/about.html`

- [ ] **Step 1: Create about.html**

Same nav/footer. Content:
- About AiOS — vision, AI as primary interface
- How it works — Debian, Wayland, GTK4, Rust
- License — BSL 1.1 summary
- swIT.work GmbH — Austrian company
- Contributing — GitHub Issues/PRs welcome

- [ ] **Step 2: Commit**

```bash
git add website/about.html
git commit -m "feat: add about page — vision, license, company info"
```

---

## Task 9: Deploy Script

**Files:**
- Create: `deploy-website.sh`

- [ ] **Step 1: Create deploy-website.sh (executable)**

The script:
1. Check `wrangler` is installed
2. If not `--no-docs`: check `mdbook`, run `mdbook build docs/user-guide`, copy `book/*` to `website/docs/`
3. Run `wrangler pages deploy website/ --project-name=aios`

Usage: `./deploy-website.sh` or `./deploy-website.sh --no-docs`

- [ ] **Step 2: Commit**

```bash
git add deploy-website.sh
git commit -m "feat: add deploy-website.sh — Cloudflare Pages deployment"
```

---

## Task 10: Final Verification

- [ ] **Step 1: Verify all new files exist**

```bash
ls -la LICENSE README.md RELEASE.md release.sh deploy-website.sh
ls -la website/index.html website/releases.html website/about.html
ls -la website/css/style.css website/js/releases.js website/img/
```

- [ ] **Step 2: Run workspace tests**

```bash
cd /home/vchira/work/ai-os/aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-target cargo test --workspace 2>&1 | grep "test result"
```

All must pass.

- [ ] **Step 3: Test release.sh channel validation**

```bash
cat VERSION  # Should be 1.0.0-alpha.1
./release.sh 2>&1 | head -5          # Should FAIL
./release.sh beta 2>&1 | head -5     # Should FAIL
./release.sh alpha 2>&1 | head -10   # Should pass validation
```

- [ ] **Step 4: Test website locally**

```bash
cd website && python3 -m http.server 8080 &
# Browse http://localhost:8080 — check all 3 pages
kill %1
```

- [ ] **Step 5: Review git log**

```bash
git log --oneline -10
```

Should show ~9 clean commits covering license, versioning, release script, README, docs, website (landing, releases, about), and deploy script.
