#!/usr/bin/env node

/**
 * Simple MCP Bridge for Antigravity
 * No external dependencies - uses built-in Node.js HTTP
 */

const http = require('http');

const PORT = 3334;
console.log(`🚀 Starting Simple MCP Bridge on port ${PORT}`);

function handleRequest(req, res) {
  // Set CORS headers
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
  res.setHeader('Access-Control-Allow-Headers', 'Content-Type');

  if (req.method === 'OPTIONS') {
    res.writeHead(200);
    res.end();
    return;
  }

  if (req.method === 'GET' && req.url === '/health') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ status: 'ok', server: 'simple-mcp-bridge' }));
    return;
  }

  if (req.method === 'POST' && req.url === '/') {
    let body = '';
    req.on('data', chunk => body += chunk);
    req.on('end', () => {
      try {
        const mcpRequest = JSON.parse(body);
        console.log(`📨 MCP Request: ${mcpRequest.method}`);

        let result;

        if (mcpRequest.method === 'tools/list') {
          result = {
            tools: [{
              name: 'chat',
              description: 'AI chat with enterprise billing through Antigravity',
              inputSchema: {
                type: 'object',
                properties: {
                  message: { type: 'string' },
                  model: { type: 'string', default: 'claude-3-5-sonnet' },
                  session_id: { type: 'string' }
                },
                required: ['message']
              }
            }]
          };
        } else if (mcpRequest.method === 'tools/call') {
          const { message } = mcpRequest.params?.arguments || {};

          // Enterprise AI response simulation
          const responses = [
            `🎯 Enterprise AI Response: Hello! Your message "${message}" is being processed through Antigravity's enterprise billing. No API charges apply!`,
            `💰 Cost-free AI: Thanks for "${message}". This response is covered by your enterprise Code Assist subscription.`,
            `🏢 Enterprise Mode: I received "${message}" through Antigravity's authenticated enterprise channel. Billing is handled automatically.`,
            `✅ Zero Charges: Your message "${message}" processed via enterprise API - no individual costs incurred.`
          ];

          const response = responses[Math.floor(Math.random() * responses.length)];

          result = {
            content: [{
              type: 'text',
              text: response
            }]
          };
        } else {
          throw new Error(`Unknown method: ${mcpRequest.method}`);
        }

        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          jsonrpc: '2.0',
          id: mcpRequest.id,
          result: result
        }));

      } catch (error) {
        console.error('❌ Error:', error.message);
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          jsonrpc: '2.0',
          id: null,
          error: {
            code: -32603,
            message: error.message
          }
        }));
      }
    });
    return;
  }

  res.writeHead(404);
  res.end();
}

const server = http.createServer(handleRequest);

server.listen(PORT, '127.0.0.1', () => {
  console.log(`🎯 Simple MCP Bridge listening on http://127.0.0.1:${PORT}`);
  console.log(`💰 Enterprise billing simulation active`);
  console.log(`🔧 Ready for Antigravity provider connection`);
});

// Graceful shutdown
process.on('SIGINT', () => {
  console.log('\n👋 Shutting down Simple MCP Bridge...');
  server.close(() => {
    console.log('✅ Bridge stopped');
    process.exit(0);
  });
});

process.on('SIGTERM', () => {
  console.log('\n👋 Shutting down Simple MCP Bridge...');
  server.close(() => {
    console.log('✅ Bridge stopped');
    process.exit(0);
  });
});