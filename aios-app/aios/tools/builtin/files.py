"""File operations tool -- read, write, list, search, and inspect files."""

from __future__ import annotations

import datetime
import fnmatch
import logging
import mimetypes
import os
from pathlib import Path
from typing import Any

from aios.tools.base import Tool, ToolResult

logger = logging.getLogger(__name__)

# Default root for file operations.  The tool refuses to operate outside this
# directory unless the resolved path is still under it (prevents traversal).
_DEFAULT_ROOT = Path.home()

# Maximum bytes to read from a file in a single request.
_MAX_READ_BYTES = 1_000_000  # ~1 MB


def _resolve_safe(path_str: str, root: Path | None = None) -> tuple[Path, str | None]:
    """Resolve *path_str* and verify it lives under *root*.

    Returns ``(resolved_path, error_message)``.  If *error_message* is not
    ``None`` the path must not be used.
    """
    if root is None:
        root = _DEFAULT_ROOT
    try:
        resolved = Path(path_str).expanduser().resolve()
    except Exception as exc:
        return Path(), f"Invalid path: {exc}"

    root_resolved = root.resolve()
    if not str(resolved).startswith(str(root_resolved)):
        return Path(), (
            f"Access denied: {resolved} is outside the allowed root ({root_resolved})."
        )
    return resolved, None


