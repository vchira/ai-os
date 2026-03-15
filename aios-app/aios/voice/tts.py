"""Text-to-Speech engine for AiOS using Piper TTS.

Converts text into spoken audio using the Piper neural TTS engine.
Piper runs entirely locally and supports dozens of voices across many
languages.  This module shells out to the ``piper`` CLI binary and
falls back gracefully when it is not installed.
"""

from __future__ import annotations

import io
import json
import logging
import os
import shutil
import struct
import subprocess
import tempfile
import wave
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Optional

import numpy as np

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

MODELS_DIR = Path(
    os.environ.get("AIOS_MODELS_DIR_PIPER", Path.home() / ".aios" / "models" / "piper")
)

PIPER_DOWNLOAD_BASE = "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0"


class VoiceGender(str, Enum):
    MALE = "male"
    FEMALE = "female"
    NEUTRAL = "neutral"


class VoiceQuality(str, Enum):
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"


# ---------------------------------------------------------------------------
# Data types
# ---------------------------------------------------------------------------


@dataclass
class VoiceInfo:
    """Metadata for a Piper voice.

    Attributes:
        id: Unique voice identifier (e.g. ``"en_US-amy-medium"``).
        name: Human-friendly name.
        language: BCP-47 language code (e.g. ``"en_US"``).
        gender: Speaker gender.
        quality: Model quality tier (affects size and fidelity).
        downloaded: Whether the model files are present locally.
    """

    id: str
    name: str
    language: str
    gender: VoiceGender
    quality: VoiceQuality
    downloaded: bool


# ---------------------------------------------------------------------------
# Built-in voice catalog
# ---------------------------------------------------------------------------

# Each entry maps voice_id -> (name, language, gender, quality,
# relative HuggingFace path without the base URL).
# The download path convention is:
#   {lang}/{voice_id}/{quality}/{voice_id}.onnx
#   {lang}/{voice_id}/{quality}/{voice_id}.onnx.json

_VOICE_CATALOG: dict[str, tuple[str, str, VoiceGender, VoiceQuality]] = {
    # English
    "en_US-amy-medium":        ("Amy (US English)",       "en_US", VoiceGender.FEMALE, VoiceQuality.MEDIUM),
    "en_US-amy-low":           ("Amy Low (US English)",   "en_US", VoiceGender.FEMALE, VoiceQuality.LOW),
    "en_US-ryan-medium":       ("Ryan (US English)",      "en_US", VoiceGender.MALE,   VoiceQuality.MEDIUM),
    "en_US-ryan-low":          ("Ryan Low (US English)",  "en_US", VoiceGender.MALE,   VoiceQuality.LOW),
    "en_US-ryan-high":         ("Ryan HQ (US English)",   "en_US", VoiceGender.MALE,   VoiceQuality.HIGH),
    "en_GB-alan-medium":       ("Alan (British)",         "en_GB", VoiceGender.MALE,   VoiceQuality.MEDIUM),
    "en_GB-alba-medium":       ("Alba (British)",         "en_GB", VoiceGender.FEMALE, VoiceQuality.MEDIUM),
    # German
    "de_DE-thorsten-medium":   ("Thorsten (German)",      "de_DE", VoiceGender.MALE,   VoiceQuality.MEDIUM),
    "de_DE-thorsten-high":     ("Thorsten HQ (German)",   "de_DE", VoiceGender.MALE,   VoiceQuality.HIGH),
    "de_DE-eva_k-x_low":      ("Eva (German)",           "de_DE", VoiceGender.FEMALE, VoiceQuality.LOW),
    # French
    "fr_FR-siwis-medium":      ("Siwis (French)",         "fr_FR", VoiceGender.FEMALE, VoiceQuality.MEDIUM),
    "fr_FR-gilles-low":        ("Gilles (French)",        "fr_FR", VoiceGender.MALE,   VoiceQuality.LOW),
    # Spanish
    "es_ES-davefx-medium":     ("DaveFX (Spanish)",       "es_ES", VoiceGender.MALE,   VoiceQuality.MEDIUM),
    "es_MX-ald-medium":        ("Ald (Mexican Spanish)",  "es_MX", VoiceGender.MALE,   VoiceQuality.MEDIUM),
    "es_ES-mls_10246-low":     ("MLS Female (Spanish)",   "es_ES", VoiceGender.FEMALE, VoiceQuality.LOW),
    # Italian
    "it_IT-riccardo-x_low":    ("Riccardo (Italian)",     "it_IT", VoiceGender.MALE,   VoiceQuality.LOW),
    "it_IT-paola-medium":      ("Paola (Italian)",        "it_IT", VoiceGender.FEMALE, VoiceQuality.MEDIUM),
    # Portuguese
    "pt_BR-faber-medium":      ("Faber (Brazilian PT)",   "pt_BR", VoiceGender.MALE,   VoiceQuality.MEDIUM),
    "pt_PT-tugao-medium":      ("Tugao (European PT)",    "pt_PT", VoiceGender.MALE,   VoiceQuality.MEDIUM),
    # Russian
    "ru_RU-irina-medium":      ("Irina (Russian)",        "ru_RU", VoiceGender.FEMALE, VoiceQuality.MEDIUM),
    "ru_RU-denis-medium":      ("Denis (Russian)",        "ru_RU", VoiceGender.MALE,   VoiceQuality.MEDIUM),
    # Chinese
    "zh_CN-huayan-medium":     ("Huayan (Chinese)",       "zh_CN", VoiceGender.FEMALE, VoiceQuality.MEDIUM),
    # Japanese
    "ja_JP-kokoro-medium":     ("Kokoro (Japanese)",      "ja_JP", VoiceGender.FEMALE, VoiceQuality.MEDIUM),
    # Ukrainian
    "uk_UA-ukrainian_tts-medium": ("Ukrainian TTS",       "uk_UA", VoiceGender.MALE,   VoiceQuality.MEDIUM),
    # Polish
    "pl_PL-gosia-medium":      ("Gosia (Polish)",         "pl_PL", VoiceGender.FEMALE, VoiceQuality.MEDIUM),
    # Dutch
    "nl_NL-mls-medium":        ("MLS (Dutch)",            "nl_NL", VoiceGender.FEMALE, VoiceQuality.MEDIUM),
    # Norwegian
    "no_NO-talesyntese-medium": ("Talesyntese (Norwegian)", "no_NO", VoiceGender.MALE, VoiceQuality.MEDIUM),
}


