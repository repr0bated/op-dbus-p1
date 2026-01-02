# Phase 8: Monitoring & Operations - Code Modifications

## PROMPT 8.1 - Metrics Collection

```
Add metrics collection to op-tools:
1. Create MetricsStore in op-core for time-series data
2. Collect: flow stats, interface stats, service states
3. Add 'metrics_query' tool with time range support
4. Calculate rates, averages, percentiles
5. Support Prometheus exposition format export
Store 1 hour of data by default.
```

## PROMPT 8.2 - Alert System

```
Create alerting system:
1. Define AlertRule struct with condition, threshold, severity
2. Add 'alerts_configure' tool to set up rules
3. Add 'alerts_check' tool to evaluate all rules
4. Built-in rules: service_down, high_drop_rate, interface_down
5. Alert state tracking (firing, resolved)
Integrate with respond_to_user for notifications.
```

## PROMPT 8.3 - Operational Runbooks

```
Add runbook execution system:
1. Define Runbook struct with steps (tool calls + conditions)
2. Add 'runbook_execute' tool to run automated procedures
3. Built-in runbooks: restart_privacy_chain, flush_flows, health_check
4. Support dry-run mode
5. Log all runbook executions with results
Store runbooks in crates/op-tools/src/runbooks/
```



