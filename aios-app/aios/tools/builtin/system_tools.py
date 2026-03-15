"""System tools -- run commands, query hardware, list processes."""

from __future__ import annotations

import datetime
import logging
import os
import platform
import re
import shutil
import subprocess
from typing import Any

from aios.tools.base import Tool, ToolResult

logger = logging.getLogger(__name__)

# Commands and patterns that are never allowed to execute.
_DANGEROUS_PATTERNS: list[re.Pattern[str]] = [
    re.compile(r"\brm\s+(-[a-zA-Z]*)?r", re.IGNORECASE),         # rm -r / rm -rf
    re.compile(r"\bmkfs\b", re.IGNORECASE),
    re.compile(r"\bdd\b.*\bof=/dev/", re.IGNORECASE),
    re.compile(r">\s*/dev/sd[a-z]", re.IGNORECASE),
    re.compile(r"\bshutdown\b", re.IGNORECASE),
    re.compile(r"\breboot\b", re.IGNORECASE),
    re.compile(r"\binit\s+[06]\b", re.IGNORECASE),
    re.compile(r"\bsystemctl\s+(poweroff|reboot|halt)\b", re.IGNORECASE),
    re.compile(r":(){ :\|:& };:", re.IGNORECASE),                 # fork bomb
    re.compile(r"\bchmod\s+(-[a-zA-Z]*)?\s*777\s+/", re.IGNORECASE),
    re.compile(r"\bchown\s+.*\s+/", re.IGNORECASE),
]

_COMMAND_TIMEOUT = 30  # seconds


def _is_command_safe(command: str) -> tuple[bool, str]:
    """Return (safe, reason).  If not safe, *reason* explains why."""
    for pat in _DANGEROUS_PATTERNS:
        if pat.search(command):
            return False, f"Blocked by safety rule: {pat.pattern}"
    return True, ""


class SystemTool(Tool):
    """Run shell commands, query system information, and list processes."""

    @property
    def name(self) -> str:
        return "system"

    @property
    def description(self) -> str:
        return (
            "System utilities: run a shell command (sandboxed), get system info, "
            "get the current date/time, or list running processes."
        )

    @property
    def parameters(self) -> dict:
        return {
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "run_command",
                        "get_system_info",
                        "get_datetime",
                        "list_processes",
                    ],
                    "description": "System action to perform.",
                },
                "command": {
                    "type": "string",
                    "description": "Shell command to execute (for run_command).",
                },
            },
            "required": ["action"],
        }

    def execute(self, **kwargs: Any) -> ToolResult:
        action: str = kwargs.get("action", "")

        if action == "run_command":
            return self._run_command(kwargs.get("command", ""))
        if action == "get_system_info":
            return self._get_system_info()
        if action == "get_datetime":
            return self._get_datetime()
        if action == "list_processes":
            return self._list_processes()

        return ToolResult.fail(
            f"Unknown action {action!r}. "
            "Use: run_command, get_system_info, get_datetime, list_processes."
        )

    # -- Actions ----------------------------------------------------------------

    @staticmethod
    def _run_command(command: str) -> ToolResult:
        if not command:
            return ToolResult.fail("'command' is required for run_command.")

        safe, reason = _is_command_safe(command)
        if not safe:
            return ToolResult.fail(f"Command rejected: {reason}")

        try:
            result = subprocess.run(
                command,
                shell=True,
                capture_output=True,
                text=True,
                timeout=_COMMAND_TIMEOUT,
                env={**os.environ, "LC_ALL": "C.UTF-8"},
            )
            output = result.stdout
            if result.stderr:
                output += ("\n--- stderr ---\n" + result.stderr) if output else result.stderr

            return ToolResult(
                success=result.returncode == 0,
                output=output.strip() or "(no output)",
                data={"returncode": result.returncode},
                error=f"Exit code {result.returncode}" if result.returncode != 0 else None,
            )
        except subprocess.TimeoutExpired:
            return ToolResult.fail(
                f"Command timed out after {_COMMAND_TIMEOUT}s."
            )
        except Exception as exc:
            return ToolResult.fail(f"Failed to run command: {exc}")

    @staticmethod
    def _get_system_info() -> ToolResult:
        info: dict[str, Any] = {}
        info["platform"] = platform.platform()
        info["architecture"] = platform.machine()
        info["python_version"] = platform.python_version()
        info["hostname"] = platform.node()
        info["cpu_count"] = os.cpu_count()

        # Memory (Linux /proc/meminfo).
        try:
            with open("/proc/meminfo") as f:
                meminfo = f.read()
            for line in meminfo.splitlines():
                if line.startswith("MemTotal:"):
                    info["memory_total"] = line.split(":")[1].strip()
                elif line.startswith("MemAvailable:"):
                    info["memory_available"] = line.split(":")[1].strip()
        except OSError:
            pass

        # Disk usage for /.
        try:
            usage = shutil.disk_usage("/")
            info["disk_total_gb"] = round(usage.total / (1024**3), 2)
            info["disk_free_gb"] = round(usage.free / (1024**3), 2)
            info["disk_used_gb"] = round(usage.used / (1024**3), 2)
        except OSError:
            pass

        # Load average.
        try:
            load1, load5, load15 = os.getloadavg()
            info["load_average"] = f"{load1:.2f}, {load5:.2f}, {load15:.2f}"
        except OSError:
            pass

        lines = [f"{k}: {v}" for k, v in info.items()]
        return ToolResult.ok("\n".join(lines), data=info)

    @staticmethod
    def _get_datetime() -> ToolResult:
        now = datetime.datetime.now()
        utc = datetime.datetime.now(datetime.timezone.utc)
        text = (
            f"Local: {now.strftime('%Y-%m-%d %H:%M:%S')}\n"
            f"UTC:   {utc.strftime('%Y-%m-%d %H:%M:%S')}\n"
            f"Timezone: {datetime.datetime.now().astimezone().tzname()}"
        )
        return ToolResult.ok(
            text,
            data={
                "local": now.isoformat(),
                "utc": utc.isoformat(),
                "timezone": datetime.datetime.now().astimezone().tzname(),
            },
        )

    @staticmethod
    def _list_processes() -> ToolResult:
        try:
            result = subprocess.run(
                ["ps", "aux", "--sort=-%mem"],
                capture_output=True,
                text=True,
                timeout=10,
            )
            if result.returncode != 0:
                return ToolResult.fail(f"ps failed: {result.stderr.strip()}")
            return ToolResult.ok(result.stdout.strip())
        except FileNotFoundError:
            return ToolResult.fail("'ps' command not found on this system.")
        except subprocess.TimeoutExpired:
            return ToolResult.fail("Process listing timed out.")
        except Exception as exc:
            return ToolResult.fail(f"Failed to list processes: {exc}")
