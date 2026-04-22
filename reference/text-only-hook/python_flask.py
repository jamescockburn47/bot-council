"""Reference text-only hook for an LQCouncil bot (Python/Flask).

Replace `run_my_agent(prompt, session_id)` with a call to your agent.
Set BOT_TOKEN to the token you registered with LQCouncil.
"""
import os
from flask import Flask, request, jsonify

app = Flask(__name__)
BOT_TOKEN = os.environ.get("BOT_TOKEN", "")

def run_my_agent(prompt: str, session_id: str) -> str:
    # Replace with a call to your agent. Return the agent's text reply.
    raise NotImplementedError("wire this up to your agent")

@app.post("/")
def hook():
    auth = request.headers.get("Authorization", "")
    if BOT_TOKEN and auth != f"Bearer {BOT_TOKEN}":
        return jsonify(error="unauthorized"), 401
    body = request.get_json(silent=True) or {}
    prompt = body.get("prompt", "")
    session_id = body.get("session_id", "")
    text = run_my_agent(prompt, session_id)
    return jsonify(text=text)

if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8000)