# ---------------------------------------------------------------------------
# TextToSpeech
# ---------------------------------------------------------------------------


class TextToSpeech:
    """Synthesise speech from text using Piper TTS.

    Piper is invoked as a subprocess (the ``piper`` CLI binary).  Model
    files (ONNX + JSON config) are stored under
    ``~/.aios/models/piper/<voice_id>/``.

    Parameters:
        voice: Default voice identifier.  See :meth:`list_voices` for
            available options.
        speed: Playback speed multiplier (1.0 = normal).
        piper_binary: Path to the ``piper`` executable.  ``None`` searches
            ``$PATH``.
    """

    def __init__(
        self,
        voice: str = "en_US-amy-medium",
        speed: float = 1.0,
        piper_binary: Optional[str] = None,
    ) -> None:
        self._voice = voice
        self._speed = speed
        self._piper_binary = piper_binary or shutil.which("piper")

        MODELS_DIR.mkdir(parents=True, exist_ok=True)

    # -- public API ---------------------------------------------------------

    @property
    def voice(self) -> str:
        """The currently selected voice id."""
        return self._voice

    @property
    def speed(self) -> float:
        """Playback speed multiplier (1.0 = normal)."""
        return self._speed

    @speed.setter
    def speed(self, value: float) -> None:
        if value <= 0:
            raise ValueError("Speed must be positive")
        self._speed = value

    def set_voice(self, voice_id: str) -> None:
        """Change the active voice.

        Parameters:
            voice_id: A voice identifier from the catalog (e.g.
                ``"en_US-ryan-medium"``).

        Raises:
            ValueError: If the voice is not in the catalog.
        """
        if voice_id not in _VOICE_CATALOG:
            raise ValueError(
                f"Unknown voice {voice_id!r}. Use list_voices() to see available voices."
            )
        self._voice = voice_id
        logger.info("TTS voice changed to %s", voice_id)

    def synthesize(self, text: str) -> np.ndarray:
        """Convert text to a NumPy audio array.

        Parameters:
            text: The text to speak.

        Returns:
            A 1-D ``float32`` NumPy array of audio samples.  The sample
            rate depends on the voice model (typically 22 050 Hz) and can
            be read from :meth:`get_voice_sample_rate`.

        Raises:
            RuntimeError: If Piper is not installed, the voice model is
                missing, or synthesis fails.
        """
        if not text.strip():
            return np.array([], dtype=np.float32)

        self._ensure_piper_available()
        model_path = self._model_path(self._voice)
        self._ensure_model_downloaded(self._voice, model_path)

        wav_bytes = self._run_piper(text, model_path)
        return self._wav_bytes_to_numpy(wav_bytes)

    def synthesize_to_file(self, text: str, path: str) -> None:
        """Synthesise text and write the result to a WAV file.

        Parameters:
            text: The text to speak.
            path: Destination file path (will be overwritten).
        """
        if not text.strip():
            logger.warning("synthesize_to_file called with empty text")
            return

        self._ensure_piper_available()
        model_path = self._model_path(self._voice)
        self._ensure_model_downloaded(self._voice, model_path)

        wav_bytes = self._run_piper(text, model_path)
        Path(path).parent.mkdir(parents=True, exist_ok=True)
        Path(path).write_bytes(wav_bytes)
        logger.info("Saved TTS audio to %s", path)

    def list_voices(self) -> list[VoiceInfo]:
        """Return info about all voices in the built-in catalog.

        Returns:
            A list of :class:`VoiceInfo` objects.  The *downloaded* field
            indicates whether model files are present locally.
        """
        voices: list[VoiceInfo] = []
        for voice_id, (name, lang, gender, quality) in _VOICE_CATALOG.items():
            model_dir = MODELS_DIR / voice_id
            onnx_file = model_dir / f"{voice_id}.onnx"
            voices.append(
                VoiceInfo(
                    id=voice_id,
                    name=name,
                    language=lang,
                    gender=gender,
                    quality=quality,
                    downloaded=onnx_file.exists(),
                )
            )
        return voices

    def download_voice(self, voice_id: str) -> Path:
        """Download a voice model from Hugging Face.

        Parameters:
            voice_id: Identifier of the voice to download.

        Returns:
            Path to the local model directory.

        Raises:
            ValueError: If the voice is not in the catalog.
            RuntimeError: If the download fails.
        """
        if voice_id not in _VOICE_CATALOG:
            raise ValueError(
                f"Unknown voice {voice_id!r}. Use list_voices() to see options."
            )

        model_dir = MODELS_DIR / voice_id
        model_dir.mkdir(parents=True, exist_ok=True)

        onnx_path = model_dir / f"{voice_id}.onnx"
        json_path = model_dir / f"{voice_id}.onnx.json"

        if onnx_path.exists() and json_path.exists():
            logger.info("Voice %s already downloaded at %s", voice_id, model_dir)
            return model_dir

        _, lang, _, quality = _VOICE_CATALOG[voice_id]
        # Piper HF repo structure: {lang}/{voice_id}/{quality}/{voice_id}.onnx
        lang_short = lang[:2]  # e.g. "en" from "en_US"

        onnx_url = f"{PIPER_DOWNLOAD_BASE}/{lang_short}/{lang}/{voice_id}/{quality.value}/{voice_id}.onnx"
        json_url = f"{onnx_url}.json"

        try:
            import requests
        except ImportError as exc:
            raise RuntimeError(
                "requests library is required to download voices: pip install requests"
            ) from exc

        for url, dest in [(onnx_url, onnx_path), (json_url, json_path)]:
            logger.info("Downloading %s ...", url)
            try:
                resp = requests.get(url, stream=True, timeout=120)
                resp.raise_for_status()
                with open(dest, "wb") as f:
                    for chunk in resp.iter_content(chunk_size=1024 * 256):
                        f.write(chunk)
            except Exception as exc:
                # Clean up partial files
                dest.unlink(missing_ok=True)
                raise RuntimeError(
                    f"Failed to download {url}: {exc}"
                ) from exc

        logger.info("Voice %s downloaded to %s", voice_id, model_dir)
        return model_dir

    def get_voice_sample_rate(self, voice_id: Optional[str] = None) -> int:
        """Read the sample rate for a voice from its config JSON.

        Parameters:
            voice_id: Voice to query (defaults to the active voice).

        Returns:
            Sample rate in Hz (typically 22050).
        """
        vid = voice_id or self._voice
        model_dir = MODELS_DIR / vid
        json_path = model_dir / f"{vid}.onnx.json"

        if json_path.exists():
            try:
                config = json.loads(json_path.read_text())
                return int(config.get("audio", {}).get("sample_rate", 22050))
            except (json.JSONDecodeError, KeyError, TypeError):
                pass

        return 22050  # Piper default

    # -- private helpers ----------------------------------------------------

    def _ensure_piper_available(self) -> None:
        """Check that the piper binary is accessible."""
        if self._piper_binary is None:
            # Try piper-tts python package as fallback
            self._piper_binary = shutil.which("piper") or shutil.which("piper-tts")

        if self._piper_binary is None:
            raise RuntimeError(
                "Piper TTS binary not found. Install it with:\n"
                "  pip install piper-tts\n"
                "or download from https://github.com/rhasspy/piper/releases"
            )

    def _model_path(self, voice_id: str) -> Path:
        """Return the expected directory for a voice model."""
        return MODELS_DIR / voice_id

    def _ensure_model_downloaded(self, voice_id: str, model_dir: Path) -> None:
        """Raise a helpful error if the model is not downloaded."""
        onnx_path = model_dir / f"{voice_id}.onnx"
        if not onnx_path.exists():
            raise RuntimeError(
                f"Voice model '{voice_id}' is not downloaded. "
                f"Download it first with:\n"
                f"  tts.download_voice({voice_id!r})\n"
                f"Expected at: {onnx_path}"
            )

    def _run_piper(self, text: str, model_dir: Path) -> bytes:
        """Invoke the piper binary and return raw WAV bytes.

        Parameters:
            text: Input text.
            model_dir: Directory containing the ``.onnx`` and
                ``.onnx.json`` files.

        Returns:
            Raw WAV file content as bytes.
        """
        voice_id = model_dir.name
        onnx_path = model_dir / f"{voice_id}.onnx"

        # Build command
        cmd: list[str] = [
            self._piper_binary,  # type: ignore[list-item]
            "--model", str(onnx_path),
            "--output-raw",
        ]

        # Speed control via length_scale (inverse: higher scale = slower)
        if self._speed != 1.0:
            length_scale = 1.0 / self._speed
            cmd.extend(["--length-scale", f"{length_scale:.3f}"])

        logger.debug("Running piper: %s", " ".join(cmd))

        try:
            result = subprocess.run(
                cmd,
                input=text.encode("utf-8"),
                capture_output=True,
                timeout=120,
                check=False,
            )
        except FileNotFoundError:
            raise RuntimeError(
                f"Piper binary not found at {self._piper_binary}. "
                "Ensure piper is installed and in PATH."
            )
        except subprocess.TimeoutExpired:
            raise RuntimeError("Piper TTS timed out after 120 seconds")

        if result.returncode != 0:
            stderr = result.stderr.decode("utf-8", errors="replace").strip()
            raise RuntimeError(f"Piper TTS failed (exit {result.returncode}): {stderr}")

        raw_audio = result.stdout
        if not raw_audio:
            raise RuntimeError("Piper produced no audio output")

        # piper --output-raw emits raw 16-bit PCM at the model's sample rate.
        # Wrap it in a WAV header so downstream code can parse it uniformly.
        sample_rate = self.get_voice_sample_rate(voice_id)
        return self._raw_pcm_to_wav(raw_audio, sample_rate=sample_rate, channels=1, sample_width=2)

    @staticmethod
    def _raw_pcm_to_wav(
        pcm: bytes,
        sample_rate: int = 22050,
        channels: int = 1,
        sample_width: int = 2,
    ) -> bytes:
        """Wrap raw PCM bytes in a WAV container."""
        buf = io.BytesIO()
        with wave.open(buf, "wb") as wf:
            wf.setnchannels(channels)
            wf.setsampwidth(sample_width)
            wf.setframerate(sample_rate)
            wf.writeframes(pcm)
        return buf.getvalue()

    @staticmethod
    def _wav_bytes_to_numpy(wav_bytes: bytes) -> np.ndarray:
        """Parse WAV bytes into a 1-D float32 NumPy array."""
        buf = io.BytesIO(wav_bytes)
        with wave.open(buf, "rb") as wf:
            n_channels = wf.getnchannels()
            sample_width = wf.getsampwidth()
            n_frames = wf.getnframes()
            raw = wf.readframes(n_frames)

        if sample_width == 2:
            audio = np.frombuffer(raw, dtype=np.int16).astype(np.float32) / 32768.0
        elif sample_width == 4:
            audio = np.frombuffer(raw, dtype=np.int32).astype(np.float32) / 2147483648.0
        else:
            audio = np.frombuffer(raw, dtype=np.uint8).astype(np.float32) / 128.0 - 1.0

        # Mix to mono if multi-channel
        if n_channels > 1:
            audio = audio.reshape(-1, n_channels).mean(axis=1)

        return audio
