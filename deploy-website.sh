#!/bin/bash
set -euo pipefail

REPO_DIR="$(cd "$(dirname "$0")" && pwd)"
WEBSITE_DIR="${REPO_DIR}/website"
DOCS_SRC="${REPO_DIR}/docs/user-guide"
DOCS_DEST="${WEBSITE_DIR}/docs"

if ! command -v wrangler &>/dev/null; then
    echo "ERROR: 'wrangler' CLI not found."
    echo "Install: npm install -g wrangler"
    echo "Login:   wrangler login"
    exit 1
fi

if [[ "${1:-}" != "--no-docs" ]]; then
    if command -v mdbook &>/dev/null; then
        echo "Building documentation..."
        (cd "${DOCS_SRC}" && mdbook build)
        echo "Copying docs to website..."
        rm -rf "${DOCS_DEST}"
        mkdir -p "${DOCS_DEST}"
        cp -r "${DOCS_SRC}/book/"* "${DOCS_DEST}/"
        echo "Documentation built and copied."
    else
        echo "WARNING: 'mdbook' not found — skipping docs rebuild."
        echo "Install: cargo install mdbook"
    fi
fi

echo "Deploying to Cloudflare Pages..."
wrangler pages deploy "${WEBSITE_DIR}" --project-name=aios

echo ""
echo "Deployed! Site should be live at https://aios.pages.dev"
