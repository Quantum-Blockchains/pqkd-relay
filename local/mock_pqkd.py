#!/usr/bin/env python3
"""
Mock PQKD server — emulates ETSI QKD 014 REST API.
Usage: python3 mock_pqkd.py <port>

Responds to:
  GET /api/v1/keys/{sae_id}/enc_keys?size=<bits>&number=<n>
  GET /api/v1/keys/{sae_id}/status
"""
import http.server
import json
import base64
import os
import sys
import uuid
from urllib.parse import urlparse, parse_qs

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 9001


class MockPqkdHandler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        parsed = urlparse(self.path)
        parts = parsed.path.strip("/").split("/")

        # GET /api/v1/keys/{sae_id}/status
        if parts[-1] == "status":
            body = json.dumps({"source_KME_ID": "mock", "target_KME_ID": "mock",
                               "master_SAE_ID": "mock", "slave_SAE_ID": "mock",
                               "key_size": 256, "stored_key_count": 100,
                               "max_key_count": 1000, "max_key_per_request": 128,
                               "max_key_size": 1024, "min_key_size": 64,
                               "max_SAE_ID_count": 0})
            self._respond(200, body)
            return

        # GET /api/v1/keys/{sae_id}/enc_keys
        if parts[-1] == "enc_keys":
            qs = parse_qs(parsed.query)
            number = int(qs.get("number", ["1"])[0])
            size_bits = int(qs.get("size", ["256"])[0])
            key_bytes = max(size_bits // 8, 1)

            keys = [
                {
                    "key_ID": str(uuid.uuid4()),
                    "key": base64.b64encode(os.urandom(key_bytes)).decode(),
                }
                for _ in range(number)
            ]
            self._respond(200, json.dumps({"keys": keys}))
            return

        self._respond(404, json.dumps({"error": "not found"}))

    def _respond(self, status: int, body: str):
        encoded = body.encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def log_message(self, fmt, *args):
        print(f"[mock-pqkd:{PORT}] {fmt % args}", flush=True)


if __name__ == "__main__":
    server = http.server.HTTPServer(("0.0.0.0", PORT), MockPqkdHandler)
    print(f"Mock PQKD listening on port {PORT}", flush=True)
    server.serve_forever()
