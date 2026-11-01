# SPDX-License-Identifier: Apache-2.0
"""Loopback-only adapter for the disposable Safari fixture, never a live host.

Safari rejects Secure cookies on plain HTTP loopback. Keep production headers
unchanged and adapt only this synthetic fixture's response. WebSockets pass
through unchanged. Every non-upgrade connection closes after its response.
"""
import select
import socket
import socketserver
import threading


def start(listen_port, upstream_port):
    class Handler(socketserver.BaseRequestHandler):
        def handle(self):
            self.request.settimeout(10)
            request = bytearray()
            while b'\r\n\r\n' not in request:
                chunk = self.request.recv(65536)
                if not chunk or len(request) + len(chunk) > 131072:
                    return
                request.extend(chunk)
            headers, body = bytes(request).split(b'\r\n\r\n', 1)
            headers = b'\r\n'.join(line.replace(b'otrios_fixture=', b'__Host-otrios_monitoring=')
                                    if line.lower().startswith(b'cookie:') else line
                                    for line in headers.split(b'\r\n'))
            upgrade = b'upgrade: websocket' in headers.lower()
            if not upgrade:
                headers = b'\r\n'.join(line for line in headers.split(b'\r\n')
                                       if not line.lower().startswith(b'connection:'))
                headers += b'\r\nConnection: close'
            with socket.create_connection(('127.0.0.1', upstream_port), timeout=10) as upstream:
                upstream.sendall(headers + b'\r\n\r\n' + body)
                response = bytearray()
                first = True
                while True:
                    ready, _, _ = select.select([self.request, upstream], [], [], 60)
                    if not ready:
                        return
                    for source in ready:
                        chunk = source.recv(65536)
                        if not chunk:
                            return
                        if source is self.request:
                            upstream.sendall(chunk)
                        elif first:
                            response.extend(chunk)
                            if len(response) > 131072:
                                return
                            if b'\r\n\r\n' not in response:
                                continue
                            head, rest = bytes(response).split(b'\r\n\r\n', 1)
                            lines = [line.replace(b'; Secure', b'').replace(b'__Host-otrios_monitoring=', b'otrios_fixture=')
                                     if line.lower().startswith(b'set-cookie:') else line
                                     for line in head.split(b'\r\n')]
                            self.request.sendall(b'\r\n'.join(lines) + b'\r\n\r\n' + rest)
                            first = False
                        else:
                            self.request.sendall(chunk)

    class Server(socketserver.ThreadingTCPServer):
        daemon_threads = True
        allow_reuse_address = True

    server = Server(('127.0.0.1', listen_port), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server
