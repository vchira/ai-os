# Network & Connectivity

AiOS requires network access for its core AI functionality (reaching the LLM API). This chapter covers network troubleshooting.

## No internet connection

**Symptoms:** The AI does not respond. Error messages about API connectivity.

### Check basic connectivity

Open a terminal (`Alt+Enter`) and test:

```bash
# Check if you have an IP address
ip addr

# Test internet connectivity
ping -c 3 8.8.8.8

# Test DNS resolution
ping -c 3 api.anthropic.com
```

### Wired (Ethernet)

Ethernet should work automatically. If it does not:

1. Check the cable is plugged in.
2. Check the network interface:
   ```bash
   ip link
   ```
   The interface (usually `eth0` or `enp*`) should show `state UP`.

3. If the interface is down:
   ```bash
   sudo ip link set eth0 up
   sudo dhclient eth0
   ```

### Wireless (Wi-Fi)

AiOS includes firmware for most Wi-Fi adapters. To connect:

1. Check if a Wi-Fi interface is detected:
   ```bash
   iw dev
   ```

2. Scan for networks:
   ```bash
   sudo iw dev wlan0 scan | grep SSID
   ```

3. Connect to a network using `wpa_supplicant`:
   ```bash
   wpa_passphrase "YourNetworkName" "YourPassword" | sudo tee /etc/wpa_supplicant.conf
   sudo wpa_supplicant -B -i wlan0 -c /etc/wpa_supplicant.conf
   sudo dhclient wlan0
   ```

   Or ask the AI to help: "Connect me to Wi-Fi network MyNetwork with password mypassword."

### DNS issues

If `ping 8.8.8.8` works but `ping api.anthropic.com` fails, you have a DNS problem:

```bash
# Set Google DNS
echo "nameserver 8.8.8.8" | sudo tee /etc/resolv.conf
```

## Web channel not accessible

**Symptoms:** Other devices cannot reach `http://aios.local`.

### Check the web channel is enabled

```
/channel
```

If the web channel shows as disabled:

```
/channel web on
```

### Check mDNS (Avahi)

The `aios.local` hostname uses mDNS via Avahi:

```bash
systemctl status avahi-daemon
```

If Avahi is not running:

```bash
sudo systemctl start avahi-daemon
```

### Use IP address instead

If mDNS is not working (common on enterprise networks), find the AiOS IP address:

```bash
ip addr | grep "inet "
```

Use the IP address directly: `http://192.168.1.100` (replace with your actual IP).

### Check the port

By default, the web channel runs on port 80. If you changed it:

```
/channel web port 80
```

Check that the port is not blocked:

```bash
ss -tlnp | grep :80
```

### Firewall

If a firewall is active, ensure the web port is open:

```bash
sudo iptables -L -n | grep 80
```

## Cannot reach LLM API

**Symptoms:** Internet works but the AI cannot connect to its provider.

### Test API connectivity

```bash
curl -s -o /dev/null -w "%{http_code}" https://api.anthropic.com/v1/messages
curl -s -o /dev/null -w "%{http_code}" https://api.openai.com/v1/chat/completions
```

A response (even an error like 401 or 405) means the connection works. No response or a timeout means the API is blocked.

### Proxy or corporate network

If you are behind a proxy:

```bash
export HTTPS_PROXY=http://proxy.company.com:8080
```

Set this in `/etc/environment` to persist across reboots.

### API provider outage

Check provider status:
- Anthropic: [status.anthropic.com](https://status.anthropic.com)
- OpenAI: [status.openai.com](https://status.openai.com)

If one provider is down, switch to your backup:

```
/provider openai
```

## Signal channel not connecting

See the [Signal Messenger](../usage/channels/signal.md) chapter for Signal-specific troubleshooting.

## Running the network self-test

```
/selftest quick
```

The quick self-test includes network connectivity checks. For channel-specific tests:

```
/selftest channel
```

This verifies each channel's network connectivity and service status.
