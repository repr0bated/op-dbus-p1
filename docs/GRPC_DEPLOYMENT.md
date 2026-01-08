# gRPC MCP Transport Deployment

## Overview

The gRPC transport provides high-performance MCP access with:
- Unary request/response (standard MCP calls)
- Server streaming (SSE-like events)
- Bidirectional streaming (full duplex)
- Run-on-connection agent support

## Quick Start

```bash
# Deploy gRPC services
chmod +x scripts/deploy-grpc.sh
./scripts/deploy-grpc.sh

# Check status
systemctl status op-mcp-grpc op-mcp-grpc-agents

# View logs
journalctl -u op-mcp-grpc -f
```

## Endpoints

| Service | Port | Description |
|---------|------|-------------|
| op-mcp-grpc | 50051 | Compact gRPC (tool discovery) |
| op-mcp-grpc-agents | 50052 | Agents gRPC (run-on-connection) |

## Build with gRPC Feature

```bash
cargo build --release -p op-mcp --features grpc
```

## Testing

```bash
# Health check with grpcurl
grpcurl -plaintext localhost:50051 op.mcp.v1.McpService/Health

# List tools
grpcurl -plaintext localhost:50051 op.mcp.v1.McpService/ListTools

# Initialize session
grpcurl -plaintext -d '{"client_name": "test"}' \
  localhost:50052 op.mcp.v1.McpService/Initialize
```

## Rust Client Usage

```rust
use op_mcp::grpc::{GrpcClient, GrpcClientConfig};

let config = GrpcClientConfig::default()
    .with_endpoint("http://localhost:50051");

let mut client = GrpcClient::connect(config).await?;

let init = client.initialize("my-client").await?;
println!("Started agents: {:?}", init.started_agents);

let result = client.call_tool("dbus_list_services", json!({})).await?;
```

## Troubleshooting

### Service won't start
```bash
journalctl -u op-mcp-grpc -n 50
/usr/local/sbin/op-mcp-server --mode grpc --grpc-port 50051 --log-level debug
```

### Port already in use
```bash
ss -tlnp | grep 50051
```