class FilesTool(Tool):
    """File-system operations scoped to the user's home directory."""

    @property
    def name(self) -> str:
        return "files"

    @property
    def description(self) -> str:
        return (
            "File operations: read a file, write a file, list a directory, "
            "search for files by name pattern, or get file info."
        )

    @property
    def parameters(self) -> dict:
        return {
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "read_file",
                        "write_file",
                        "list_directory",
                        "search_files",
                        "file_info",
                    ],
                    "description": "File operation to perform.",
                },
                "path": {
                    "type": "string",
                    "description": "File or directory path (relative to home, or absolute).",
                },
                "content": {
                    "type": "string",
                    "description": "Content to write (for write_file).",
                },
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern for search_files (e.g. '*.py').",
                },
            },
            "required": ["action"],
        }

    def execute(self, **kwargs: Any) -> ToolResult:
        action: str = kwargs.get("action", "")

        if action == "read_file":
            return self._read_file(kwargs.get("path", ""))
        if action == "write_file":
            return self._write_file(kwargs.get("path", ""), kwargs.get("content", ""))
        if action == "list_directory":
            return self._list_directory(kwargs.get("path", "~"))
        if action == "search_files":
            return self._search_files(
                kwargs.get("path", "~"), kwargs.get("pattern", "*")
            )
        if action == "file_info":
            return self._file_info(kwargs.get("path", ""))

        return ToolResult.fail(
            f"Unknown action {action!r}. "
            "Use: read_file, write_file, list_directory, search_files, file_info."
        )

    # -- Actions ----------------------------------------------------------------

    @staticmethod
    def _read_file(path_str: str) -> ToolResult:
        if not path_str:
            return ToolResult.fail("'path' is required for read_file.")

        resolved, err = _resolve_safe(path_str)
        if err:
            return ToolResult.fail(err)
        if not resolved.is_file():
            return ToolResult.fail(f"Not a file or does not exist: {resolved}")

        try:
            size = resolved.stat().st_size
            if size > _MAX_READ_BYTES:
                return ToolResult.fail(
                    f"File too large ({size} bytes). Max is {_MAX_READ_BYTES} bytes."
                )
            content = resolved.read_text(encoding="utf-8", errors="replace")
            return ToolResult.ok(content, data={"path": str(resolved), "size": size})
        except Exception as exc:
            return ToolResult.fail(f"Failed to read file: {exc}")

    @staticmethod
    def _write_file(path_str: str, content: str) -> ToolResult:
        if not path_str:
            return ToolResult.fail("'path' is required for write_file.")

        resolved, err = _resolve_safe(path_str)
        if err:
            return ToolResult.fail(err)

        try:
            resolved.parent.mkdir(parents=True, exist_ok=True)
            resolved.write_text(content, encoding="utf-8")
            return ToolResult.ok(
                f"Wrote {len(content)} bytes to {resolved}",
                data={"path": str(resolved), "bytes_written": len(content)},
            )
        except Exception as exc:
            return ToolResult.fail(f"Failed to write file: {exc}")

    @staticmethod
    def _list_directory(path_str: str) -> ToolResult:
        resolved, err = _resolve_safe(path_str)
        if err:
            return ToolResult.fail(err)
        if not resolved.is_dir():
            return ToolResult.fail(f"Not a directory or does not exist: {resolved}")

        try:
            entries: list[dict[str, Any]] = []
            for entry in sorted(resolved.iterdir()):
                kind = "dir" if entry.is_dir() else "file"
                try:
                    size = entry.stat().st_size if entry.is_file() else 0
                except OSError:
                    size = 0
                entries.append({"name": entry.name, "type": kind, "size": size})

            lines = []
            for e in entries:
                prefix = "[DIR] " if e["type"] == "dir" else "      "
                size_str = f" ({e['size']} B)" if e["type"] == "file" else ""
                lines.append(f"{prefix}{e['name']}{size_str}")

            return ToolResult.ok(
                "\n".join(lines) if lines else "(empty directory)",
                data={"path": str(resolved), "entries": entries},
            )
        except PermissionError:
            return ToolResult.fail(f"Permission denied: {resolved}")
        except Exception as exc:
            return ToolResult.fail(f"Failed to list directory: {exc}")

    @staticmethod
    def _search_files(path_str: str, pattern: str) -> ToolResult:
        resolved, err = _resolve_safe(path_str)
        if err:
            return ToolResult.fail(err)
        if not resolved.is_dir():
            return ToolResult.fail(f"Not a directory: {resolved}")

        matches: list[str] = []
        max_results = 200

        try:
            for root, dirs, files in os.walk(resolved):
                # Skip hidden directories.
                dirs[:] = [d for d in dirs if not d.startswith(".")]
                for fname in files:
                    if fnmatch.fnmatch(fname, pattern):
                        matches.append(os.path.join(root, fname))
                        if len(matches) >= max_results:
                            break
                if len(matches) >= max_results:
                    break
        except PermissionError:
            return ToolResult.fail(f"Permission denied while searching {resolved}")
        except Exception as exc:
            return ToolResult.fail(f"Search failed: {exc}")

        truncated = len(matches) >= max_results
        text = "\n".join(matches) if matches else "(no matches)"
        if truncated:
            text += f"\n... (truncated at {max_results} results)"

        return ToolResult.ok(
            text,
            data={"matches": matches, "truncated": truncated},
        )

    @staticmethod
    def _file_info(path_str: str) -> ToolResult:
        if not path_str:
            return ToolResult.fail("'path' is required for file_info.")

        resolved, err = _resolve_safe(path_str)
        if err:
            return ToolResult.fail(err)
        if not resolved.exists():
            return ToolResult.fail(f"Path does not exist: {resolved}")

        try:
            stat = resolved.stat()
            mime_type, _ = mimetypes.guess_type(str(resolved))
            info: dict[str, Any] = {
                "path": str(resolved),
                "type": "directory" if resolved.is_dir() else "file",
                "size": stat.st_size,
                "mime_type": mime_type or "unknown",
                "modified": datetime.datetime.fromtimestamp(stat.st_mtime).isoformat(),
                "created": datetime.datetime.fromtimestamp(stat.st_ctime).isoformat(),
                "permissions": oct(stat.st_mode)[-3:],
            }

            lines = [f"{k}: {v}" for k, v in info.items()]
            return ToolResult.ok("\n".join(lines), data=info)
        except Exception as exc:
            return ToolResult.fail(f"Failed to get file info: {exc}")
