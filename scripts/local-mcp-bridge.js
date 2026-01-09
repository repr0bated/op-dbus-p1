#!/usr/bin/env node

/**
 * Local MCP Bridge for Antigravity
 *
 * Simple MCP server that provides AI capabilities
 * Acts as backend for Antigravity provider
 */

const express = require('express');
const app = express();
const PORT = 3334; // Different port from the main bridge

console.log(`🚀 Starting Local MCP Bridge on port ${PORT}`);

// Middleware
app.use(express.json());

// Health check
app.get('/health', (req, res) => {
  res.json({ status: 'ok', server: 'local-mcp-bridge' });
});

// MCP JSON-RPC endpoint
app.post('/', async (req, res) => {
  const mcpRequest = req.body;
  console.log(`📨 MCP Request: ${mcpRequest.method}`);

  try {
    let result;

    switch (mcpRequest.method) {
      case 'tools/list':
        result = {
          tools: [{
            name: 'chat',
            description: 'AI chat with enterprise billing',
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
        break;

      case 'tools/call':
        const { message, model = 'claude-3-5-sonnet' } = mcpRequest.params.arguments || {};

        // Simulate AI response (replace with real AI call later)
        const responses = [
          `Hello! I'm responding via enterprise billing through Antigravity. You said: "${message}"`,
          `Thanks for your message: "${message}". This is using enterprise AI access!`,
          `I received your message: "${message}". Enterprise billing active!`,
          `Your message "${message}" was processed through Antigravity's enterprise AI system.`
        ];

        const response = responses[Math.floor(Math.random() * responses.length)];

        result = {
          content: [{
            type: 'text',
            text: response
          }]
        };
        break;

      default:
        throw new Error(`Unknown method: ${mcpRequest.method}`);
    }

    res.json({
      jsonrpc: '2.0',
      id: mcpRequest.id,
      result: result
    });

  } catch (error) {
    console.error('❌ MCP Error:', error.message);
    res.status(500).json({
      jsonrpc: '2.0',
      id: mcpRequest.id,
      error: {
        code: -32603,
        message: error.message
      }
    });
  }
});

// Start server
app.listen(PORT, '127.0.0.1', () => {
  console.log(`🎯 Local MCP Bridge listening on http://127.0.0.1:${PORT}`);
  console.log(`💰 Enterprise billing simulation active`);
  console.log(`🔧 Replace AI responses with real enterprise API calls`);
});

// Graceful shutdown
process.on('SIGINT', () => {
  console.log('\n👋 Shutting down Local MCP Bridge...');
  process.exit(0);
});

process.on('SIGTERM', () => {
  console.log('\n👋 Shutting down Local MCP Bridge...');
  process.exit(0);
});