# Skill Tooling Implementation

Skill tools are registered in `op-dbus-service` and expose the default
`SkillRegistry` from `op-chat`.

## Tools
- `skill_list`: list skills with optional `category` or `tag` filters.
- `skill_describe`: return metadata or full skill context for a named skill.

## Data returned
- `skill_list` returns an array of skill metadata (name, description, category,
  tags, required tools, version).
- `skill_describe` returns either metadata only or the full serialized skill
  object (including context fields such as system prompt additions).

These tools are available to MCP clients through `tools/list` and `tools/call`.
