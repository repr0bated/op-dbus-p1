# Remove Lazy Initialization

Skill tooling is now initialized eagerly during service startup in
`op-dbus-service`. The `SkillRegistry` is constructed with its default skills
and shared via an `Arc<RwLock<...>>` across the registered tools. This keeps the
registry fully available before the MCP server begins handling requests.
