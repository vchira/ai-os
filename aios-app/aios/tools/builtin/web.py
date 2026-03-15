"""Web tools -- fetch URLs, search the web, and download files."""

from __future__ import annotations

import json
import logging
import os
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import Request, urlopen

from aios.tools.base import Tool, ToolResult

logger = logging.getLogger(__name__)

# Maximum response body to keep in memory (bytes).
_MAX_FETCH_BYTES = 2_000_000  # ~2 MB

# Directory for downloaded files.
_DOWNLOAD_DIR = Path.home() / "Downloads"

# Default timeout for HTTP requests (seconds).
_HTTP_TIMEOUT = 30

# User-Agent header to identify AiOS requests.
_USER_AGENT = "AiOS/1.0 (WebTool)"


class WebTool(Tool):
    """Fetch URLs, search the web, and download files."""

    @property
    def name(self) -> str:
        return "web"

    @property
    def description(self) -> str:
        return (
            "Web utilities: fetch a URL (returns text content), search the web, "
            "or download a file to a local path."
        )

    @property
    def parameters(self) -> dict:
        return {
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["fetch_url", "search_web", "download_file"],
                    "description": "Web action to perform.",
                },
                "url": {
                    "type": "string",
                    "description": "URL to fetch or download.",
                },
                "query": {
                    "type": "string",
                    "description": "Search query (for search_web).",
                },
                "destination": {
                    "type": "string",
                    "description": "Local file path for download_file (defaults to ~/Downloads/<filename>).",
                },
            },
            "required": ["action"],
        }

    def execute(self, **kwargs: Any) -> ToolResult:
        action: str = kwargs.get("action", "")

        if action == "fetch_url":
            return self._fetch_url(kwargs.get("url", ""))
        if action == "search_web":
            return self._search_web(kwargs.get("query", ""))
        if action == "download_file":
            return self._download_file(
                kwargs.get("url", ""),
                kwargs.get("destination", ""),
            )

        return ToolResult.fail(
            f"Unknown action {action!r}. Use: fetch_url, search_web, download_file."
        )

    # -- Actions ----------------------------------------------------------------

    @staticmethod
    def _fetch_url(url: str) -> ToolResult:
        if not url:
            return ToolResult.fail("'url' is required for fetch_url.")

        req = Request(url, headers={"User-Agent": _USER_AGENT})
        try:
            with urlopen(req, timeout=_HTTP_TIMEOUT) as resp:
                content_type = resp.headers.get("Content-Type", "")
                body = resp.read(_MAX_FETCH_BYTES)

                # Try decoding as text.
                charset = "utf-8"
                if "charset=" in content_type:
                    charset = content_type.split("charset=")[-1].split(";")[0].strip()
                try:
                    text = body.decode(charset, errors="replace")
                except (LookupError, UnicodeDecodeError):
                    text = body.decode("utf-8", errors="replace")

                return ToolResult.ok(
                    text,
                    data={
                        "url": url,
                        "status": resp.status,
                        "content_type": content_type,
                        "length": len(body),
                    },
                )
        except HTTPError as exc:
            return ToolResult.fail(f"HTTP error {exc.code}: {exc.reason}")
        except URLError as exc:
            return ToolResult.fail(f"Could not reach URL: {exc.reason}")
        except Exception as exc:
            return ToolResult.fail(f"Fetch failed: {exc}")

    @staticmethod
    def _search_web(query: str) -> ToolResult:
        """Perform a web search using a search API.

        This implementation uses the DuckDuckGo Instant Answer API (which
        requires no API key).  It can be swapped for Google/Bing/SearXNG by
        changing the URL and parsing logic.
        """
        if not query:
            return ToolResult.fail("'query' is required for search_web.")

        params = urlencode({"q": query, "format": "json", "no_html": "1"})
        url = f"https://api.duckduckgo.com/?{params}"
        req = Request(url, headers={"User-Agent": _USER_AGENT})

        try:
            with urlopen(req, timeout=_HTTP_TIMEOUT) as resp:
                data = json.loads(resp.read())

            results: list[dict[str, str]] = []

            # Abstract (main answer).
            if data.get("Abstract"):
                results.append({
                    "title": data.get("Heading", ""),
                    "snippet": data["Abstract"],
                    "url": data.get("AbstractURL", ""),
                })

            # Related topics.
            for topic in data.get("RelatedTopics", []):
                if "Text" in topic:
                    results.append({
                        "title": topic.get("FirstURL", "").split("/")[-1].replace("_", " "),
                        "snippet": topic["Text"],
                        "url": topic.get("FirstURL", ""),
                    })
                # Sub-topics (grouped).
                for sub in topic.get("Topics", []):
                    if "Text" in sub:
                        results.append({
                            "title": sub.get("FirstURL", "").split("/")[-1].replace("_", " "),
                            "snippet": sub["Text"],
                            "url": sub.get("FirstURL", ""),
                        })

            if not results:
                return ToolResult.ok(
                    "No results found.",
                    data={"query": query, "results": []},
                )

            lines = []
            for i, r in enumerate(results[:10], 1):
                lines.append(f"{i}. {r['title']}")
                lines.append(f"   {r['snippet'][:200]}")
                if r["url"]:
                    lines.append(f"   {r['url']}")
                lines.append("")

            return ToolResult.ok(
                "\n".join(lines).strip(),
                data={"query": query, "results": results[:10]},
            )
        except HTTPError as exc:
            return ToolResult.fail(f"Search API error {exc.code}: {exc.reason}")
        except URLError as exc:
            return ToolResult.fail(f"Could not reach search API: {exc.reason}")
        except Exception as exc:
            return ToolResult.fail(f"Search failed: {exc}")

    @staticmethod
    def _download_file(url: str, destination: str) -> ToolResult:
        if not url:
            return ToolResult.fail("'url' is required for download_file.")

        # Determine destination path.
        if destination:
            dest_path = Path(destination).expanduser().resolve()
        else:
            filename = url.rstrip("/").split("/")[-1].split("?")[0] or "download"
            _DOWNLOAD_DIR.mkdir(parents=True, exist_ok=True)
            dest_path = _DOWNLOAD_DIR / filename

        # Do not overwrite without the user knowing.
        if dest_path.exists():
            # Append a numeric suffix.
            stem = dest_path.stem
            suffix = dest_path.suffix
            counter = 1
            while dest_path.exists():
                dest_path = dest_path.with_name(f"{stem}_{counter}{suffix}")
                counter += 1

        req = Request(url, headers={"User-Agent": _USER_AGENT})
        try:
            dest_path.parent.mkdir(parents=True, exist_ok=True)
            with urlopen(req, timeout=120) as resp:
                with open(dest_path, "wb") as fout:
                    total = 0
                    while True:
                        chunk = resp.read(65536)
                        if not chunk:
                            break
                        fout.write(chunk)
                        total += len(chunk)

            return ToolResult.ok(
                f"Downloaded {total} bytes to {dest_path}",
                data={
                    "url": url,
                    "path": str(dest_path),
                    "bytes_downloaded": total,
                },
            )
        except HTTPError as exc:
            return ToolResult.fail(f"Download HTTP error {exc.code}: {exc.reason}")
        except URLError as exc:
            return ToolResult.fail(f"Could not reach URL: {exc.reason}")
        except Exception as exc:
            return ToolResult.fail(f"Download failed: {exc}")
