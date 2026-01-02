# Phase 6: LXC Integration - Code Modifications

## PROMPT 6.1 - LXC Profile Management

```
Add LXC profile tools:
1. 'lxc_list_profiles' - list available profiles
2. 'lxc_get_profile' - get profile configuration
3. 'lxc_create_profile' - create new profile with devices/config
4. 'lxc_apply_profile' - apply profile to container
Create 'ovs-bridge' profile template for OVS attachment.
```

## PROMPT 6.2 - Container Network Attachment

```
Create 'lxc_attach_ovs' tool that:
1. Gets container's eth0 host-side veth name
2. Adds veth to specified OVS bridge
3. Optionally sets VLAN tag
4. Optionally creates OVS port with specific name
5. Verifies attachment via ovs_list_ports
Handle running vs stopped container states.
```

## PROMPT 6.3 - Container Lifecycle Orchestration

```
Add 'lxc_orchestrate' tool for multi-container operations:
1. Accept array of container specs with dependencies
2. Create containers in dependency order
3. Configure networking for each
4. Start in correct order
5. Health check each container
6. Rollback on failure
Return orchestration results with timing.
```



