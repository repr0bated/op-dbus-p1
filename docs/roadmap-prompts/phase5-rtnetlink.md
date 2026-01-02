# Phase 5: Rtnetlink - Code Modifications

## PROMPT 5.1 - Complete Rtnetlink Operations

```
Add missing rtnetlink operations to op-network:
1. rtnetlink_add_route - add routing table entry
2. rtnetlink_delete_route - remove route
3. rtnetlink_add_neighbor - add ARP/NDP entry  
4. rtnetlink_set_mtu - change interface MTU
5. rtnetlink_create_veth - create veth pair
Implement using rtnetlink crate with proper error handling.
```

## PROMPT 5.2 - Interface Configuration Tool

```
Create 'rtnetlink_configure_interface' composite tool that:
1. Creates interface if needed (for veth, bridge types)
2. Sets IP address
3. Sets MTU
4. Brings interface up
5. Adds routes if specified
Accept full config as JSON, apply atomically.
```

## PROMPT 5.3 - Network Namespace Support

```
Add network namespace support to rtnetlink tools:
1. Accept optional 'netns' parameter
2. Execute operations in specified namespace
3. Add 'rtnetlink_list_namespaces' tool
4. Add 'rtnetlink_create_namespace' tool
5. Support moving interfaces between namespaces
Use setns() or ip-netns for namespace operations.
```



