#!/usr/bin/env python3
"""AiOS API Proxy — bridges plain HTTP from the VM to HTTPS APIs.

Run on the host machine. The VM connects to host:8080 over plain HTTP,
this proxy reads the Host header and forwards to the correct HTTPS API.

Supported backends:
  - api.anthropic.com (Claude)
  - api.openai.com (OpenAI)

Usage: python3 tools/api_proxy.py [port]
"""

import http.server
import urllib.request
import ssl
import sys

LISTEN_PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8080

# Allowed HTTPS backends (whitelist for security)
ALLOWED_HOSTS = {
    'api.anthropic.com',
    'api.openai.com',
}

class ProxyHandler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        content_len = int(self.headers.get('Content-Length', 0))
        body = self.rfile.read(content_len) if content_len > 0 else b''

        # Determine target from Host header
        target_host = self.headers.get('Host', '').split(':')[0]
        if target_host not in ALLOWED_HOSTS:
            msg = f"Blocked: {target_host} not in allowed hosts".encode()
            self.send_response(403)
            self.send_header('Content-Length', str(len(msg)))
            self.end_headers()
            self.wfile.write(msg)
            return

        # Forward all headers except Host
        headers = {}
        for key, val in self.headers.items():
            if key.lower() not in ('host', 'content-length'):
                headers[key] = val

        url = f"https://{target_host}{self.path}"
        req = urllib.request.Request(url, data=body, headers=headers, method='POST')

        try:
            ctx = ssl.create_default_context()
            with urllib.request.urlopen(req, context=ctx, timeout=60) as resp:
                resp_body = resp.read()
                self.send_response(resp.status)
                for key, val in resp.getheaders():
                    if key.lower() not in ('transfer-encoding', 'connection'):
                        self.send_header(key, val)
                self.send_header('Content-Length', str(len(resp_body)))
                self.end_headers()
                self.wfile.write(resp_body)
        except urllib.error.HTTPError as e:
            resp_body = e.read()
            self.send_response(e.code)
            self.send_header('Content-Length', str(len(resp_body)))
            self.end_headers()
            self.wfile.write(resp_body)
        except Exception as e:
            msg = str(e).encode()
            self.send_response(502)
            self.send_header('Content-Length', str(len(msg)))
            self.end_headers()
            self.wfile.write(msg)

    def log_message(self, fmt, *args):
        print(f"[proxy] {args[0]}")

if __name__ == '__main__':
    server = http.server.HTTPServer(('0.0.0.0', LISTEN_PORT), ProxyHandler)
    print(f"AiOS API Proxy listening on 0.0.0.0:{LISTEN_PORT}")
    print(f"Routes Host header to HTTPS: {', '.join(sorted(ALLOWED_HOSTS))}")
    print(f"VM should connect to 10.0.2.2:{LISTEN_PORT}")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nProxy stopped.")
