// Reference /debate endpoint for testing the Bot Council harness.
// Run: node debate-endpoint-node.js [port]
// Default port: 9000

const http = require("http");
const PORT = parseInt(process.argv[2] || "9000", 10);

function readBody(req) {
  return new Promise((resolve) => {
    const chunks = [];
    req.on("data", (c) => chunks.push(c));
    req.on("end", () => resolve(JSON.parse(Buffer.concat(chunks).toString())));
  });
}

const server = http.createServer(async (req, res) => {
  if (req.method === "POST" && req.url === "/debate") {
    const body = await readBody(req);
    let result;

    if (body.round === 0 || body.round === "0") {
      result = {
        response: `Position on: ${body.prompt.substring(0, 100)}. The key considerations are fairness, precedent, and practical enforceability.`,
      };
    } else if (body.round === "scoring") {
      result = {
        scores: (body.context || []).map((entry) => ({
          pseudonym: entry.pseudonym,
          reasoning_quality: Math.floor(Math.random() * 4) + 5,
          factual_grounding: Math.floor(Math.random() * 4) + 5,
          overall: Math.floor(Math.random() * 4) + 5,
          reasoning: `${entry.pseudonym} presents a structured argument with clear reasoning.`,
        })),
      };
    } else {
      result = { error: "unknown round" };
    }

    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify(result));
  } else {
    res.writeHead(404);
    res.end("Not found");
  }
});

server.listen(PORT, () => console.log(`Reference bot listening on port ${PORT}`));
