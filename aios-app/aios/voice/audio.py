"""Audio I/O layer for AiOS voice system.

Provides capture (recording) and playback of audio using sounddevice,
which works with PipeWire, PulseAudio, and ALSA backends.
"""

from __future__ import annotations

import logging
import threading
import wave
from pathlib import Path
from typing import Optional

import numpy as np

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------

DEFAULT_SAMPLE_RATE = 16000
DEFAULT_CHANNELS = 1
DEFAULT_DTYPE = "float32"

# VAD defaults
VAD_ENERGY_THRESHOLD = 0.01
VAD_FRAME_DURATION_MS = 30


def _import_sounddevice():
    """Lazy-import sounddevice so the rest of the module can be imported
    even when no audio device is present (e.g. CI servers)."""
    try:
        import sounddevice as sd
        return sd
    except (ImportError, OSError) as exc:
        raise RuntimeError(
            "sounddevice is required for audio I/O. "
            "Install it with: pip install sounddevice"
        ) from exc


# ---------------------------------------------------------------------------
# AudioCapture
# ---------------------------------------------------------------------------


class AudioCapture:
    """Record audio from the system microphone.

    Uses *sounddevice* for cross-backend (PipeWire / PulseAudio / ALSA)
    capture.  Audio is collected in a background thread and returned as a
    NumPy array when :meth:`stop_recording` is called.

    Parameters:
        sample_rate: Sampling frequency in Hz (default 16 000).
        channels: Number of audio channels (default 1 -- mono).
        vad_threshold: RMS energy threshold for voice-activity detection.
            Frames whose RMS energy exceeds this value are considered speech.
        device: Sounddevice input device index or name.  ``None`` selects the
            system default.
    """

    def __init__(
        self,
        sample_rate: int = DEFAULT_SAMPLE_RATE,
        channels: int = DEFAULT_CHANNELS,
        vad_threshold: float = VAD_ENERGY_THRESHOLD,
        device: Optional[int | str] = None,
    ) -> None:
        self.sample_rate = sample_rate
        self.channels = channels
        self.vad_threshold = vad_threshold
        self.device = device

        self._frames: list[np.ndarray] = []
        self._stream: object | None = None
        self._recording = False
        self._lock = threading.Lock()
        self._speech_detected = False

    # -- public API ---------------------------------------------------------

    @property
    def is_recording(self) -> bool:
        """``True`` while audio capture is active."""
        return self._recording

    @property
    def speech_detected(self) -> bool:
        """``True`` if voice activity was detected during the current
        (or most recent) recording session."""
        return self._speech_detected

    def start_recording(self) -> None:
        """Begin capturing audio from the microphone.

        Raises:
            RuntimeError: If a recording session is already active or if
                no audio device can be opened.
        """
        if self._recording:
            raise RuntimeError("Recording is already in progress")

        sd = _import_sounddevice()

        with self._lock:
            self._frames = []
            self._speech_detected = False

        try:
            self._stream = sd.InputStream(
                samplerate=self.sample_rate,
                channels=self.channels,
                dtype=DEFAULT_DTYPE,
                device=self.device,
                callback=self._audio_callback,
                blocksize=int(self.sample_rate * VAD_FRAME_DURATION_MS / 1000),
            )
            self._stream.start()  # type: ignore[union-attr]
            self._recording = True
            logger.info(
                "Recording started (rate=%d, channels=%d, device=%s)",
                self.sample_rate,
                self.channels,
                self.device or "default",
            )
        except Exception as exc:
            self._recording = False
            self._stream = None
            raise RuntimeError(f"Failed to open audio input device: {exc}") from exc

    def stop_recording(self) -> np.ndarray:
        """Stop recording and return captured audio.

        Returns:
            A 1-D ``float32`` NumPy array of audio samples at the configured
            sample rate (mono) or a 2-D array ``(samples, channels)`` when
            capturing in stereo.

        Raises:
            RuntimeError: If no recording session is active.
        """
        if not self._recording:
            raise RuntimeError("No recording in progress")

        self._recording = False

        if self._stream is not None:
            try:
                self._stream.stop()  # type: ignore[union-attr]
                self._stream.close()  # type: ignore[union-attr]
            except Exception:
                logger.warning("Error closing audio stream", exc_info=True)
            finally:
                self._stream = None

        with self._lock:
            if not self._frames:
                logger.warning("Recording stopped but no audio frames were captured")
                return np.array([], dtype=np.float32)

            audio = np.concatenate(self._frames, axis=0)
            self._frames = []

        # Squeeze single-channel audio to 1-D for convenience
        if self.channels == 1 and audio.ndim == 2:
            audio = audio.squeeze(axis=1)

        logger.info(
            "Recording stopped: %.2f s, speech_detected=%s",
            len(audio) / self.sample_rate,
            self._speech_detected,
        )
        return audio

    def detect_voice_activity(self, audio_chunk: np.ndarray) -> bool:
        """Simple energy-based voice activity detection.

        Parameters:
            audio_chunk: A short chunk of audio samples (float32).

        Returns:
            ``True`` if the RMS energy of the chunk exceeds
            :attr:`vad_threshold`.
        """
        if audio_chunk.size == 0:
            return False
        rms = float(np.sqrt(np.mean(audio_chunk.astype(np.float64) ** 2)))
        return rms > self.vad_threshold

    @staticmethod
    def list_devices() -> list[dict]:
        """Return a list of available audio input devices.

        Each entry is a dict with keys from ``sounddevice.query_devices()``.
        """
        sd = _import_sounddevice()
        devices = sd.query_devices()
        if isinstance(devices, dict):
            devices = [devices]
        return [d for d in devices if d.get("max_input_channels", 0) > 0]

    # -- private ------------------------------------------------------------

    def _audio_callback(
        self,
        indata: np.ndarray,
        frames: int,
        time_info: object,
        status: object,
    ) -> None:
        """Called by sounddevice from its audio thread for each block."""
        if status:
            logger.debug("Audio callback status: %s", status)

        chunk = indata.copy()

        with self._lock:
            self._frames.append(chunk)

        if not self._speech_detected and self.detect_voice_activity(chunk):
            self._speech_detected = True


