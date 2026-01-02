# Phase 7: Topology Orchestration - Code Modifications

## PROMPT 7.1 - Topology Definition Schema

```
Create topology definition system in op-tools:
1. Define TopologySpec struct with bridges, ports, flows, containers
2. Add 'topology_validate' tool to check spec syntax
3. Add 'topology_diff' to compare current vs desired state
4. Store topology specs as YAML/JSON files
5. Support topology versioning
Create schema in crates/op-tools/src/topology/mod.rs
```

## PROMPT 7.2 - Topology Reconciler

```
Create 'topology_apply' tool that:
1. Loads topology spec (from file or inline JSON)
2. Discovers current state using existing tools
3. Calculates required changes (create/modify/delete)
4. Applies changes in correct order (bridges→ports→flows→containers)
5. Verifies final state matches spec
6. Reports drift if any
Implement idempotent reconciliation loop.
```

## PROMPT 7.3 - Privacy Chain Blueprint

```
Create built-in 'privacy_chain' topology blueprint:
1. Hardcode the WG→WARP→XRay chain architecture
2. Accept parameters: bridge_name, subnet, container_images
3. Generate full topology spec from parameters
4. Integrate with topology_apply for one-command deployment
Add as 'deploy_privacy_chain' tool with sensible defaults.
```



