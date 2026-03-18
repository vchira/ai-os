# Web Interface

The Web channel lets you interact with AiOS from any device on your local network using a web browser. It mirrors the desktop experience with a full chat interface, panels, and image support.

## Accessing the web interface

Open a browser on any device connected to the same network and navigate to:

```
http://aios.local
```

The web interface works on phones, tablets, laptops, and desktops. No app installation is required.

If `aios.local` does not resolve (some networks block mDNS), use the IP address of the AiOS machine instead. You can find it by asking the AI on the desktop: "What is my IP address?"

## Enabling the web channel

The web channel is enabled by default. To check its status:

```
/channel
```

To enable or disable:

```
/channel web on
/channel web off
```

To change the port (default is 80):

```
/channel web port 8080
```

If you change the port, access the web interface at `http://aios.local:8080`.

## Features

The web interface provides nearly all the capabilities of the desktop channel:

- **Chat conversation** -- send messages and see AI responses
- **Markdown rendering** -- formatted text, code blocks, lists
- **Panels and forms** -- interactive input dialogs
- **Image display** -- view images the AI generates or retrieves
- **Notifications** -- toast messages for system events

The web interface is a single HTML/CSS/JavaScript page served by an axum HTTP server on the AiOS machine. It communicates with the AI through a WebSocket connection.

## Channel switching

When you send a message through the web interface, the web becomes the active channel. The desktop will show an overlay indicating the AI is talking on the web channel.

If someone sends a message from the desktop (or Signal), the channel switches there. The web interface will show a notification that the conversation has moved.

## Use cases

- **Couch browsing** -- control AiOS from your phone while sitting across the room
- **Multi-device** -- start a conversation on the desktop, continue from your tablet
- **Shared access** -- multiple people can view the web interface (only one can be the active channel at a time)
- **Headless operation** -- if AiOS is running on a machine without a monitor, the web interface is the primary way to interact

## Network requirements

The web channel requires:

- AiOS and the client device on the same local network
- Port 80 (or your configured port) not blocked by a firewall
- Avahi/mDNS for the `aios.local` hostname (or use the IP address directly)

AiOS uses Avahi for mDNS discovery. Most home networks support this automatically. Enterprise networks may block mDNS -- in that case, use the IP address.

## Security considerations

The web interface has no authentication by default. Anyone on your local network can access it. This is by design for ease of use in home environments.

If you are on a shared or untrusted network:

- Disable the web channel: `/channel web off`
- Or restrict access at the network level (firewall rules)

The web interface communicates over plain HTTP (not HTTPS) on the local network. API keys and vault contents are never transmitted through the web interface -- only chat messages and tool results.
