# MCP Proxy Tool Exposure

The MCP proxy surfaces tools from the daemon's tool registry. With the skill
registry tools registered in `op-dbus-service`, MCP clients now see and can
invoke the orchestration skill helpers through the existing `tools/list` and
`tools/call` MCP requests.

No changes to the proxy protocol are required; the proxy continues to forward
requests to the daemon's gRPC MCP service.
