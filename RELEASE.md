# AiOS Release Process

## Quick Reference

```bash
./release.sh alpha      # Publish alpha pre-release
./release.sh beta       # Publish beta pre-release
./release.sh release    # Publish stable release
./release.sh            # Publish stable release (same as "release")
```

The script validates that the `VERSION` file matches the requested channel.
It will refuse to proceed on mismatch, secret detection, or dirty git state.

## Prerequisites

Before running `release.sh`, ensure:

- [ ] `gh` CLI installed and authenticated (`gh auth login`)
- [ ] Docker installed and running
- [ ] Git working tree is clean (no uncommitted changes)
- [ ] `VERSION` file contains the correct version for this release
- [ ] No `.env` files with real API keys in the repo
- [ ] No `autoconfig-*.json` files (only `*.sample.json` allowed)
- [ ] No `vault.enc` files anywhere in the repo

## Step-by-Step Release Checklist

### 1. Prepare the version

Edit `VERSION` to the target version:

```bash
# For alpha releases (during development):
echo "1.0.0-alpha.3" > VERSION

# For beta releases (feature-complete, testing):
echo "1.0.0-beta.1" > VERSION

# For stable releases (production-ready):
echo "1.0.0" > VERSION
```

Commit the version change:

```bash
git add VERSION
git commit -m "chore: set VERSION to $(cat VERSION) for release"
git push
```

### 2. Verify clean state

```bash
git status          # Must be clean
gh auth status      # Must be authenticated
docker info         # Must be running
```

### 3. Run the release

```bash
./release.sh alpha    # or beta, or release
```

The script performs these steps automatically:

1. Validates the channel matches `VERSION`
2. Checks prerequisites (gh, docker, clean git)
3. Scans for secrets (API keys, vault files, credentials)
4. Builds the ISO via `distro/build.sh --no-bump`
5. Renames ISO and generates SHA256 checksum
6. Creates an annotated git tag `v{VERSION}`
7. Pushes tag and creates GitHub Release (with ISO + checksum attached)
8. For stable releases: bumps VERSION to next patch alpha and pushes

### 4. Verify the release

- Check the GitHub Releases page for the new release
- Verify the ISO download link works
- Verify the checksum: `sha256sum -c aios-{VERSION}-amd64.iso.sha256`

## Version Lifecycle

```
Development          Alpha Releases         Beta Releases       Stable Release
-----------          --------------         -------------       --------------

1.0.0-alpha.1  ──┐
1.0.0-alpha.2    │  ./release.sh alpha
1.0.0-alpha.3  ──┤  → publishes alpha.3
1.0.0-alpha.4    │  (pre-release on GitHub)
...              │
                 │
  [manual edit]  │
  VERSION =      │
  1.0.0-beta.1 ─┘─┐
1.0.0-beta.2      │  ./release.sh beta
1.0.0-beta.2    ──┤  → publishes beta.2
...               │  (pre-release on GitHub)
                  │
  [manual edit]   │
  VERSION =       │
  1.0.0         ──┘── ./release.sh release
                       → publishes 1.0.0
                       → tagged as "Latest"
                       → auto-bumps VERSION
                         to 1.0.1-alpha.1
                       → next cycle begins
```

### Channel transitions

- **alpha -> beta**: Manually edit `VERSION` from `X.Y.Z-alpha.N` to `X.Y.Z-beta.1`, commit, push.
- **beta -> stable**: Manually edit `VERSION` from `X.Y.Z-beta.N` to `X.Y.Z`, commit, push.
- **stable -> next alpha**: Automatic. After `./release.sh release`, the script bumps `VERSION` to `X.Y.(Z+1)-alpha.1`.

## Troubleshooting

### gh CLI not authenticated

```
ERROR: Prerequisites check failed.
  ✗ gh CLI not authenticated. Run: gh auth login
```

Fix: Run `gh auth login` and follow the prompts. You need write access to the repository.

### Dirty git working tree

```
ERROR: Prerequisites check failed.
  ✗ Git working tree is dirty. Commit or stash changes first.
```

Fix: Commit or stash your changes before releasing:

```bash
git add -A && git commit -m "chore: pre-release cleanup"
# or
git stash
```

### Secret scan failed

```
SECRET SCAN FAILED — release aborted.
```

The script found sensitive data in the working tree. Common causes:

- **`.env` with API keys**: Remove it or ensure it only has comments/empty values.
- **`autoconfig-*.json`** (non-sample): Rename to `*.sample.json` or delete.
- **`autoconfig.json` symlink**: Remove it (`rm autoconfig.json`).
- **`vault.enc`**: Remove any vault files from the repo tree.
- **API key patterns in source**: Search for `sk-ant-` or `sk-` strings in `distro/`.

This gate is non-negotiable. Every item must be resolved before releasing.

### Docker not available

```
ERROR: Prerequisites check failed.
  ✗ docker not found.
```

Fix: Install Docker and ensure the daemon is running:

```bash
sudo systemctl start docker
```

### Tag already exists

```
ERROR: Tag 'v1.0.0-alpha.3' already exists.
```

This means a release with this version was already published. Either:

- Bump the version in `VERSION` and commit, or
- Delete the existing tag if it was created in error: `git tag -d v1.0.0-alpha.3 && git push origin :refs/tags/v1.0.0-alpha.3`

### Channel mismatch

```
ERROR: Channel mismatch: requested 'beta' but VERSION is '1.0.0-alpha.3' (alpha).
```

The channel you passed to `release.sh` doesn't match the `VERSION` file. Either:

- Use the correct channel: `./release.sh alpha`
- Or edit `VERSION` to match: `echo "1.0.0-beta.1" > VERSION` then commit and push.