# ---------------------------------------------------------------------------
# AudioPlayback
# ---------------------------------------------------------------------------


class AudioPlayback:
    """Play audio through the system speakers.

    Playback is non-blocking -- audio is written to a *sounddevice*
    ``OutputStream`` that runs in a background thread managed by the
    PortAudio host API.

    Parameters:
        device: Sounddevice output device index or name.  ``None`` selects
            the system default.
    """

    def __init__(self, device: Optional[int | str] = None) -> None:
        self.device = device
        self._stream: object | None = None
        self._playing = False
        self._lock = threading.Lock()
        self._done_event = threading.Event()

    # -- public API ---------------------------------------------------------

    @property
    def is_playing(self) -> bool:
        """``True`` while audio playback is active."""
        return self._playing

    def play(
        self,
        audio_data: np.ndarray,
        sample_rate: int = DEFAULT_SAMPLE_RATE,
        blocking: bool = False,
    ) -> None:
        """Play raw audio samples.

        Parameters:
            audio_data: 1-D or 2-D ``float32`` / ``int16`` NumPy array.
            sample_rate: Sample rate of *audio_data*.
            blocking: If ``True``, block until playback finishes.

        Raises:
            RuntimeError: If playback is already in progress.
        """
        if self._playing:
            self.stop()

        sd = _import_sounddevice()

        if audio_data.size == 0:
            logger.warning("play() called with empty audio data")
            return

        # Ensure 2-D (samples, channels)
        if audio_data.ndim == 1:
            audio_data = audio_data[:, np.newaxis]

        channels = audio_data.shape[1]
        self._done_event.clear()
        self._play_index = 0
        self._play_data = audio_data

        def _callback(outdata: np.ndarray, frames: int, time_info: object, status: object) -> None:
            if status:
                logger.debug("Playback callback status: %s", status)

            start = self._play_index
            end = start + frames

            if end >= len(self._play_data):
                # Final chunk -- pad with zeros
                valid = self._play_data[start:]
                outdata[: len(valid)] = valid
                outdata[len(valid):] = 0
                self._play_index = len(self._play_data)
                raise sd.CallbackStop()
            else:
                outdata[:] = self._play_data[start:end]
                self._play_index = end

        def _finished_callback() -> None:
            self._playing = False
            self._done_event.set()
            logger.debug("Playback finished")

        try:
            self._stream = sd.OutputStream(
                samplerate=sample_rate,
                channels=channels,
                dtype=audio_data.dtype.name,
                device=self.device,
                callback=_callback,
                finished_callback=_finished_callback,
            )
            self._playing = True
            self._stream.start()  # type: ignore[union-attr]
            logger.info(
                "Playback started: %.2f s, rate=%d, channels=%d",
                len(audio_data) / sample_rate,
                sample_rate,
                channels,
            )
        except Exception as exc:
            self._playing = False
            self._stream = None
            raise RuntimeError(f"Failed to open audio output device: {exc}") from exc

        if blocking:
            self.wait()

    def play_file(self, path: str, blocking: bool = False) -> None:
        """Play a WAV file.

        Parameters:
            path: Path to a ``.wav`` file.
            blocking: If ``True``, block until playback finishes.

        Raises:
            FileNotFoundError: If *path* does not exist.
            RuntimeError: If the file cannot be read.
        """
        wav_path = Path(path)
        if not wav_path.exists():
            raise FileNotFoundError(f"WAV file not found: {path}")

        try:
            with wave.open(str(wav_path), "rb") as wf:
                n_channels = wf.getnchannels()
                sample_width = wf.getsampwidth()
                sample_rate = wf.getframerate()
                n_frames = wf.getnframes()
                raw = wf.readframes(n_frames)
        except wave.Error as exc:
            raise RuntimeError(f"Failed to read WAV file: {exc}") from exc

        # Convert raw bytes to numpy array
        if sample_width == 2:
            audio = np.frombuffer(raw, dtype=np.int16).astype(np.float32) / 32768.0
        elif sample_width == 4:
            audio = np.frombuffer(raw, dtype=np.int32).astype(np.float32) / 2147483648.0
        elif sample_width == 1:
            audio = (np.frombuffer(raw, dtype=np.uint8).astype(np.float32) - 128.0) / 128.0
        else:
            raise RuntimeError(f"Unsupported WAV sample width: {sample_width} bytes")

        if n_channels > 1:
            audio = audio.reshape(-1, n_channels)

        self.play(audio, sample_rate=sample_rate, blocking=blocking)

    def stop(self) -> None:
        """Stop any active playback immediately."""
        with self._lock:
            if self._stream is not None:
                try:
                    self._stream.stop()  # type: ignore[union-attr]
                    self._stream.close()  # type: ignore[union-attr]
                except Exception:
                    logger.warning("Error stopping audio playback", exc_info=True)
                finally:
                    self._stream = None
            self._playing = False
            self._done_event.set()

    def wait(self, timeout: float | None = None) -> bool:
        """Block until current playback finishes.

        Parameters:
            timeout: Maximum seconds to wait (``None`` = wait forever).

        Returns:
            ``True`` if playback finished, ``False`` on timeout.
        """
        return self._done_event.wait(timeout=timeout)

    @staticmethod
    def list_devices() -> list[dict]:
        """Return a list of available audio output devices."""
        sd = _import_sounddevice()
        devices = sd.query_devices()
        if isinstance(devices, dict):
            devices = [devices]
        return [d for d in devices if d.get("max_output_channels", 0) > 0]
