# Audio Problems

AiOS uses PipeWire for audio. This chapter covers common audio issues and their solutions.

## No audio input (microphone)

**Symptoms:** The AI does not hear you. Voice input does not work. No transcription appears.

### Check that voice input is enabled

```
/mic on
```

If the microphone was disabled, this re-enables it.

### Check audio devices

Open a terminal (`Alt+Enter`) and check PipeWire status:

```bash
wpctl status
```

This shows all audio devices. Look for your microphone under "Sources." If it is not listed, the device may not be detected.

### Check the microphone volume

```bash
wpctl get-volume @DEFAULT_SOURCE@
```

If the volume is 0.00 or very low:

```bash
wpctl set-volume @DEFAULT_SOURCE@ 0.8
```

### Check that PipeWire is running

```bash
systemctl --user status pipewire
```

If PipeWire is not running:

```bash
systemctl --user start pipewire
```

### USB microphone not detected

1. Unplug and replug the microphone.
2. Check `dmesg` for USB errors:
   ```bash
   dmesg | tail -20
   ```
3. Try a different USB port.

## No audio output (speakers)

**Symptoms:** The AI responds with text but does not speak. No sound from speakers.

### Check that voice output is enabled

```
/speaker on
```

### Check the speaker volume

```bash
wpctl get-volume @DEFAULT_SINK@
```

Increase volume if needed:

```bash
wpctl set-volume @DEFAULT_SINK@ 0.8
```

### Check the output device

```bash
wpctl status
```

Look under "Sinks" for your speakers or headphones. If the wrong device is the default:

```bash
wpctl set-default <device-id>
```

Replace `<device-id>` with the ID number from `wpctl status`.

### HDMI audio

If you are connected via HDMI and want audio through the TV/monitor:

```bash
wpctl status
```

Find the HDMI output device and set it as default:

```bash
wpctl set-default <hdmi-device-id>
```

## Audio in virtual machines

Audio in VMs requires proper audio passthrough configuration.

### QEMU

Make sure your QEMU command includes audio:

```bash
-audio driver=pipewire,model=virtio
```

Or for PulseAudio:

```bash
-audio driver=pa,model=virtio
```

The included `run-vm.sh` script configures this automatically.

### VirtualBox

1. Go to VM Settings > Audio
2. Enable Audio
3. Set Host Audio Driver to your system's driver (PulseAudio or ALSA)
4. Set Audio Controller to ICH AC97 or Intel HD Audio

### If VM audio still does not work

Audio passthrough in VMs can be unreliable. If you cannot get it working:

```
/mic off
/speaker off
```

Use text-only mode. All AI functionality works without audio.

## Poor voice recognition

**Symptoms:** The AI misunderstands what you say frequently.

### Set your language explicitly

```
/language en
```

Auto-detection sometimes picks the wrong language, especially in noisy environments.

### Reduce background noise

Whisper works best in relatively quiet environments. A headset with a close microphone helps significantly.

### Check the Whisper model

The system auto-selects a Whisper model based on available RAM. If recognition is poor and you have enough RAM (8 GB+), the system should be using whisper-medium or larger. You can check which model is active with `/sysinfo`.

## TTS voice sounds wrong

### List available voices

```
/voice
```

### Switch to a different voice

```
/voice en-us
```

### Piper vs espeak-ng

If TTS sounds robotic, it may be using espeak-ng as a fallback. Check if a Piper voice is available for your language:

```
/voice
```

Piper voices produce significantly more natural speech. If no Piper voice is available for your language, espeak-ng is used automatically.

## Still stuck?

Run the audio-specific self-test:

```
/selftest audio
```

This checks PipeWire status, audio devices, and voice engine availability. It reports exactly what is working and what is not.
