"""Central registry for tool instances.

The :class:`ToolRegistry` is a singleton that holds every tool available to the
AI.  Built-in tools ship with AiOS; external plugins can be loaded from
directories or discovered via Python *entry_points*.
"""

from __future__ import annotations

import importlib
import importlib.util
import logging
import os
import sys
from pathlib import Path
from typing import Optional

from .base import Tool

logger = logging.getLogger(__name__)

# Modules inside ``builtin/`` that contain tool classes to auto-register.
_BUILTIN_MODULES = [
    "aios.tools.builtin.memory",
    "aios.tools.builtin.display",
    "aios.tools.builtin.system_tools",
    "aios.tools.builtin.files",
    "aios.tools.builtin.web",
]


class ToolRegistry:
    """Singleton registry of :class:`Tool` instances.

    Access the singleton via :meth:`instance`.  The first call creates it; all
    subsequent calls return the same object.
    """

    _instance: Optional[ToolRegistry] = None

    def __init__(self) -> None:
        self._tools: dict[str, Tool] = {}

    # -- Singleton access -------------------------------------------------------

    @classmethod
    def instance(cls) -> ToolRegistry:
        """Return the global registry singleton, creating it on first call."""
        if cls._instance is None:
            cls._instance = cls()
        return cls._instance

    @classmethod
    def reset(cls) -> None:
        """Destroy the singleton (useful for tests)."""
        cls._instance = None

    # -- Core CRUD --------------------------------------------------------------

    def register(self, tool: Tool) -> None:
        """Register a tool.  Raises ``ValueError`` if the name is already taken."""
        if tool.name in self._tools:
            raise ValueError(
                f"A tool named {tool.name!r} is already registered"
            )
        self._tools[tool.name] = tool
        logger.info("Registered tool %r", tool.name)

    def unregister(self, name: str) -> None:
        """Remove a tool by name.  Raises ``KeyError`` if not found."""
        if name not in self._tools:
            raise KeyError(f"No tool named {name!r} is registered")
        del self._tools[name]
        logger.info("Unregistered tool %r", name)

    def get(self, name: str) -> Tool:
        """Return a tool by name.  Raises ``KeyError`` if not found."""
        try:
            return self._tools[name]
        except KeyError:
            raise KeyError(f"No tool named {name!r} is registered") from None

    def list_tools(self) -> list[Tool]:
        """Return all registered tools sorted by name."""
        return sorted(self._tools.values(), key=lambda t: t.name)

    def get_tools_schema(self) -> list[dict]:
        """Return JSON-Schema descriptions of every tool (for LLM function calling).

        Each dict has keys: ``name``, ``description``, ``parameters``.
        """
        return [tool.to_schema() for tool in self.list_tools()]

    # -- Discovery & loading ----------------------------------------------------

    def load_builtin_tools(self) -> None:
        """Import the built-in tool modules and register tool classes found in them.

        Each module is expected to expose one or more :class:`Tool` subclasses.
        Only *concrete* subclasses (i.e. not abstract) are instantiated and
        registered.
        """
        for modname in _BUILTIN_MODULES:
            try:
                mod = importlib.import_module(modname)
            except Exception:
                logger.exception("Failed to import built-in module %s", modname)
                continue
            self._register_tools_from_module(mod)

    def load_plugin(self, path: str) -> None:
        """Load a single plugin from *path* (a directory containing a Python package).

        The directory must contain either a top-level ``__init__.py`` or a
        ``plugin.py`` file that exposes :class:`Tool` subclasses.
        """
        plugin_dir = Path(path).resolve()
        if not plugin_dir.is_dir():
            raise FileNotFoundError(f"Plugin path does not exist: {plugin_dir}")

        # Add the parent to sys.path so the package can be imported.
        parent = str(plugin_dir.parent)
        if parent not in sys.path:
            sys.path.insert(0, parent)

        package_name = plugin_dir.name

        # Prefer plugin.py inside the directory, fall back to __init__.py.
        plugin_py = plugin_dir / "plugin.py"
        init_py = plugin_dir / "__init__.py"

        if plugin_py.exists():
            spec = importlib.util.spec_from_file_location(
                f"{package_name}.plugin", str(plugin_py)
            )
        elif init_py.exists():
            spec = importlib.util.spec_from_file_location(
                package_name, str(init_py), submodule_search_locations=[str(plugin_dir)]
            )
        else:
            raise FileNotFoundError(
                f"Plugin directory {plugin_dir} has no plugin.py or __init__.py"
            )

        if spec is None or spec.loader is None:
            raise ImportError(f"Could not create module spec for {plugin_dir}")

        mod = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = mod
        spec.loader.exec_module(mod)  # type: ignore[union-attr]
        self._register_tools_from_module(mod)
        logger.info("Loaded plugin from %s", plugin_dir)

    def load_plugins_dir(self, path: str) -> None:
        """Load every subdirectory in *path* as a plugin.

        Non-directory entries and hidden directories (starting with ``"."``) are
        silently skipped.
        """
        plugins_dir = Path(path).resolve()
        if not plugins_dir.is_dir():
            logger.warning("Plugins directory does not exist: %s", plugins_dir)
            return

        for entry in sorted(plugins_dir.iterdir()):
            if entry.is_dir() and not entry.name.startswith("."):
                try:
                    self.load_plugin(str(entry))
                except Exception:
                    logger.exception("Failed to load plugin %s", entry)

    def load_entry_points(self, group: str = "aios.plugins") -> None:
        """Discover plugins registered as Python entry points under *group*.

        Each entry point should resolve to a module containing one or more
        concrete :class:`Tool` subclasses.
        """
        try:
            from importlib.metadata import entry_points  # Python 3.9+
        except ImportError:
            from importlib_metadata import entry_points  # type: ignore[no-redef]

        eps = entry_points()
        # Python 3.12+ returns a SelectableGroups / list; earlier returns a dict.
        if isinstance(eps, dict):
            plugin_eps = eps.get(group, [])
        else:
            plugin_eps = eps.select(group=group)  # type: ignore[union-attr]

        for ep in plugin_eps:
            try:
                mod = ep.load()
                if isinstance(mod, type) and issubclass(mod, Tool) and mod is not Tool:
                    self.register(mod())
                else:
                    # Assume it's a module.
                    self._register_tools_from_module(mod)
                logger.info("Loaded entry-point plugin %r", ep.name)
            except Exception:
                logger.exception("Failed to load entry-point %r", ep.name)

    # -- Internal helpers -------------------------------------------------------

    def _register_tools_from_module(self, mod: object) -> None:
        """Scan *mod* for concrete :class:`Tool` subclasses and register them."""
        for attr_name in dir(mod):
            obj = getattr(mod, attr_name)
            if (
                isinstance(obj, type)
                and issubclass(obj, Tool)
                and obj is not Tool
                and not getattr(obj, "__abstractmethods__", None)
            ):
                try:
                    instance = obj()
                    if instance.name not in self._tools:
                        self.register(instance)
                except Exception:
                    logger.exception(
                        "Failed to instantiate tool class %s from %s",
                        attr_name,
                        getattr(mod, "__name__", mod),
                    )
