"""Client for the AiOS online plugin store.

The store is a simple REST API that lets users search for, install, update, and
remove third-party tool plugins.  Plugins are downloaded as tarballs, extracted
into ``~/.aios/plugins/<name>/``, and loaded by the :class:`ToolRegistry`.
"""

from __future__ import annotations

import io
import json
import logging
import shutil
import tarfile
import zipfile
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Optional
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

logger = logging.getLogger(__name__)

STORE_URL: str = "https://store.aios.dev/api/v1"

_PLUGINS_DIR = Path.home() / ".aios" / "plugins"
_MANIFEST_NAME = "manifest.json"


@dataclass
class PluginInfo:
    """Metadata for a plugin, either installed locally or available in the store."""

    name: str
    version: str
    description: str
    author: str
    url: str

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PluginInfo:
        return cls(
            name=data.get("name", ""),
            version=data.get("version", "0.0.0"),
            description=data.get("description", ""),
            author=data.get("author", ""),
            url=data.get("url", ""),
        )

    def to_dict(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "version": self.version,
            "description": self.description,
            "author": self.author,
            "url": self.url,
        }


class ToolStore:
    """Client for the AiOS plugin store REST API.

    Parameters:
        base_url: Override the default store URL.
        plugins_dir: Override the local plugin install directory.
    """

    def __init__(
        self,
        base_url: str = STORE_URL,
        plugins_dir: Optional[Path] = None,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.plugins_dir = plugins_dir or _PLUGINS_DIR

    # -- Remote operations ------------------------------------------------------

    def search(self, query: str) -> list[PluginInfo]:
        """Search the online store for plugins matching *query*."""
        url = f"{self.base_url}/plugins/search?q={_urlencode(query)}"
        data = self._api_get(url)
        if not isinstance(data, list):
            data = data.get("results", [])
        return [PluginInfo.from_dict(item) for item in data]

    def install(self, plugin_name: str) -> PluginInfo:
        """Download and install a plugin by name.

        The plugin archive is fetched from the store, extracted to
        ``~/.aios/plugins/<plugin_name>/``, and its manifest is written
        alongside the code.

        Returns the :class:`PluginInfo` of the installed plugin.
        Raises ``RuntimeError`` on failure.
        """
        # Fetch plugin metadata.
        url = f"{self.base_url}/plugins/{_urlencode(plugin_name)}"
        meta = self._api_get(url)
        info = PluginInfo.from_dict(meta)

        download_url = meta.get("download_url", f"{self.base_url}/plugins/{_urlencode(plugin_name)}/download")

        # Download the archive.
        archive_bytes = self._download(download_url)

        # Prepare the target directory.
        target = self.plugins_dir / plugin_name
        if target.exists():
            shutil.rmtree(target)
        target.mkdir(parents=True, exist_ok=True)

        # Extract — support both .tar.gz and .zip payloads.
        self._extract_archive(archive_bytes, target)

        # Persist manifest so we can query installed plugins offline.
        manifest_path = target / _MANIFEST_NAME
        manifest_path.write_text(json.dumps(info.to_dict(), indent=2), encoding="utf-8")

        logger.info("Installed plugin %s %s to %s", info.name, info.version, target)
        return info

    def uninstall(self, plugin_name: str) -> None:
        """Remove an installed plugin."""
        target = self.plugins_dir / plugin_name
        if not target.exists():
            raise FileNotFoundError(
                f"Plugin {plugin_name!r} is not installed at {target}"
            )
        shutil.rmtree(target)
        logger.info("Uninstalled plugin %s", plugin_name)

    def update(self, plugin_name: str) -> PluginInfo:
        """Update a plugin to the latest version from the store.

        This is effectively an uninstall + install cycle.
        """
        target = self.plugins_dir / plugin_name
        if not target.exists():
            raise FileNotFoundError(
                f"Plugin {plugin_name!r} is not installed — cannot update"
            )
        # Install will overwrite the existing directory.
        return self.install(plugin_name)

    # -- Local queries ----------------------------------------------------------

    def list_installed(self) -> list[PluginInfo]:
        """Return metadata for every locally installed plugin."""
        results: list[PluginInfo] = []
        if not self.plugins_dir.is_dir():
            return results

        for entry in sorted(self.plugins_dir.iterdir()):
            if not entry.is_dir() or entry.name.startswith("."):
                continue
            manifest = entry / _MANIFEST_NAME
            if manifest.exists():
                try:
                    data = json.loads(manifest.read_text(encoding="utf-8"))
                    results.append(PluginInfo.from_dict(data))
                except Exception:
                    logger.warning("Corrupt manifest in %s", entry)
            else:
                # Provide a minimal entry even without a manifest.
                results.append(
                    PluginInfo(
                        name=entry.name,
                        version="unknown",
                        description="",
                        author="",
                        url="",
                    )
                )
        return results

    # -- HTTP helpers -----------------------------------------------------------

    def _api_get(self, url: str) -> Any:
        """Perform a GET request and return the parsed JSON body."""
        req = Request(url, headers={"Accept": "application/json"})
        try:
            with urlopen(req, timeout=30) as resp:
                body = resp.read()
                return json.loads(body)
        except HTTPError as exc:
            raise RuntimeError(
                f"Store API error: {exc.code} {exc.reason} for {url}"
            ) from exc
        except URLError as exc:
            raise RuntimeError(
                f"Could not reach the plugin store at {url}: {exc.reason}"
            ) from exc

    def _download(self, url: str) -> bytes:
        """Download a binary payload from *url*."""
        req = Request(url)
        try:
            with urlopen(req, timeout=120) as resp:
                return resp.read()
        except HTTPError as exc:
            raise RuntimeError(
                f"Download failed: {exc.code} {exc.reason} for {url}"
            ) from exc
        except URLError as exc:
            raise RuntimeError(
                f"Could not download from {url}: {exc.reason}"
            ) from exc

    @staticmethod
    def _extract_archive(data: bytes, target: Path) -> None:
        """Extract a .tar.gz or .zip archive into *target*."""
        buf = io.BytesIO(data)

        if tarfile.is_tarfile(buf):
            buf.seek(0)
            with tarfile.open(fileobj=buf, mode="r:*") as tf:
                # Security: prevent path traversal.
                for member in tf.getmembers():
                    member_path = (target / member.name).resolve()
                    if not str(member_path).startswith(str(target.resolve())):
                        raise RuntimeError(
                            f"Tar archive contains path traversal: {member.name}"
                        )
                tf.extractall(path=target)  # noqa: S202
            return

        buf.seek(0)
        if zipfile.is_zipfile(buf):
            buf.seek(0)
            with zipfile.ZipFile(buf) as zf:
                for info in zf.infolist():
                    member_path = (target / info.filename).resolve()
                    if not str(member_path).startswith(str(target.resolve())):
                        raise RuntimeError(
                            f"Zip archive contains path traversal: {info.filename}"
                        )
                zf.extractall(path=target)  # noqa: S202
            return

        raise RuntimeError("Downloaded archive is neither a valid tar.gz nor zip file")


def _urlencode(value: str) -> str:
    """Percent-encode a string for safe use in URL path segments and query params."""
    from urllib.parse import quote

    return quote(value, safe="")
