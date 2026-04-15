from http.server import HTTPServer, BaseHTTPRequestHandler
import json

class DebateHandler(BaseHTTPRequestHandler):
    def do_POST(self):
        if self.path != '/debate':
            self.send_response(404)
            self.end_headers()
            self.wfile.write(json.dumps({"error": "not found"}).encode())
            return

        length = int(self.headers.get('Content-Length', 0))
        body = json.loads(self.rfile.read(length))
        round_num = body.get('round')
        role = body.get('role', 'unknown')

        if round_num == 0:
            response = {
                "response": f"[{role}] Initial position: This topic requires rigorous empirical analysis.",
            }
        elif round_num == 1:
            response = {
                "response": "The strongest opposing argument is the appeal to historical precedent. I would reconsider if shown systematic evidence of a different pattern.",
                "confidence": 60,
            }
        elif round_num == 2:
            response = {
                "response": "I challenge the reliance on anecdotal evidence rather than systematic study.",
                "confidence": 65,
                "challenge": {
                    "claim_targeted": "The assertion based on anecdotal evidence",
                    "counter_evidence": "Systematic reviews show no consistent pattern matching the anecdotal claims.",
                    "type": "factual",
                },
            }
        elif round_num == 3:
            response = {
                "response": "Question: What would constitute sufficient evidence to falsify your position? Answer: A controlled study with adequate sample size.",
                "confidence": 63,
            }
        elif round_num == 4:
            response = {
                "response": "Final position: Empirical rigour remains the correct lens. The debate has sharpened the methodological requirements.",
                "confidence": 70,
                "position_change": {
                    "changed": True,
                    "from_summary": "General call for empirical analysis",
                    "to_summary": "Specific methodological requirements identified",
                    "reason": "Agent B's challenge about sample size validity was compelling.",
                },
            }
        elif round_num == "scoring":
            scores = [
                {
                    "pseudonym": entry["pseudonym"],
                    "reasoning_quality": 7,
                    "factual_grounding": 6,
                    "overall": 7,
                    "reasoning": "Adequate reasoning.",
                }
                for entry in body.get("context", [])
            ]
            response = {"scores": scores}
        else:
            response = {"response": "Unknown round", "confidence": 50}

        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(json.dumps(response).encode())

HTTPServer(('', 3201), DebateHandler).serve_forever()
