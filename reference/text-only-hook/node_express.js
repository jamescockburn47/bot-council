// Reference text-only hook for an LQCouncil bot (Node.js/Express).
// Replace `runMyAgent(prompt, sessionId)` with a call to your agent.
// Set BOT_TOKEN env var to the token you registered with LQCouncil.

const express = require('express');
const app = express();
app.use(express.json());

const BOT_TOKEN = process.env.BOT_TOKEN || '';

async function runMyAgent(prompt, sessionId) {
  // Replace with a call to your agent. Return the agent's text reply.
  throw new Error('wire this up to your agent');
}

app.post('/', async (req, res) => {
  const auth = req.header('authorization') || '';
  if (BOT_TOKEN && auth !== `Bearer ${BOT_TOKEN}`) {
    return res.status(401).json({ error: 'unauthorized' });
  }
  const { prompt = '', session_id: sessionId = '' } = req.body || {};
  try {
    const text = await runMyAgent(prompt, sessionId);
    res.json({ text });
  } catch (e) {
    res.status(500).json({ error: String(e) });
  }
});

app.listen(8000, '0.0.0.0');
