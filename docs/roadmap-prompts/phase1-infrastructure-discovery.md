# Phase 1: Infrastructure Discovery - Code Modifications

## PROMPT 1.1 - System Inventory Tool

```
Create a new tool called 'system_inventory' in op-tools that:
1. Calls dbus_systemd_list_units to get all services
2. Calls rtnetlink_list_interfaces to get network info
3. Calls ovs_list_bridges (with graceful failure if OVS not running)
4. Calls lxc_check_available
5. Returns a consolidated JSON with sections: services, network, ovs, containers
Add it to the tool registry with category 'discovery'.
```

## PROMPT 1.2 - Health Check Aggregator

```
Add a 'system_health_check' tool that:
1. Filters systemd units for failed state
2. Checks critical services: sshd, nginx, docker
3. Identifies services enabled but not running
4. Returns health status with issues array and recommendations
Include severity levels: critical, warning, info.
```

## PROMPT 1.3 - Network Topology Mapper

```
Create 'network_topology_map' tool that:
1. Gets all interfaces via rtnetlink
2. Identifies interface types (bridge, veth, physical) from kind field
3. Maps bridge memberships from OVS data
4. Builds a topology graph as JSON with nodes and edges
Return structured data suitable for visualization.
```



