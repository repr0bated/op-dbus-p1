# AI Bootstrap Prompt for op-dbus-v2

Use this prompt to onboard an AI assistant (Aye Chat, Claude, ChatGPT, etc.) to the project.

---

## The Prompt

```
I'm working on op-dbus-v2, a Rust-based system administration platform. Here's the context:

## Project Overview

op-dbus-v2 is a modular Rust workspace that provides:
1. **Natural language chatbot** for system administration
2. **Direct tool access** via D-Bus, OVSDB, rtnetlink, and LXC APIs
3. **MCP (Model Context Protocol)** endpoint for external AI clients
4. **Web UI** at http://localhost:3000 with chat interface

## Architecture

- Pure Rust backend (no Node.js)
- Cargo workspace with crates: op-core, op-tools, op-chat, op-llm, op-web, op-network, op-mcp
- LLM integration via Gemini (primary), with Anthropic/HuggingFace support
- Forced tool execution - chatbot must use tools, cannot hallucinate CLI commands

## Goal: Privacy Chain Topology

The end goal is a chatbot that can deploy and manage a "privacy chain" network topology:

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│   priv_wg   │───▶│  priv_warp  │───▶│  priv_xray  │
│ (WireGuard) │    │   (WARP)    │    │   (XRay)    │
└─────────────┘    └─────────────┘    └─────────────┘
       │                 │                   │
       └─────────────────┴───────────────────┘
                      ovs-br0 (OVS Bridge)
```

Traffic flow: User → WireGuard → Cloudflare WARP → XRay → Internet

## Components Used

- **Open vSwitch (OVS)**: Software-defined networking bridge
- **OpenFlow**: Flow rules to chain traffic between containers
- **LXC/LXD**: Lightweight containers for each service
- **Systemd**: Service management via D-Bus
- **Rtnetlink**: Kernel network interface configuration

## Current Tools Available

The chatbot has these tool categories:
- `dbus_systemd_*` - Service management (list, start, stop, restart, status)
- `ovs_*` - OVS operations (create/delete bridge, add/remove port, list)
- `openflow_*` - Flow rule management (add, delete, list flows)
- `rtnetlink_*` - Network interfaces (list, create veth, set IP, bring up/down)
- `lxc_*` - Container management (list, create, start, stop, delete)
- `file_*` - File operations (read, write, list directory)

## What I Need Help With

[INSERT YOUR SPECIFIC REQUEST HERE]

Examples:
- "Help me implement the topology reconciler that compares current state to desired state"
- "Review the OVS tool implementations and suggest improvements"
- "Help debug why the chatbot isn't executing multiple tools in sequence"
- "Design the data structures for topology specification"

## Key Files

- `crates/op-tools/src/` - Tool implementations
- `crates/op-web/src/orchestrator.rs` - Chat orchestration with multi-turn execution
- `crates/op-network/src/` - OVS and rtnetlink clients
- `crates/op-chat/src/nl_admin.rs` - Natural language processing
- `docs/roadmap-prompts/` - Implementation prompts for each phase
```

---

## Quick Version (for context-limited chats)

```
I'm building op-dbus-v2, a Rust chatbot for Linux system administration.

Goal: Deploy a "privacy chain" topology via natural language:
- OVS bridge connecting 3 LXC containers
- WireGuard → WARP → XRay traffic chain
- OpenFlow rules for packet steering

Tech stack:
- Rust workspace with crates for tools, chat, LLM, web
- D-Bus for systemd, OVSDB for OVS, rtnetlink for networking, LXC API for containers
- Gemini LLM with forced tool execution (no CLI hallucination)
- Web UI at localhost:3000

Current state: Basic tools work. Need to implement topology orchestration.

[YOUR REQUEST HERE]
```

---

## Roadmap Summary

| Phase | Goal | Status |
|-------|------|--------|
| 1 | Infrastructure Discovery | ✅ Basic tools work |
| 2 | D-Bus/Systemd Integration | ✅ Core operations |
| 3 | OVS/OVSDB Integration | 🟡 Needs transactions |
| 4 | OpenFlow/SDN Programming | 🟡 Needs templates |
| 5 | Rtnetlink/Kernel Networking | 🟡 Needs routes/namespaces |
| 6 | LXC/Container Integration | 🟡 Needs OVS attachment |
| 7 | Topology Orchestration | ❌ Not started |
| 8 | Monitoring & Operations | ❌ Not started |

---

## Repository Structure

```
op-dbus-v2/
├── crates/
│   ├── op-core/          # Foundation traits, types
│   ├── op-tools/         # Tool registry and implementations
│   ├── op-chat/          # NL processing, tool call parsing
│   ├── op-llm/           # LLM providers (Gemini, Anthropic)
│   ├── op-web/           # HTTP server, WebSocket, orchestrator
│   ├── op-network/       # OVS client, rtnetlink helpers
│   ├── op-mcp/           # MCP protocol for external clients
│   └── op-lxc/           # LXC container operations
├── docs/
│   └── roadmap-prompts/  # Implementation and testing prompts
├── Cargo.toml            # Workspace definition
└── DEVELOPMENT-RULES.md  # Coding standards
```

---

## Anti-Patterns to Avoid

The chatbot enforces these rules:
1. **No CLI hallucination** - Must use tools, not suggest `ovs-vsctl` or `ip` commands
2. **No sudo** - Runs as root, sudo causes failures
3. **No NetworkManager** - Direct protocol access only
4. **No Docker/Podman** - LXC containers only
5. **Multi-turn execution** - Complex tasks need multiple tool calls



