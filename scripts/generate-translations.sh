#!/bin/bash
# Generate translation files from English source using the Claude API.
# Usage: ./scripts/generate-translations.sh [language_code]
# If no language specified, generates all supported languages.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
SOURCE="${PROJECT_DIR}/aios-app-rs/aios-core/i18n/en.json"
OUTPUT_DIR="${PROJECT_DIR}/aios-app-rs/aios-core/i18n"

LANGUAGES=(de fr es it pt ro nl pl cs hu sv no da fi el tr ar hi ja zh ko ru uk id)

# Read API key
if [ -z "${CLAUDE_API_KEY:-}" ]; then
    CLAUDE_API_KEY=$(grep -oP 'CLAUDE_API_KEY\s*=\s*\K\S+' "${PROJECT_DIR}/.env" 2>/dev/null || true)
fi
if [ -z "$CLAUDE_API_KEY" ]; then
    echo "Error: Set CLAUDE_API_KEY in .env or environment"
    exit 1
fi

declare -A LANG_NAMES=(
    [de]="German" [fr]="French" [es]="Spanish" [it]="Italian"
    [pt]="Portuguese" [ro]="Romanian" [nl]="Dutch" [pl]="Polish"
    [cs]="Czech" [hu]="Hungarian" [sv]="Swedish" [no]="Norwegian"
    [da]="Danish" [fi]="Finnish" [el]="Greek" [tr]="Turkish"
    [ar]="Arabic" [hi]="Hindi" [ja]="Japanese" [zh]="Chinese Simplified"
    [ko]="Korean" [ru]="Russian" [uk]="Ukrainian" [id]="Indonesian"
)

generate_language() {
    local lang=$1
    local lang_name=${LANG_NAMES[$lang]}
    echo "Generating ${lang_name} (${lang})..."

    local direction="ltr"
    if [ "$lang" = "ar" ]; then direction="rtl"; fi

    local prompt="Translate all string values in this JSON to ${lang_name}. Rules:
- Keep all JSON keys exactly as they are (do not translate keys)
- Only translate the string values
- Update the _meta block: set language to \"${lang_name}\", code to \"${lang}\", direction to \"${direction}\"
- Keep {placeholder} variables unchanged (e.g., {provider}, {name}, {size})
- Keep emoji/unicode symbols unchanged
- Keep URLs unchanged
- Keep technical terms like API, SSH, TTS, STT, LLM, AiOS unchanged
- Return ONLY valid JSON — no markdown code fences, no explanation, no backticks"

    local response
    response=$(curl -s https://api.anthropic.com/v1/messages \
        -H "x-api-key: ${CLAUDE_API_KEY}" \
        -H "anthropic-version: 2023-06-01" \
        -H "content-type: application/json" \
        -d "$(jq -n --arg prompt "$prompt" --rawfile source "$SOURCE" '{
            model: "claude-sonnet-4-20250514",
            max_tokens: 16384,
            messages: [{role: "user", content: ($prompt + "\n\n" + $source)}]
        }')")

    # Check for API errors
    local error
    error=$(echo "$response" | jq -r '.error.message // empty' 2>/dev/null)
    if [ -n "$error" ]; then
        echo "  ERROR: API returned: ${error}"
        return 1
    fi

    # Extract the text content
    local text
    text=$(echo "$response" | jq -r '.content[0].text')

    # Strip markdown code fences if present (```json ... ``` or ``` ... ```)
    text=$(echo "$text" | sed '/^```\(json\)\?$/d')

    echo "$text" > "${OUTPUT_DIR}/${lang}.json"

    # Validate JSON
    if ! jq empty "${OUTPUT_DIR}/${lang}.json" 2>/dev/null; then
        echo "  WARNING: Invalid JSON for ${lang}, removing file"
        rm -f "${OUTPUT_DIR}/${lang}.json"
        return 1
    fi

    # Pretty-print to normalize formatting
    local tmp="${OUTPUT_DIR}/${lang}.json.tmp"
    jq '.' "${OUTPUT_DIR}/${lang}.json" > "$tmp" && mv "$tmp" "${OUTPUT_DIR}/${lang}.json"

    echo "  -> ${OUTPUT_DIR}/${lang}.json ($(wc -l < "${OUTPUT_DIR}/${lang}.json") lines)"
}

if [ -n "${1:-}" ]; then
    if [ -z "${LANG_NAMES[$1]+x}" ]; then
        echo "Error: Unknown language code '$1'"
        echo "Supported: ${LANGUAGES[*]}"
        exit 1
    fi
    generate_language "$1"
else
    for lang in "${LANGUAGES[@]}"; do
        generate_language "$lang" || true
        sleep 1  # Rate limiting
    done
fi

echo "Done!"
