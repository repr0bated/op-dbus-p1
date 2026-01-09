import * as http from 'http';
import * as vscode from 'vscode';

let server: http.Server | undefined;

type OpenAIMessage = {
  role: string;
  content: string;
};

type OpenAIRequest = {
  model?: string;
  messages?: OpenAIMessage[];
};

function sendJson(res: http.ServerResponse, status: number, payload: unknown) {
  const body = JSON.stringify(payload);
  res.writeHead(status, {
    'Content-Type': 'application/json; charset=utf-8',
    'Content-Length': Buffer.byteLength(body),
  });
  res.end(body);
}

function getConfig() {
  const config = vscode.workspace.getConfiguration('opDbusBridge');
  return {
    host: config.get<string>('host', '127.0.0.1'),
    port: config.get<number>('port', 3333),
    modelFamily: config.get<string>('modelFamily', ''),
    fallbackCommand: config.get<string>('fallbackCommand', 'cursor.chat.sendMessage'),
  };
}

async function readBody(req: http.IncomingMessage): Promise<string> {
  return await new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    req.on('data', (chunk) => chunks.push(Buffer.from(chunk)));
    req.on('end', () => resolve(Buffer.concat(chunks).toString('utf8')));
    req.on('error', reject);
  });
}

function prepareMessages(messages: OpenAIMessage[]) {
  const systemChunks: string[] = [];
  const chatMessages: { role: 'user' | 'assistant'; content: string }[] = [];

  for (const msg of messages) {
    if (msg.role === 'system') {
      systemChunks.push(msg.content);
      continue;
    }
    if (msg.role === 'assistant') {
      chatMessages.push({ role: 'assistant', content: msg.content });
    } else {
      chatMessages.push({ role: 'user', content: msg.content });
    }
  }

  const systemPrefix = systemChunks.join('\n\n').trim();
  if (systemPrefix && chatMessages.length > 0) {
    chatMessages[0] = {
      role: chatMessages[0].role,
      content: `${systemPrefix}\n\n${chatMessages[0].content}`,
    };
  }

  return chatMessages;
}

async function sendViaLanguageModel(messages: OpenAIMessage[], modelFamily: string) {
  const vscodeAny = vscode as any;
  const lm = vscodeAny.lm;
  if (!lm || typeof lm.selectChatModels !== 'function') {
    return undefined;
  }

  const selector = modelFamily ? { family: modelFamily } : {};
  const models = await lm.selectChatModels(selector);
  if (!models || models.length === 0) {
    return undefined;
  }

  const chatModel = models[0];
  const prepared = prepareMessages(messages);
  const chatMessages = prepared.map((msg) => {
    if (msg.role === 'assistant') {
      return vscodeAny.LanguageModelChatMessage.Assistant(msg.content);
    }
    return vscodeAny.LanguageModelChatMessage.User(msg.content);
  });

  const response = await chatModel.sendRequest(chatMessages, {});
  let content = '';
  for await (const chunk of response.text) {
    content += chunk;
  }
  return content.trim();
}

async function sendViaFallbackCommand(command: string, messages: OpenAIMessage[]) {
  const lastUser = [...messages].reverse().find((msg) => msg.role === 'user');
  const prompt = lastUser?.content || 'Hello';
  const result = await vscode.commands.executeCommand(command, prompt);
  if (typeof result === 'string') {
    return result.trim();
  }
  if (result !== undefined && result !== null) {
    return String(result).trim();
  }
  return undefined;
}

async function handleChatRequest(request: OpenAIRequest, config: ReturnType<typeof getConfig>) {
  const messages = Array.isArray(request.messages) ? request.messages : [];
  if (messages.length === 0) {
    throw new Error('No messages provided');
  }

  const model = request.model || 'ide-model';

  let content = await sendViaLanguageModel(messages, config.modelFamily);
  if (!content) {
    content = await sendViaFallbackCommand(config.fallbackCommand, messages);
  }

  if (!content) {
    throw new Error('No IDE model available to handle request');
  }

  return {
    id: `bridge-${Date.now()}`,
    object: 'chat.completion',
    created: Math.floor(Date.now() / 1000),
    model,
    choices: [
      {
        index: 0,
        message: {
          role: 'assistant',
          content,
        },
        finish_reason: 'stop',
      },
    ],
    usage: {
      prompt_tokens: Math.ceil(content.length / 4),
      completion_tokens: Math.ceil(content.length / 4),
      total_tokens: Math.ceil(content.length / 2),
    },
  };
}

function startServer() {
  if (server) {
    return;
  }

  const config = getConfig();
  server = http.createServer(async (req, res) => {
    try {
      if (req.method === 'GET' && req.url === '/health') {
        return sendJson(res, 200, {
          status: 'ok',
          bridge: 'op-dbus-antigravity',
        });
      }

      if (req.method === 'POST' && req.url === '/v1/chat/completions') {
        const body = await readBody(req);
        const parsed = JSON.parse(body) as OpenAIRequest;
        const response = await handleChatRequest(parsed, config);
        return sendJson(res, 200, response);
      }

      sendJson(res, 404, { error: { message: 'Not found', type: 'not_found' } });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      sendJson(res, 500, { error: { message, type: 'bridge_error' } });
    }
  });

  server.listen(config.port, config.host, () => {
    vscode.window.showInformationMessage(
      `OP-DBUS Antigravity Bridge listening on http://${config.host}:${config.port}`,
    );
  });
}

function stopServer() {
  if (!server) {
    return;
  }
  server.close();
  server = undefined;
  vscode.window.showInformationMessage('OP-DBUS Antigravity Bridge stopped.');
}

export function activate(context: vscode.ExtensionContext) {
  context.subscriptions.push(
    vscode.commands.registerCommand('op-dbus-bridge.start', () => startServer()),
    vscode.commands.registerCommand('op-dbus-bridge.stop', () => stopServer()),
  );

  startServer();
}

export function deactivate() {
  stopServer();
}
