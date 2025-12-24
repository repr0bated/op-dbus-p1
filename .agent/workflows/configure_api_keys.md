---
description: How to configure API keys for op-chat-server
---

The `op-chat-server` uses environment variables to configure LLM providers.
These variables are loaded from `/etc/default/op-chat-server`.

To configure API keys:

1. Edit the configuration file:
   ```bash
   sudo nano /etc/default/op-chat-server
   ```

2. Add your API keys:
   ```ini
   GEMINI_API_KEY=your_gemini_key_here
   ANTHROPIC_API_KEY=your_anthropic_key_here
   PERPLEXITY_API_KEY=your_perplexity_key_here
   HF_TOKEN=your_huggingface_token_here
   ```

3. Restart the service:
   ```bash
   sudo systemctl restart op-chat-server
   ```

4. Check logs to verify provider initialization:
   ```bash
   sudo journalctl -u op-chat-server -n 50 | grep "provider initialized"
   ```
