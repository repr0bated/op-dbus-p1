# Implementation Summary

This repo now exposes orchestration skills to MCP clients by registering
`skill_list` and `skill_describe` tools alongside the standard tool registry
used by the gRPC MCP service. These tools are backed by the default
`SkillRegistry` in `op-chat`.

Key points:
- Skills are registered at startup (no lazy initialization).
- MCP clients can list or inspect skills via the gRPC MCP service.
- Tool metadata is derived directly from the Skill registry for consistency.
