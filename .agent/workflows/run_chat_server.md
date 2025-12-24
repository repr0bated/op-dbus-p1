---
description: How to run the op-chat-server
---

1. Build the server:
   ```bash
   cargo build -p op-chat-server
   ```

2. Run the server:
   ```bash
   cargo run -p op-chat-server
   ```

3. Check help:
   ```bash
   cargo run -p op-chat-server -- --help
   ```

4. The server runs on port 8080 by default. You can change it with `--port`.
