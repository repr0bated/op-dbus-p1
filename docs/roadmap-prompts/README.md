# op-dbus-v2 Roadmap Prompts

Three types of prompts:
1. **AI Bootstrap Prompt** - Onboard any AI to the project ([ai-bootstrap-prompt.md](ai-bootstrap-prompt.md))
2. **Code Modification Prompts** - For AI to implement features (phase*.md)
3. **Chatbot Testing Prompts** - For testing the chatbot UI ([chatbot-testing-prompts.md](chatbot-testing-prompts.md))

## Implementation Order

### Critical Path (Minimum Viable)

1. **Phase 3.1** - OVS transactions (foundation)
2. **Phase 4.1** - OpenFlow rule builder
3. **Phase 5.1** - Complete rtnetlink ops
4. **Phase 7.1** - Topology schema
5. **Phase 7.2** - Topology reconciler  
6. **Phase 7.3** - Privacy chain blueprint

### Extended Features

7. **Phase 6.2** - Container OVS attachment
8. **Phase 2.1** - Batch service operations
9. **Phase 8.1** - Metrics collection

## Phases

| Phase | Focus | File |
|-------|-------|------|
| 1 | Infrastructure Discovery | [phase1-infrastructure-discovery.md](phase1-infrastructure-discovery.md) |
| 2 | D-Bus/Systemd Integration | [phase2-dbus-systemd.md](phase2-dbus-systemd.md) |
| 3 | OVS/OVSDB Integration | [phase3-ovs-ovsdb.md](phase3-ovs-ovsdb.md) |
| 4 | OpenFlow/SDN Programming | [phase4-openflow-sdn.md](phase4-openflow-sdn.md) |
| 5 | Rtnetlink/Kernel Networking | [phase5-rtnetlink.md](phase5-rtnetlink.md) |
| 6 | LXC/Container Integration | [phase6-lxc-integration.md](phase6-lxc-integration.md) |
| 7 | Topology Orchestration | [phase7-topology-orchestration.md](phase7-topology-orchestration.md) |
| 8 | Monitoring & Operations | [phase8-monitoring-operations.md](phase8-monitoring-operations.md) |

## Usage

Copy a prompt and give it to the AI coding assistant (Cursor, etc.) to implement that feature.

Example:
```
@workspace Implement Phase 3.1 - OVS Transaction Support

[paste prompt content here]
```

## Target Architecture

```
Privacy Chain: WireGuard → WARP → XRay

┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│   priv_wg   │───▶│  sock_warp  │───▶│  priv_xray  │
│ (WireGuard) │    │   (WARP)    │    │   (XRay)    │
└─────────────┘    └─────────────┘    └─────────────┘
     │                   │                   │
     └───────────────────┴───────────────────┘
                    ovs-br0
```

