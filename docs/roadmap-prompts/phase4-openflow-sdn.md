# Phase 4: OpenFlow/SDN - Code Modifications

## PROMPT 4.1 - OpenFlow Rule Builder

```
Create 'openflow_build_rule' helper in op-network that:
1. Accepts match criteria as structured JSON (in_port, dl_src, dl_dst, nw_src, nw_dst, etc.)
2. Accepts actions as array (output, drop, mod_dl_dst, push_vlan, etc.)
3. Validates rule syntax before sending
4. Returns OVS-compatible flow string
Support OpenFlow 1.3 match fields and actions.
```

## PROMPT 4.2 - Flow Rule Templates

```
Add 'openflow_apply_template' tool with predefined templates:
1. 'privacy_chain' - creates bidirectional flow chain between 3 ports
2. 'vlan_trunk' - VLAN tagging/untagging rules
3. 'load_balance' - round-robin across output ports
4. 'acl_block' - drop traffic matching criteria
Templates accept parameters and generate multiple flow rules.
```

## PROMPT 4.3 - Flow Monitoring

```
Create 'openflow_monitor_flows' tool that:
1. Dumps flows with statistics
2. Compares with previous dump to calculate deltas
3. Identifies flows with packet count changes
4. Flags flows with high drop rates or errors
Store state between calls for delta calculation.
```



