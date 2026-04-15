# Reference /debate endpoint for testing the Bot Council harness.
# Run: python debate-endpoint-python.py [port]
# Default port: 9000
# No external dependencies required (stdlib only).

import json
import random
import sys
from http.server import HTTPServer, BaseHTTPRequestHandler


class DebateHandler(BaseHTTPRequestHandler):
    def do_POST(self):
        if self.path != "/debate":
            self.send_response(404)
            self.end_headers()
            return

        length = int(self.headers.get("Content-Length", 0))
        body = json.loads(self.rfile.read(length))

        if body.get("round") in (0, "0"):
            result = {
                "response": (
                    f"My position: {body.get('prompt', '')[:100]}. "
                    "The critical factors are evidence quality, procedural fairness, "
                    "and alignment with existing legal frameworks."
                ),
            }
        elif body.get("round") == "scoring":
            result = {
                "scores": [
                    {
                        "pseudonym": entry["pseudonym"],
                        "reasoning_quality": random.randint(5, 9),
                        "factual_grounding": random.randint(5, 9),
                        "overall": random.randint(5, 9),
                        "reasoning": f"{entry['pseudonym']} provides a well-structured argument.",
                    }
                    for entry in body.get("context", [])
                ],
            }
        else:
            result = {"error": "unknown round"}

        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(result).encode())

    def log_message(self, fmt, *args):
        print(f"[{self.log_date_time_string()}] {fmt % args}")


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 9000
    server = HTTPServer(("0.0.0.0", port), DebateHandler)
    print(f"Reference bot listening on port {port}")
    server.serve_forever()
