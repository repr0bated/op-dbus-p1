#!/usr/bin/env node

/**
 * Antigravity Bridge - Local OpenAI-compatible proxy
 *
 * Forwards /v1/chat/completions to an upstream LLM endpoint that already
 * handles OAuth/enterprise auth. No API keys are required here.
 */

const http = require('http');
const https = require('https');
const { URL } = require('url');

const PORT = process.env.ANTIGRAVITY_BRIDGE_PORT || 3333;
const UPSTREAM_URL = process.env.ANTIGRAVITY_UPSTREAM_URL
  || process.env.ANTIGRAVITY_BRIDGE_URL
  || 'http://127.0.0.1:3334/v1/chat/completions';
const UPSTREAM_AUTH = process.env.ANTIGRAVITY_UPSTREAM_AUTH || '';

console.log(`Starting Antigravity Bridge on port ${PORT}`);
console.log(`Upstream: ${UPSTREAM_URL}`);

function sendJson(res, status, payload) {
  const body = JSON.stringify(payload);
  res.writeHead(status, {
    'Content-Type': 'application/json; charset=utf-8',
    'Content-Length': Buffer.byteLength(body),
  });
  res.end(body);
}

function forwardToUpstream(body, res) {
  const upstream = new URL(UPSTREAM_URL);
  const isHttps = upstream.protocol === 'https:';
  const headers = {
    'Content-Type': 'application/json',
    'Content-Length': Buffer.byteLength(body),
  };
  if (UPSTREAM_AUTH) {
    headers['Authorization'] = UPSTREAM_AUTH;
  }

  const requestOptions = {
    method: 'POST',
    hostname: upstream.hostname,
    port: upstream.port || (isHttps ? 443 : 80),
    path: upstream.pathname + upstream.search,
    headers,
  };

  const client = isHttps ? https : http;
  const upstreamReq = client.request(requestOptions, (upstreamRes) => {
    const chunks = [];
    upstreamRes.on('data', (chunk) => chunks.push(chunk));
    upstreamRes.on('end', () => {
      const responseBody = Buffer.concat(chunks);
      res.writeHead(upstreamRes.statusCode || 502, {
        'Content-Type': upstreamRes.headers['content-type'] || 'application/json',
      });
      res.end(responseBody);
    });
  });

  upstreamReq.on('error', (err) => {
    sendJson(res, 502, {
      error: {
        message: `Upstream request failed: ${err.message}`,
        type: 'upstream_error',
      },
    });
  });

  upstreamReq.write(body);
  upstreamReq.end();
}

const server = http.createServer((req, res) => {
  if (req.method === 'GET' && req.url === '/health') {
    return sendJson(res, 200, { status: 'ok', bridge: 'antigravity', upstream: UPSTREAM_URL });
  }

  if (req.method === 'POST' && req.url === '/v1/chat/completions') {
    const chunks = [];
    req.on('data', (chunk) => chunks.push(chunk));
    req.on('end', () => {
      const body = Buffer.concat(chunks).toString('utf8');
      let parsed;
      try {
        parsed = JSON.parse(body);
      } catch (err) {
        return sendJson(res, 400, {
          error: { message: `Invalid JSON: ${err.message}`, type: 'bad_request' },
        });
      }
      console.log(`Request: ${parsed.messages?.length || 0} messages, model: ${parsed.model}`);
      forwardToUpstream(body, res);
    });
    return;
  }

  res.writeHead(404, { 'Content-Type': 'application/json; charset=utf-8' });
  res.end(JSON.stringify({ error: { message: 'Not found', type: 'not_found' } }));
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`Antigravity Bridge listening on http://127.0.0.1:${PORT}`);
});

process.on('SIGINT', () => {
  console.log('\nShutting down Antigravity Bridge...');
  server.close(() => process.exit(0));
});

process.on('SIGTERM', () => {
  console.log('\nShutting down Antigravity Bridge...');
  server.close(() => process.exit(0));
});
