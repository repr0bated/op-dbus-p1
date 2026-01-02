# Phase 3: OVS/OVSDB - Code Modifications

## PROMPT 3.1 - OVS Transaction Support

```
Enhance OvsdbClient in op-network to support:
1. Multi-operation transactions (begin/commit/rollback)
2. Add 'ovs_transaction' tool that accepts array of operations
3. Execute atomically - all succeed or all fail
4. Return transaction ID and results
Operations: create_bridge, add_port, set_option, delete_port.
```

## PROMPT 3.2 - OVS Port Types

```
Add support for different port types in ovs_add_port:
1. 'internal' - OVS internal port
2. 'system' - system interface attachment
3. 'patch' - patch port to another bridge
4. 'vxlan' - VXLAN tunnel port with remote_ip option
5. 'gre' - GRE tunnel port
Update OVSDB operations to set Interface type correctly.
```

## PROMPT 3.3 - OVS Statistics Tool

```
Create 'ovs_get_statistics' tool that:
1. Queries Interface table for statistics columns
2. Gets rx_bytes, tx_bytes, rx_packets, tx_packets, rx_errors, tx_errors
3. Supports per-port or aggregate bridge stats
4. Calculates rates if called with 'include_rates' and previous data
Return structured statistics with timestamps.
```



