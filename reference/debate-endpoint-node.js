const http = require('http');

const server = http.createServer((req, res) => {
  if (req.method !== 'POST' || req.url !== '/debate') {
    res.writeHead(404, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ error: 'not found' }));
    return;
  }

  let body = '';
  req.on('data', chunk => { body += chunk; });
  req.on('end', () => {
    const data = JSON.parse(body);
    const round = data.round;
    const role = data.role || 'unknown';

    let response;

    if (round === 0) {
      response = {
        response: `[${role}] My initial position on this topic is that it requires careful analysis from multiple angles.`,
      };
    } else if (round === 1) {
      response = {
        response: 'The strongest opposing argument comes from the empirical evidence cited. I would change my position if presented with verified data contradicting my core claim.',
        confidence: 65,
      };
    } else if (round === 2) {
      response = {
        response: 'I challenge the assumption that the evidence presented is conclusive. The methodology has significant gaps.',
        confidence: 70,
        challenge: {
          claim_targeted: 'The claim that the evidence is conclusive',
          counter_evidence: 'The sample size is insufficient and the control group was not properly isolated.',
          type: 'factual',
        },
      };
    } else if (round === 3) {
      response = {
        response: 'What assumption does your position rely on that, if false, would invalidate your entire argument? The core assumption is falsifiable through longitudinal study.',
        confidence: 68,
      };
    } else if (round === 4) {
      response = {
        response: 'My final position remains that careful empirical analysis is required. The debate has refined but not fundamentally altered my view.',
        confidence: 72,
        position_change: {
          changed: false,
          from_summary: 'Careful analysis required',
          to_summary: 'Careful analysis required, with refined methodology criteria',
          reason: 'The opposing arguments raised valid methodological concerns but did not undermine the core thesis.',
        },
      };
    } else if (round === 'scoring') {
      const scores = (data.context || []).map(entry => ({
        pseudonym: entry.pseudonym,
        reasoning_quality: 7,
        factual_grounding: 6,
        overall: 7,
        reasoning: 'Solid argument with room for improvement.',
      }));
      response = { scores };
    } else {
      response = { response: 'Unknown round', confidence: 50 };
    }

    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(response));
  });
});

server.listen(3200, () => console.log('Reference bot listening on :3200'));
