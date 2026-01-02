# Phase 2: D-Bus/Systemd - Code Modifications

## PROMPT 2.1 - Batch Service Operations

```
Add 'dbus_systemd_batch_operation' tool that:
1. Accepts array of units and operation (start/stop/restart)
2. Executes operations in sequence with error handling
3. Returns results for each unit (success/failure/error message)
4. Supports 'check_first' option to skip if already in desired state
Add rollback capability if any operation fails.
```

## PROMPT 2.2 - Service Dependency Resolver

```
Create 'dbus_systemd_get_dependencies' tool that:
1. Takes a unit name
2. Queries Requires, Wants, After, Before properties via D-Bus
3. Builds dependency tree (recursive option)
4. Returns structured dependency graph
Use org.freedesktop.systemd1.Unit interface.
```

## PROMPT 2.3 - Journal Log Reader

```
Add 'dbus_systemd_read_journal' tool that:
1. Takes unit name and optional line count (default 50)
2. Uses journalctl via shell_execute or native journal API
3. Parses and returns structured log entries with timestamps
4. Supports filtering by priority level
Return JSON array of log entries.
```



