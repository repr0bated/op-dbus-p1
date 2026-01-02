# Chatbot Testing Prompts

Prompts to give to the op-web chatbot to test and achieve each roadmap milestone.

---

## Phase 1: Infrastructure Discovery

### 1.1 System Inventory
```
List all running systemd services and show their status. Also list all network interfaces on this system.
```

### 1.2 Network Discovery
```
Show me all network interfaces, their IP addresses, and identify which ones are bridges vs physical interfaces.
```

### 1.3 OVS Status Check
```
Check if Open vSwitch is running. If it is, list all OVS bridges and their ports.
```

### 1.4 Container Discovery
```
Check if LXC/LXD is available on this system. If so, list all containers and their current state.
```

---

## Phase 2: D-Bus/Systemd Integration

### 2.1 Service Status
```
Get the detailed status of the sshd service including whether it's enabled and running.
```

### 2.2 Service Control
```
Restart the nginx service and confirm it started successfully.
```

### 2.3 Multiple Services
```
Check the status of these services: sshd, nginx, docker. Tell me which are running and which are stopped.
```

### 2.4 Service Dependencies
```
What services does docker.service depend on? Show me its dependency chain.
```

---

## Phase 3: OVS/OVSDB Integration

### 3.1 Create Bridge
```
Create a new OVS bridge called 'test-br0'.
```

### 3.2 Add Ports
```
Add a port called 'test-port1' to the bridge 'test-br0' as an internal port.
```

### 3.3 List Configuration
```
Show me the complete configuration of bridge 'test-br0' including all its ports.
```

### 3.4 Cleanup
```
Delete the bridge 'test-br0' and all its ports.
```

---

## Phase 4: OpenFlow/SDN Programming

### 4.1 List Flows
```
List all OpenFlow rules on bridge 'ovs-br0'.
```

### 4.2 Add Simple Flow
```
Add an OpenFlow rule to bridge 'ovs-br0' that forwards all traffic from port 1 to port 2.
```

### 4.3 Add Bidirectional Flow
```
Create bidirectional flow rules between ports 1 and 2 on bridge 'ovs-br0'.
```

### 4.4 Priority Rules
```
Add a high-priority OpenFlow rule to drop all traffic from MAC address 00:11:22:33:44:55 on bridge 'ovs-br0'.
```

### 4.5 Flow Cleanup
```
Delete all OpenFlow rules from bridge 'ovs-br0'.
```

---

## Phase 5: Rtnetlink/Kernel Networking

### 5.1 Interface Details
```
Show detailed information about interface 'eth0' including its IP addresses and MTU.
```

### 5.2 Create Veth Pair
```
Create a veth pair called 'veth0' and 'veth1'.
```

### 5.3 Set IP Address
```
Assign IP address 10.0.0.1/24 to interface 'veth0' and bring it up.
```

### 5.4 Routing
```
Show the current routing table and add a route for 192.168.100.0/24 via gateway 10.0.0.254.
```

---

## Phase 6: LXC/Container Integration

### 6.1 Create Container
```
Create a new LXC container called 'test-container' using the Ubuntu 22.04 image.
```

### 6.2 Container Networking
```
Configure the container 'test-container' to use OVS bridge 'ovs-br0' for its network interface.
```

### 6.3 Start Container
```
Start the container 'test-container' and verify it's running.
```

### 6.4 Container Status
```
Show the status and network configuration of container 'test-container'.
```

### 6.5 Cleanup
```
Stop and delete the container 'test-container'.
```

---

## Phase 7: Full Topology Deployment

### 7.1 Privacy Chain Setup - Step by Step
```
I want to set up a privacy chain with WireGuard -> WARP -> XRay. First, create an OVS bridge called 'privacy-br0' for this topology.
```

### 7.2 Create Containers
```
Create three LXC containers for the privacy chain:
1. 'priv-wg' for WireGuard
2. 'priv-warp' for Cloudflare WARP
3. 'priv-xray' for XRay proxy
```

### 7.3 Connect to Bridge
```
Connect all three containers (priv-wg, priv-warp, priv-xray) to the 'privacy-br0' OVS bridge.
```

### 7.4 Configure Flow Chain
```
Set up OpenFlow rules on 'privacy-br0' to chain traffic:
- Incoming traffic goes to priv-wg
- priv-wg output goes to priv-warp
- priv-warp output goes to priv-xray
- priv-xray output goes to external
```

### 7.5 Full Deployment (Advanced)
```
Deploy a complete privacy chain topology with:
- OVS bridge 'privacy-br0'
- Three containers: priv-wg (WireGuard), priv-warp (WARP), priv-xray (XRay)
- All containers connected to the bridge
- OpenFlow rules to chain traffic: WireGuard -> WARP -> XRay -> external
- Verify all components are running
```

---

## Phase 8: Monitoring & Operations

### 8.1 System Health
```
Give me a health check of the system - list any failed services, down interfaces, or other issues.
```

### 8.2 Network Statistics
```
Show me traffic statistics for all interfaces on bridge 'privacy-br0'.
```

### 8.3 Flow Statistics
```
Show OpenFlow statistics for bridge 'privacy-br0' - which rules are matching traffic?
```

### 8.4 Container Health
```
Check the health of all containers in the privacy chain - are they running and reachable?
```

### 8.5 Full Status Report
```
Give me a complete status report of the privacy chain topology including:
- Bridge status
- Container states
- Flow rules and their packet counts
- Any errors or warnings
```

---

## Quick Validation Prompts

### Minimal Test
```
List all systemd services and all network interfaces.
```

### OVS Test
```
Create bridge 'test-br', add port 'test-p1', list the bridge config, then delete the bridge.
```

### Full Stack Test
```
Check if OVS, LXC, and systemd are all working. Create a test container, attach it to an OVS bridge, verify connectivity, then clean up.
```



