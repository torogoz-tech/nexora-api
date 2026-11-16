"""Pause one fixture graph request; never log credentials, payloads or responses."""
from http.client import HTTPConnection
from http.server import BaseHTTPRequestHandler, HTTPServer
import sys


class ReadGate(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        size = int(self.headers.get("Content-Length", "0"))
        if size < 0 or size > 1048576:
            self.send_error(413)
            return
        payload = self.rfile.read(size)
        print("waiting", flush=True)
        if sys.stdin.readline().strip() != "release":
            self.send_error(503)
            return
        upstream = HTTPConnection("127.0.0.1", int(sys.argv[1]), timeout=10)
        try:
            headers = {name: self.headers[name] for name in
                       ("Authorization", "Content-Type", "Accept") if name in self.headers}
            upstream.request("POST", self.path, payload, headers)
            response = upstream.getresponse()
            content = response.read(1048577)
            if len(content) > 1048576:
                self.send_error(502)
                return
            self.send_response(response.status)
            self.send_header("Content-Type", response.getheader("Content-Type", "application/json"))
            self.send_header("Content-Length", str(len(content)))
            self.end_headers()
            self.wfile.write(content)
        finally:
            upstream.close()


server = HTTPServer(("127.0.0.1", 0), ReadGate)
print(f"ready {server.server_port}", flush=True)
server.serve_forever()
