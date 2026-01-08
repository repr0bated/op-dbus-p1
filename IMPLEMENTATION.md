# Agent Capabilities + Workstack Orchestration + Skills + MCP-gRPC Integration

## Overview

Extend the existing `op-cache` crate and create supporting infrastructure:

1. **Agents with capabilities array** — declared at registration time
2. **Skills** — domain-specific knowledge that augments agent/tool execution
3. **Orchestrator** — resolves capabilities → agents, applies skills, routes to workstack if 2+ agents
4. **Workstack cache** — caches intermediate step results
5. **Pattern tracker** — detects frequent sequences, suggests promotion
6. **gRPC services** — op-dbus daemon exposes everything via gRPC
7. **MCP proxy** — thin shim spawned by MCP clients, connects to daemon via gRPC
8. **NO LAZY PATTERNS** — eager initialization everywhere

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    MCP Client (Claude Desktop)                  │
│                         spawns ↓                                │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    mcp-proxy (thin shim)                        │
│  - Reads JSON-RPC from stdin                                    │
│  - Connects to op-dbus daemon via gRPC                          │
│  - Writes responses to stdout                                   │
│  - STATELESS — all state in daemon                              │
└─────────────────────────────────────────────────────────────────┘
                              │ gRPC
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    op-dbus daemon (always running)              │
│  ┌───────────┬───────────┬───────────┬───────────┬───────────┐ │
│  │  Agent    │  Skill    │Orchestrator│  Cache   │   MCP     │ │
│  │  Service  │  Service  │  Service   │  Service │  Service  │ │
│  └───────────┴───────────┴───────────┴───────────┴───────────┘ │
│                              │                                  │
│  ┌───────────────────────────┴───────────────────────────────┐ │
│  │              Core Components (op-cache)                   │ │
│  │  AgentRegistry │ SkillRegistry │ Orchestrator │ Cache     │ │
│  └───────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

---

## Skills System

Skills provide **domain-specific knowledge augmentation** for tool/agent execution:

```
┌─────────────────────────────────────────────────────────────────┐
│                         SKILL                                   │
├─────────────────────────────────────────────────────────────────┤
│  name: "python_debugging"                                       │
│  category: "debugging"                                          │
│  priority: 10                                                   │
│  active: false                                                  │
├─────────────────────────────────────────────────────────────────┤
│  CONTEXT:                                                       │
│  - system_prompt_additions: ["Use pdb breakpoints..."]         │
│  - input_transformations: {tool → arg modifications}           │
│  - output_transformations: {tool → output modifications}       │
│  - variables: {"debug_level": "verbose"}                       │
│  - constraints: [RequireArgument, ForbidArgument, ...]         │
├─────────────────────────────────────────────────────────────────┤
│  METADATA:                                                      │
│  - required_tools: ["agent_python_pro"]                        │
│  - tags: ["python", "debugging"]                               │
└─────────────────────────────────────────────────────────────────┘
```

### Skill Flow in Orchestration

```
1. Request arrives with activated skills
           │
           ▼
2. Skill.check_constraints(tool, args)
   - RequireArgument: ensure arg exists
   - ForbidArgument: ensure arg absent
   - RequireConfirmation: flag for review
           │
           ▼
3. Skill.transform_input(tool, args)
   - Inject default arguments
   - Modify existing arguments
           │
           ▼
4. Agent/Tool executes
           │
           ▼
5. Skill.transform_output(tool, output)
   - Post-process results
   - Add annotations
           │
           ▼
6. Return augmented result
```

---

## File Structure

```
crates/
├── op-cache/                      # Core library
│   ├── src/
│   │   ├── lib.rs                 # UPDATE
│   │   ├── agent.rs               # NEW: Agent + capabilities
│   │   ├── skill.rs               # NEW: Skills system
│   │   ├── orchestrator.rs        # NEW: Routes requests with skills
│   │   ├── workstack_cache.rs     # NEW: Step caching
│   │   ├── pattern_tracker.rs     # NEW: Sequence tracking
│   │   ├── btrfs_cache.rs         # EXISTING
│   │   ├── numa.rs                # EXISTING
│   │   └── snapshot_manager.rs    # EXISTING
│   ├── proto/
│   │   └── op_cache.proto         # NEW: gRPC definitions
│   ├── build.rs                   # NEW
│   └── Cargo.toml                 # UPDATE
│
├── op-dbus/                       # Daemon
│   └── src/
│       ├── main.rs                # UPDATE
│       └── grpc/                  # NEW
│           ├── mod.rs
│           ├── agent_service.rs
│           ├── skill_service.rs   # NEW
│           ├── orchestrator_service.rs
│           ├── cache_service.rs
│           ├── mcp_service.rs
│           └── server.rs
│
└── mcp-proxy/                     # NEW
    ├── src/main.rs
    └── Cargo.toml
```

---

## Step 1: Update Cargo.toml (op-cache)

```toml
[package]
name = "op-cache"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1.0"
bincode = "1.3"
chrono = { version = "0.4", features = ["serde"] }
log = "0.4"
num_cpus = "1.16"
rusqlite = { version = "0.31", features = ["bundled"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
sha2 = "0.10"
tokio = { version = "1.0", features = ["full"] }
tracing = "0.1"
uuid = { version = "1.0", features = ["v4"] }
prost = "0.12"
tonic = "0.11"
tokio-stream = "0.1"

[build-dependencies]
tonic-build = "0.11"
```

---

## Step 2: Create proto/op_cache.proto

```protobuf
syntax = "proto3";
package op_cache;

import "google/protobuf/empty.proto";

// ============================================================================
// Core Types
// ============================================================================

enum Capability {
    CAPABILITY_UNSPECIFIED = 0;
    CAPABILITY_CODE_ANALYSIS = 1;
    CAPABILITY_SECURITY_AUDIT = 2;
    CAPABILITY_PERFORMANCE_ANALYSIS = 3;
    CAPABILITY_DEPENDENCY_ANALYSIS = 4;
    CAPABILITY_CODE_GENERATION = 10;
    CAPABILITY_TEST_GENERATION = 11;
    CAPABILITY_DOCUMENTATION_GENERATION = 12;
    CAPABILITY_DATA_EXTRACTION = 20;
    CAPABILITY_DATA_VALIDATION = 21;
    CAPABILITY_EMBEDDING = 22;
    CAPABILITY_PLANNING = 30;
    CAPABILITY_SUMMARIZATION = 31;
    CAPABILITY_QUESTION_ANSWERING = 32;
    CAPABILITY_SHELL_EXECUTION = 40;
    CAPABILITY_API_CALL = 41;
}

enum Priority {
    PRIORITY_UNSPECIFIED = 0;
    PRIORITY_HIGH = 1;
    PRIORITY_NORMAL = 2;
    PRIORITY_LOW = 3;
}

message Agent {
    string id = 1;
    string name = 2;
    string description = 3;
    repeated Capability capabilities = 4;  // THE ARRAY
    repeated Capability requires = 5;
    Priority priority = 6;
    bool parallelizable = 7;
    uint64 estimated_latency_ms = 8;
    bool enabled = 9;
}

// ============================================================================
// Skills
// ============================================================================

enum ConstraintType {
    CONSTRAINT_UNSPECIFIED = 0;
    CONSTRAINT_REQUIRE_ARGUMENT = 1;
    CONSTRAINT_FORBID_ARGUMENT = 2;
    CONSTRAINT_REQUIRE_BEFORE = 3;
    CONSTRAINT_REQUIRE_AFTER = 4;
    CONSTRAINT_MAX_EXECUTIONS = 5;
    CONSTRAINT_REQUIRE_CONFIRMATION = 6;
}

message SkillConstraint {
    ConstraintType constraint_type = 1;
    string target = 2;  // tool name or "*" for all
    string value = 3;   // JSON value
}

message SkillContext {
    repeated string system_prompt_additions = 1;
    map<string, string> input_transformations = 2;   // tool -> JSON transform
    map<string, string> output_transformations = 3;  // tool -> JSON transform
    map<string, string> variables = 4;               // name -> JSON value
    repeated SkillConstraint constraints = 5;
}

message SkillMetadata {
    string description = 1;
    string category = 2;
    repeated string tags = 3;
    repeated string required_tools = 4;
    string version = 5;
}

message Skill {
    string name = 1;
    SkillMetadata metadata = 2;
    SkillContext context = 3;
    bool active = 4;
    int32 priority = 5;
}

// ============================================================================
// Agent Service
// ============================================================================

service AgentService {
    rpc Register(RegisterAgentRequest) returns (RegisterAgentResponse);
    rpc Unregister(UnregisterAgentRequest) returns (UnregisterAgentResponse);
    rpc Execute(ExecuteAgentRequest) returns (ExecuteAgentResponse);
    rpc GetAgent(GetAgentRequest) returns (Agent);
    rpc ListAgents(ListAgentsRequest) returns (ListAgentsResponse);
    rpc FindByCapability(FindByCapabilityRequest) returns (FindByCapabilityResponse);
}

message RegisterAgentRequest {
    Agent agent = 1;
}

message RegisterAgentResponse {
    bool success = 1;
    string agent_id = 2;
    string error = 3;
}

message UnregisterAgentRequest {
    string agent_id = 1;
}

message UnregisterAgentResponse {
    bool success = 1;
}

message ExecuteAgentRequest {
    string agent_id = 1;
    bytes input = 2;
    repeated string active_skills = 3;  // Skills to apply
}

message ExecuteAgentResponse {
    bytes output = 1;
    uint64 latency_ms = 2;
    bool success = 3;
    string error = 4;
    repeated string skills_applied = 5;
}

message GetAgentRequest {
    string agent_id = 1;
}

message ListAgentsRequest {
    bool enabled_only = 1;
}

message ListAgentsResponse {
    repeated Agent agents = 1;
}

message FindByCapabilityRequest {
    repeated Capability capabilities = 1;
}

message FindByCapabilityResponse {
    repeated Agent agents = 1;
}

// ============================================================================
// Skill Service
// ============================================================================

service SkillService {
    // Registration
    rpc Register(RegisterSkillRequest) returns (RegisterSkillResponse);
    rpc Unregister(UnregisterSkillRequest) returns (UnregisterSkillResponse);
    
    // Activation
    rpc Activate(ActivateSkillRequest) returns (ActivateSkillResponse);
    rpc Deactivate(DeactivateSkillRequest) returns (DeactivateSkillResponse);
    
    // Query
    rpc GetSkill(GetSkillRequest) returns (Skill);
    rpc ListSkills(ListSkillsRequest) returns (ListSkillsResponse);
    rpc ListActiveSkills(google.protobuf.Empty) returns (ListSkillsResponse);
    rpc ListByCategory(ListByCategoryRequest) returns (ListSkillsResponse);
    
    // Context
    rpc GetCombinedContext(google.protobuf.Empty) returns (SkillContext);
    
    // Constraint checking
    rpc CheckConstraints(CheckConstraintsRequest) returns (CheckConstraintsResponse);
    
    // Transformations
    rpc TransformInput(TransformRequest) returns (TransformResponse);
    rpc TransformOutput(TransformRequest) returns (TransformResponse);
}

message RegisterSkillRequest {
    Skill skill = 1;
}

message RegisterSkillResponse {
    bool success = 1;
    string skill_name = 2;
    string error = 3;
}

message UnregisterSkillRequest {
    string skill_name = 1;
}

message UnregisterSkillResponse {
    bool success = 1;
}

message ActivateSkillRequest {
    string skill_name = 1;
}

message ActivateSkillResponse {
    bool success = 1;
    string error = 2;
}

message DeactivateSkillRequest {
    string skill_name = 1;
}

message DeactivateSkillResponse {
    bool success = 1;
}

message GetSkillRequest {
    string skill_name = 1;
}

message ListSkillsRequest {
    bool active_only = 1;
}

message ListByCategoryRequest {
    string category = 1;
}

message CheckConstraintsRequest {
    string tool_name = 1;
    bytes arguments = 2;  // JSON arguments
    repeated string active_skills = 3;
}

message CheckConstraintsResponse {
    bool valid = 1;
    repeated string violations = 2;
    repeated string warnings = 3;
}

message TransformRequest {
    string tool_name = 1;
    bytes data = 2;  // JSON input or output
    repeated string active_skills = 3;
}

message TransformResponse {
    bytes data = 1;  // Transformed JSON
    repeated string skills_applied = 2;
}

// ============================================================================
// Orchestrator Service
// ============================================================================

service OrchestratorService {
    rpc Execute(OrchestratorRequest) returns (OrchestratorResponse);
    rpc ExecuteAgents(ExecuteAgentsRequest) returns (OrchestratorResponse);
    rpc Resolve(ResolveRequest) returns (ResolveResponse);
    rpc GetPatterns(GetPatternsRequest) returns (GetPatternsResponse);
}

message OrchestratorRequest {
    repeated Capability required_capabilities = 1;
    bytes input = 2;
    repeated string preferred_agents = 3;
    repeated string excluded_agents = 4;
    repeated string activate_skills = 5;  // Skills to activate for this request
}

message OrchestratorResponse {
    string request_id = 1;
    bytes output = 2;
    repeated StepResult steps = 3;
    uint64 total_latency_ms = 4;
    uint32 cache_hits = 5;
    uint32 cache_misses = 6;
    bool used_workstack = 7;
    repeated string resolved_agents = 8;
    repeated string skills_applied = 9;  // Skills that were applied
}

message ExecuteAgentsRequest {
    repeated string agent_ids = 1;
    bytes input = 2;
    repeated string activate_skills = 3;
}

message StepResult {
    uint32 step_index = 1;
    string agent_id = 2;
    uint64 latency_ms = 3;
    bool cached = 4;
    uint64 output_size = 5;
    repeated string skills_applied = 6;
}

message ResolveRequest {
    repeated Capability required_capabilities = 1;
}

message ResolveResponse {
    repeated Agent agents = 1;
    repeated Capability fulfilled = 2;
    repeated Capability missing = 3;
}

message GetPatternsRequest {}

message GetPatternsResponse {
    repeated PatternSuggestion patterns = 1;
}

message PatternSuggestion {
    string pattern_id = 1;
    repeated string agent_sequence = 2;
    uint32 call_count = 3;
    uint64 avg_latency_ms = 4;
    string suggested_name = 5;
}

// ============================================================================
// Cache Service
// ============================================================================

service CacheService {
    rpc GetEmbedding(GetEmbeddingRequest) returns (GetEmbeddingResponse);
    rpc PutEmbedding(PutEmbeddingRequest) returns (PutEmbeddingResponse);
    rpc GetWorkstackStep(GetWorkstackStepRequest) returns (GetWorkstackStepResponse);
    rpc PutWorkstackStep(PutWorkstackStepRequest) returns (PutWorkstackStepResponse);
    rpc InvalidateWorkstack(InvalidateWorkstackRequest) returns (InvalidateWorkstackResponse);
    rpc GetStats(GetStatsRequest) returns (CacheStats);
    rpc Cleanup(CleanupRequest) returns (CleanupResponse);
}

message GetEmbeddingRequest {
    string text = 1;
}

message GetEmbeddingResponse {
    bool found = 1;
    repeated float vector = 2;
}

message PutEmbeddingRequest {
    string text = 1;
    repeated float vector = 2;
}

message PutEmbeddingResponse {
    bool success = 1;
    string text_hash = 2;
}

message GetWorkstackStepRequest {
    string workstack_id = 1;
    uint32 step_index = 2;
    string input_hash = 3;
}

message GetWorkstackStepResponse {
    bool found = 1;
    bytes output = 2;
}

message PutWorkstackStepRequest {
    string workstack_id = 1;
    uint32 step_index = 2;
    string input_hash = 3;
    bytes output = 4;
    int64 ttl_seconds = 5;
}

message PutWorkstackStepResponse {
    bool success = 1;
}

message InvalidateWorkstackRequest {
    string workstack_id = 1;
}

message InvalidateWorkstackResponse {
    uint32 entries_removed = 1;
}

message GetStatsRequest {}

message CacheStats {
    uint64 total_entries = 1;
    uint64 hot_entries = 2;
    uint64 total_accesses = 3;
    uint64 disk_usage_bytes = 4;
    double hit_rate = 5;
}

message CleanupRequest {
    int64 max_age_days = 1;
}

message CleanupResponse {
    uint32 entries_removed = 1;
    uint64 bytes_freed = 2;
}

// ============================================================================
// MCP Service
// ============================================================================

service MCPService {
    rpc HandleRequest(MCPRequest) returns (MCPResponse);
    rpc ListTools(ListToolsRequest) returns (ListToolsResponse);
}

message MCPRequest {
    string jsonrpc = 1;
    string method = 2;
    string id = 3;
    bytes params = 4;
}

message MCPResponse {
    string jsonrpc = 1;
    string id = 2;
    bytes result = 3;
    MCPError error = 4;
}

message MCPError {
    int32 code = 1;
    string message = 2;
    bytes data = 3;
}

message ListToolsRequest {}

message ListToolsResponse {
    repeated MCPTool tools = 1;
}

message MCPTool {
    string name = 1;
    string description = 2;
    bytes input_schema = 3;
}
```

---

## Step 3: Create src/skill.rs

```rust
//! Skills system — domain-specific knowledge augmentation
//!
//! Skills provide:
//! - System prompt additions
//! - Input/output transformations
//! - Constraints enforcement
//! - Domain-specific variables
//!
//! NO LAZY INITIALIZATION.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Constraint types for skill enforcement
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintType {
    RequireArgument,
    ForbidArgument,
    RequireBefore,
    RequireAfter,
    MaxExecutions,
    RequireConfirmation,
}

/// A constraint enforced by a skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillConstraint {
    pub constraint_type: ConstraintType,
    pub target: String,  // tool name or "*" for all
    pub value: Value,
}

/// Context provided by an active skill
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillContext {
    /// Additional system prompt content
    pub system_prompt_additions: Vec<String>,
    /// Input transformations by tool name
    pub input_transformations: HashMap<String, Value>,
    /// Output transformations by tool name
    pub output_transformations: HashMap<String, Value>,
    /// Variables available to the skill
    pub variables: HashMap<String, Value>,
    /// Constraints to enforce
    pub constraints: Vec<SkillConstraint>,
}

/// Skill metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
    pub required_tools: Vec<String>,
    pub version: String,
}

/// A skill definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub metadata: SkillMetadata,
    pub context: SkillContext,
    pub active: bool,
    pub priority: i32,
}

impl Skill {
    /// Create a new skill
    pub fn new(name: &str, description: &str, category: &str) -> Self {
        Self {
            name: name.to_string(),
            metadata: SkillMetadata {
                description: description.to_string(),
                category: category.to_string(),
                tags: Vec::new(),
                required_tools: Vec::new(),
                version: "1.0.0".to_string(),
            },
            context: SkillContext::default(),
            active: false,
            priority: 0,
        }
    }

    /// Add system prompt addition
    pub fn with_prompt(mut self, prompt: &str) -> Self {
        self.context.system_prompt_additions.push(prompt.to_string());
        self
    }

    /// Add required tool
    pub fn with_required_tool(mut self, tool: &str) -> Self {
        self.metadata.required_tools.push(tool.to_string());
        self
    }

    /// Add constraint
    pub fn with_constraint(mut self, constraint: SkillConstraint) -> Self {
        self.context.constraints.push(constraint);
        self
    }

    /// Add variable
    pub fn with_variable(mut self, name: &str, value: Value) -> Self {
        self.context.variables.insert(name.to_string(), value);
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Add tag
    pub fn with_tag(mut self, tag: &str) -> Self {
        self.metadata.tags.push(tag.to_string());
        self
    }

    /// Add input transformation for a tool
    pub fn with_input_transform(mut self, tool: &str, transform: Value) -> Self {
        self.context.input_transformations.insert(tool.to_string(), transform);
        self
    }

    /// Add output transformation for a tool
    pub fn with_output_transform(mut self, tool: &str, transform: Value) -> Self {
        self.context.output_transformations.insert(tool.to_string(), transform);
        self
    }

    /// Activate the skill
    pub fn activate(&mut self) {
        self.active = true;
        info!(skill = %self.name, "Skill activated");
    }

    /// Deactivate the skill
    pub fn deactivate(&mut self) {
        self.active = false;
        info!(skill = %self.name, "Skill deactivated");
    }

    /// Transform input arguments
    pub fn transform_input(&self, tool_name: &str, mut args: Value) -> Value {
        if let Some(transform) = self.context.input_transformations.get(tool_name) {
            if let (Some(args_obj), Some(transform_obj)) = (args.as_object_mut(), transform.as_object()) {
                for (key, value) in transform_obj {
                    if !args_obj.contains_key(key) {
                        args_obj.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        args
    }

    /// Transform output
    pub fn transform_output(&self, tool_name: &str, output: Value) -> Value {
        if let Some(_transform) = self.context.output_transformations.get(tool_name) {
            // Apply transformation logic
            // For now, return as-is
            output
        } else {
            output
        }
    }

    /// Check constraints
    pub fn check_constraints(&self, tool_name: &str, args: &Value) -> Result<()> {
        for constraint in &self.context.constraints {
            if constraint.target != tool_name && constraint.target != "*" {
                continue;
            }

            match constraint.constraint_type {
                ConstraintType::RequireArgument => {
                    if let Some(required_key) = constraint.value.as_str() {
                        if args.get(required_key).is_none() {
                            bail!(
                                "Skill '{}' requires argument '{}' for tool '{}'",
                                self.name, required_key, tool_name
                            );
                        }
                    }
                }
                ConstraintType::ForbidArgument => {
                    if let Some(forbidden_key) = constraint.value.as_str() {
                        if args.get(forbidden_key).is_some() {
                            bail!(
                                "Skill '{}' forbids argument '{}' for tool '{}'",
                                self.name, forbidden_key, tool_name
                            );
                        }
                    }
                }
                ConstraintType::RequireConfirmation => {
                    warn!(
                        skill = %self.name,
                        tool = %tool_name,
                        "Tool requires confirmation"
                    );
                    // Don't block, just warn
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Registry of skills — NO LAZY, all eager
pub struct SkillRegistry {
    skills: RwLock<HashMap<String, Skill>>,
}

impl SkillRegistry {
    /// Create empty registry
    pub fn new() -> Self {
        Self {
            skills: RwLock::new(HashMap::new()),  // EAGER
        }
    }

    /// Create registry with default skills
    pub fn with_defaults() -> Self {
        let registry = Self::new();
        // Register defaults synchronously via blocking
        let mut skills = HashMap::new();
        for skill in default_skills() {
            skills.insert(skill.name.clone(), skill);
        }
        Self {
            skills: RwLock::new(skills),
        }
    }

    /// Register a skill
    pub async fn register(&self, skill: Skill) {
        info!(skill = %skill.name, category = %skill.metadata.category, "Registering skill");
        let mut skills = self.skills.write().await;
        skills.insert(skill.name.clone(), skill);
    }

    /// Get skill by name
    pub async fn get(&self, name: &str) -> Option<Skill> {
        let skills = self.skills.read().await;
        skills.get(name).cloned()
    }

    /// List all skills
    pub async fn list_all(&self) -> Vec<Skill> {
        let skills = self.skills.read().await;
        skills.values().cloned().collect()
    }

    /// List active skills (sorted by priority)
    pub async fn active_skills(&self) -> Vec<Skill> {
        let skills = self.skills.read().await;
        let mut active: Vec<Skill> = skills.values().filter(|s| s.active).cloned().collect();
        active.sort_by(|a, b| b.priority.cmp(&a.priority));
        active
    }

    /// List skills by category
    pub async fn list_by_category(&self, category: &str) -> Vec<Skill> {
        let skills = self.skills.read().await;
        skills.values()
            .filter(|s| s.metadata.category == category)
            .cloned()
            .collect()
    }

    /// Activate skill by name
    pub async fn activate(&self, name: &str) -> Result<()> {
        let mut skills = self.skills.write().await;
        if let Some(skill) = skills.get_mut(name) {
            skill.activate();
            Ok(())
        } else {
            bail!("Skill not found: {}", name)
        }
    }

    /// Deactivate skill by name
    pub async fn deactivate(&self, name: &str) -> Result<()> {
        let mut skills = self.skills.write().await;
        if let Some(skill) = skills.get_mut(name) {
            skill.deactivate();
            Ok(())
        } else {
            bail!("Skill not found: {}", name)
        }
    }

    /// Get combined context from all active skills
    pub async fn combined_context(&self) -> SkillContext {
        let active = self.active_skills().await;
        let mut combined = SkillContext::default();

        for skill in active {
            combined.system_prompt_additions.extend(skill.context.system_prompt_additions);
            combined.input_transformations.extend(skill.context.input_transformations);
            combined.output_transformations.extend(skill.context.output_transformations);
            combined.variables.extend(skill.context.variables);
            combined.constraints.extend(skill.context.constraints);
        }

        combined
    }

    /// Check constraints for all active skills
    pub async fn check_constraints(&self, tool_name: &str, args: &Value) -> Result<Vec<String>> {
        let active = self.active_skills().await;
        let mut warnings = Vec::new();

        for skill in active {
            if let Err(e) = skill.check_constraints(tool_name, args) {
                return Err(e);
            }
        }

        Ok(warnings)
    }

    /// Transform input through all active skills
    pub async fn transform_input(&self, tool_name: &str, mut args: Value) -> Value {
        let active = self.active_skills().await;
        for skill in active {
            args = skill.transform_input(tool_name, args);
        }
        args
    }

    /// Transform output through all active skills
    pub async fn transform_output(&self, tool_name: &str, mut output: Value) -> Value {
        let active = self.active_skills().await;
        for skill in active {
            output = skill.transform_output(tool_name, output);
        }
        output
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// Default skills
fn default_skills() -> Vec<Skill> {
    vec![
        // Python debugging
        Skill::new("python_debugging", "Enhanced Python debugging", "debugging")
            .with_prompt("Use pdb breakpoints and inspect variables systematically.")
            .with_required_tool("agent_python_pro")
            .with_variable("debug_level", json!("verbose"))
            .with_priority(10),

        // Rust optimization
        Skill::new("rust_optimization", "Rust performance optimization", "optimization")
            .with_prompt("Focus on zero-cost abstractions, avoid allocations, use iterators.")
            .with_required_tool("agent_rust_pro")
            .with_priority(10),

        // Security audit
        Skill::new("security_audit", "Security-focused code review", "security")
            .with_prompt("Check for: SQL injection, XSS, CSRF, path traversal, secrets in code.")
            .with_constraint(SkillConstraint {
                constraint_type: ConstraintType::RequireConfirmation,
                target: "*".to_string(),
                value: json!(true),
            })
            .with_priority(20),

        // TDD workflow
        Skill::new("tdd_workflow", "Test-Driven Development", "methodology")
            .with_prompt("Red-Green-Refactor: 1) Write failing test, 2) Minimal code to pass, 3) Refactor.")
            .with_priority(15),

        // Documentation
        Skill::new("documentation", "Documentation generation", "documentation")
            .with_prompt("Generate clear docstrings, README sections, and API documentation.")
            .with_priority(5),

        // OVS networking
        Skill::new("ovs_networking", "Open vSwitch expertise", "networking")
            .with_prompt("Use OVSDB JSON-RPC for bridge/port management. Never use ovs-vsctl CLI.")
            .with_required_tool("ovs_create_bridge")
            .with_required_tool("ovs_list_bridges")
            .with_priority(10),

        // Systemd management
        Skill::new("systemd_management", "Systemd service management", "system")
            .with_prompt("Use D-Bus for systemd operations. Check status before changes.")
            .with_required_tool("systemd_status")
            .with_constraint(SkillConstraint {
                constraint_type: ConstraintType::RequireBefore,
                target: "systemd_restart".to_string(),
                value: json!("systemd_status"),
            })
            .with_priority(10),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_creation() {
        let skill = Skill::new("test", "Test skill", "testing")
            .with_prompt("Test prompt")
            .with_priority(5);

        assert_eq!(skill.name, "test");
        assert_eq!(skill.metadata.category, "testing");
        assert_eq!(skill.priority, 5);
        assert!(!skill.active);
    }

    #[tokio::test]
    async fn test_skill_registry() {
        let registry = SkillRegistry::with_defaults();

        assert!(registry.get("python_debugging").await.is_some());
        assert!(registry.get("nonexistent").await.is_none());

        registry.activate("python_debugging").await.unwrap();
        assert_eq!(registry.active_skills().await.len(), 1);

        registry.deactivate("python_debugging").await.unwrap();
        assert_eq!(registry.active_skills().await.len(), 0);
    }

    #[test]
    fn test_constraint_checking() {
        let skill = Skill::new("test", "test", "test")
            .with_constraint(SkillConstraint {
                constraint_type: ConstraintType::RequireArgument,
                target: "test_tool".to_string(),
                value: json!("required_arg"),
            });

        // Should fail - missing required argument
        let result = skill.check_constraints("test_tool", &json!({"other": "value"}));
        assert!(result.is_err());

        // Should pass
        let result = skill.check_constraints("test_tool", &json!({"required_arg": "value"}));
        assert!(result.is_ok());
    }
}
```

---

## Step 4: Create src/agent.rs

```rust
//! Agent registry with capabilities array
//!
//! Capabilities are stored at REGISTRATION TIME.
//! NO LAZY INITIALIZATION.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::skill::SkillRegistry;

/// Capability enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    CodeAnalysis,
    SecurityAudit,
    PerformanceAnalysis,
    DependencyAnalysis,
    CodeGeneration,
    TestGeneration,
    DocumentationGeneration,
    DataExtraction,
    DataValidation,
    Embedding,
    Planning,
    Summarization,
    QuestionAnswering,
    ShellExecution,
    ApiCall,
}

/// Execution priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
pub enum Priority {
    High = 0,
    #[default]
    Normal = 1,
    Low = 2,
}

/// Agent definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub description: String,
    pub capabilities: Vec<Capability>,
    pub requires: Vec<Capability>,
    pub priority: Priority,
    pub parallelizable: bool,
    pub estimated_latency_ms: u64,
    pub enabled: bool,
}

impl Agent {
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            description: String::new(),
            capabilities: Vec::new(),
            requires: Vec::new(),
            priority: Priority::Normal,
            parallelizable: false,
            estimated_latency_ms: 100,
            enabled: true,
        }
    }

    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }

    pub fn with_capability(mut self, cap: Capability) -> Self {
        if !self.capabilities.contains(&cap) {
            self.capabilities.push(cap);
        }
        self
    }

    pub fn with_capabilities(mut self, caps: &[Capability]) -> Self {
        for cap in caps {
            if !self.capabilities.contains(cap) {
                self.capabilities.push(*cap);
            }
        }
        self
    }

    pub fn requires(mut self, cap: Capability) -> Self {
        if !self.requires.contains(&cap) {
            self.requires.push(cap);
        }
        self
    }

    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    pub fn parallelizable(mut self, parallel: bool) -> Self {
        self.parallelizable = parallel;
        self
    }

    pub fn with_latency(mut self, ms: u64) -> Self {
        self.estimated_latency_ms = ms;
        self
    }

    pub fn provides(&self, cap: Capability) -> bool {
        self.capabilities.contains(&cap)
    }
}

/// Agent executor function
pub type AgentExecutor = Arc<dyn Fn(&[u8]) -> Result<Vec<u8>> + Send + Sync>;

/// Registered agent
struct RegisteredAgent {
    definition: Agent,
    executor: AgentExecutor,
}

/// Agent registry — NO LAZY
pub struct AgentRegistry {
    agents: RwLock<HashMap<String, RegisteredAgent>>,
    capability_index: RwLock<HashMap<Capability, Vec<String>>>,
    skill_registry: Arc<SkillRegistry>,
}

impl AgentRegistry {
    /// Create new registry with skill support
    pub fn new(skill_registry: Arc<SkillRegistry>) -> Self {
        Self {
            agents: RwLock::new(HashMap::new()),
            capability_index: RwLock::new(HashMap::new()),
            skill_registry,
        }
    }

    /// Register agent
    pub async fn register(&self, agent: Agent, executor: AgentExecutor) -> Result<()> {
        let agent_id = agent.id.clone();
        let capabilities = agent.capabilities.clone();

        {
            let mut agents = self.agents.write().await;
            agents.insert(
                agent_id.clone(),
                RegisteredAgent { definition: agent, executor },
            );
        }

        {
            let mut index = self.capability_index.write().await;
            for cap in capabilities {
                index
                    .entry(cap)
                    .or_insert_with(Vec::new)
                    .push(agent_id.clone());
            }
        }

        info!("Registered agent: {}", agent_id);
        Ok(())
    }

    /// Find agents by capability
    pub async fn find_by_capability(&self, cap: Capability) -> Vec<Agent> {
        let index = self.capability_index.read().await;
        let agents = self.agents.read().await;

        index
            .get(&cap)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| agents.get(id).map(|a| a.definition.clone()))
                    .filter(|a| a.enabled)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Find agents by capabilities
    pub async fn find_by_capabilities(&self, caps: &[Capability]) -> Vec<Agent> {
        let mut result = Vec::new();
        let mut seen = HashSet::new();

        for cap in caps {
            for agent in self.find_by_capability(*cap).await {
                if !seen.contains(&agent.id) {
                    seen.insert(agent.id.clone());
                    result.push(agent);
                }
            }
        }

        result
    }

    /// Get agent by ID
    pub async fn get(&self, agent_id: &str) -> Option<Agent> {
        let agents = self.agents.read().await;
        agents.get(agent_id).map(|a| a.definition.clone())
    }

    /// Execute agent with skill support
    pub async fn execute(
        &self,
        agent_id: &str,
        input: &[u8],
        apply_skills: bool,
    ) -> Result<(Vec<u8>, Vec<String>)> {
        let executor = {
            let agents = self.agents.read().await;
            agents
                .get(agent_id)
                .map(|a| a.executor.clone())
                .context(format!("Agent not found: {}", agent_id))?
        };

        // Apply skill transformations if enabled
        let (processed_input, skills_applied) = if apply_skills {
            let input_value: serde_json::Value = serde_json::from_slice(input)
                .unwrap_or(serde_json::Value::Null);

            // Check constraints
            self.skill_registry.check_constraints(agent_id, &input_value).await?;

            // Transform input
            let transformed = self.skill_registry.transform_input(agent_id, input_value).await;
            let active_skills: Vec<String> = self.skill_registry
                .active_skills()
                .await
                .iter()
                .map(|s| s.name.clone())
                .collect();

            (serde_json::to_vec(&transformed)?, active_skills)
        } else {
            (input.to_vec(), Vec::new())
        };

        let output = executor(&processed_input)?;

        // Transform output if skills active
        let final_output = if apply_skills && !skills_applied.is_empty() {
            let output_value: serde_json::Value = serde_json::from_slice(&output)
                .unwrap_or(serde_json::Value::Null);
            let transformed = self.skill_registry.transform_output(agent_id, output_value).await;
            serde_json::to_vec(&transformed)?
        } else {
            output
        };

        Ok((final_output, skills_applied))
    }

    /// List all agents
    pub async fn list_all(&self) -> Vec<Agent> {
        let agents = self.agents.read().await;
        agents.values().map(|a| a.definition.clone()).collect()
    }

    /// Get skill registry
    pub fn skill_registry(&self) -> &Arc<SkillRegistry> {
        &self.skill_registry
    }
}
```

---

## Step 5: Create src/orchestrator.rs

```rust
//! Orchestrator — routes requests with skill support
//!
//! NO LAZY PATTERNS.

use anyhow::Result;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

use crate::agent::{Agent, AgentRegistry, Capability};
use crate::pattern_tracker::PatternTracker;
use crate::skill::SkillRegistry;
use crate::workstack_cache::WorkstackCache;

/// Request with capabilities
pub struct CapabilityRequest {
    pub required_capabilities: Vec<Capability>,
    pub input: Vec<u8>,
    pub preferred_agents: Vec<String>,
    pub excluded_agents: Vec<String>,
    pub activate_skills: Vec<String>,  // Skills to activate for this request
}

impl CapabilityRequest {
    pub fn new(capabilities: Vec<Capability>, input: Vec<u8>) -> Self {
        Self {
            required_capabilities: capabilities,
            input,
            preferred_agents: Vec::new(),
            excluded_agents: Vec::new(),
            activate_skills: Vec::new(),
        }
    }

    pub fn with_skills(mut self, skills: Vec<String>) -> Self {
        self.activate_skills = skills;
        self
    }
}

/// Execution result
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub request_id: String,
    pub output: Vec<u8>,
    pub steps: Vec<StepResult>,
    pub total_latency_ms: u64,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub used_workstack: bool,
    pub resolved_agents: Vec<String>,
    pub skills_applied: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct StepResult {
    pub step_index: usize,
    pub agent_id: String,
    pub latency_ms: u64,
    pub cached: bool,
    pub output_size: usize,
    pub skills_applied: Vec<String>,
}

/// Orchestrator config
pub struct OrchestratorConfig {
    pub workstack_threshold: usize,
    pub enable_caching: bool,
    pub enable_skills: bool,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            workstack_threshold: 2,
            enable_caching: true,
            enable_skills: true,
        }
    }
}

/// Orchestrator — NO LAZY
pub struct Orchestrator {
    registry: Arc<AgentRegistry>,
    skill_registry: Arc<SkillRegistry>,
    cache: Arc<WorkstackCache>,
    pattern_tracker: Arc<PatternTracker>,
    config: OrchestratorConfig,
}

impl Orchestrator {
    /// Create orchestrator — EAGER initialization
    pub async fn new(
        cache_dir: PathBuf,
        registry: Arc<AgentRegistry>,
        skill_registry: Arc<SkillRegistry>,
        config: OrchestratorConfig,
    ) -> Result<Self> {
        let cache = WorkstackCache::new(cache_dir.clone()).await?;
        let pattern_tracker = PatternTracker::new(cache_dir).await?;

        Ok(Self {
            registry,
            skill_registry,
            cache: Arc::new(cache),
            pattern_tracker: Arc::new(pattern_tracker),
            config,
        })
    }

    /// Main entry point
    pub async fn execute(&self, request: CapabilityRequest) -> Result<ExecutionResult> {
        let start = Instant::now();
        let request_id = uuid::Uuid::new_v4().to_string();

        // Activate requested skills
        for skill_name in &request.activate_skills {
            let _ = self.skill_registry.activate(skill_name).await;
        }

        // Resolve capabilities to agents
        let agents = self.resolve_capabilities(&request).await?;

        if agents.is_empty() {
            return Ok(ExecutionResult {
                request_id,
                output: request.input,
                steps: Vec::new(),
                total_latency_ms: 0,
                cache_hits: 0,
                cache_misses: 0,
                used_workstack: false,
                resolved_agents: Vec::new(),
                skills_applied: request.activate_skills.clone(),
            });
        }

        // Route based on agent count
        let result = if agents.len() >= self.config.workstack_threshold {
            self.execute_workstack(&request_id, &agents, request.input, start).await
        } else {
            self.execute_single(&request_id, &agents[0], request.input, start).await
        };

        // Deactivate skills after execution
        for skill_name in &request.activate_skills {
            let _ = self.skill_registry.deactivate(skill_name).await;
        }

        let mut result = result?;
        result.skills_applied = request.activate_skills;
        Ok(result)
    }

    async fn resolve_capabilities(&self, request: &CapabilityRequest) -> Result<Vec<Agent>> {
        let mut selected: Vec<Agent> = Vec::new();
        let mut fulfilled: HashSet<Capability> = HashSet::new();

        for cap in &request.required_capabilities {
            if fulfilled.contains(cap) {
                continue;
            }

            let candidates = self.registry.find_by_capability(*cap).await;
            let candidates: Vec<_> = candidates
                .into_iter()
                .filter(|a| !request.excluded_agents.contains(&a.id))
                .filter(|a| !selected.iter().any(|s| s.id == a.id))
                .collect();

            if let Some(agent) = self.select_best(&candidates, &request.preferred_agents) {
                for c in &agent.capabilities {
                    fulfilled.insert(*c);
                }
                selected.push(agent);
            }
        }

        selected.sort_by_key(|a| a.priority);
        Ok(selected)
    }

    fn select_best(&self, candidates: &[Agent], preferred: &[String]) -> Option<Agent> {
        if candidates.is_empty() {
            return None;
        }

        for pref in preferred {
            if let Some(agent) = candidates.iter().find(|a| &a.id == pref) {
                return Some(agent.clone());
            }
        }

        candidates.iter().min_by_key(|a| a.estimated_latency_ms).cloned()
    }

    async fn execute_single(
        &self,
        request_id: &str,
        agent: &Agent,
        input: Vec<u8>,
        start: Instant,
    ) -> Result<ExecutionResult> {
        let step_start = Instant::now();
        let (output, skills_applied) = self.registry
            .execute(&agent.id, &input, self.config.enable_skills)
            .await?;

        Ok(ExecutionResult {
            request_id: request_id.to_string(),
            output,
            steps: vec![StepResult {
                step_index: 0,
                agent_id: agent.id.clone(),
                latency_ms: step_start.elapsed().as_millis() as u64,
                cached: false,
                output_size: 0,
                skills_applied: skills_applied.clone(),
            }],
            total_latency_ms: start.elapsed().as_millis() as u64,
            cache_hits: 0,
            cache_misses: 1,
            used_workstack: false,
            resolved_agents: vec![agent.id.clone()],
            skills_applied,
        })
    }

    async fn execute_workstack(
        &self,
        request_id: &str,
        agents: &[Agent],
        input: Vec<u8>,
        start: Instant,
    ) -> Result<ExecutionResult> {
        let workstack_id = format!("ws-{}", &Self::hash(&input)[..12]);
        let mut current_input = input.clone();
        let mut steps = Vec::new();
        let mut cache_hits = 0usize;
        let mut cache_misses = 0usize;
        let mut all_skills_applied = Vec::new();

        for (step_index, agent) in agents.iter().enumerate() {
            let input_hash = Self::hash(&current_input);
            let step_start = Instant::now();

            let (output, cached, skills_applied) = if self.config.enable_caching {
                match self.cache.get(&workstack_id, step_index, &input_hash)? {
                    Some(cached) => {
                        cache_hits += 1;
                        (cached, true, Vec::new())
                    }
                    None => {
                        cache_misses += 1;
                        let (output, skills) = self.registry
                            .execute(&agent.id, &current_input, self.config.enable_skills)
                            .await?;
                        self.cache.put(&workstack_id, step_index, &input_hash, &output, None)?;
                        (output, false, skills)
                    }
                }
            } else {
                let (output, skills) = self.registry
                    .execute(&agent.id, &current_input, self.config.enable_skills)
                    .await?;
                (output, false, skills)
            };

            all_skills_applied.extend(skills_applied.clone());

            steps.push(StepResult {
                step_index,
                agent_id: agent.id.clone(),
                latency_ms: step_start.elapsed().as_millis() as u64,
                cached,
                output_size: output.len(),
                skills_applied,
            });

            current_input = output;
        }

        let total_latency_ms = start.elapsed().as_millis() as u64;

        // Track pattern
        let agent_ids: Vec<&str> = agents.iter().map(|a| a.id.as_str()).collect();
        let _ = self.pattern_tracker.record_sequence(&agent_ids, total_latency_ms);

        info!(
            "Workstack {} completed: {} agents, {} cache hits, {} skills applied",
            workstack_id, agents.len(), cache_hits, all_skills_applied.len()
        );

        // Deduplicate skills
        all_skills_applied.sort();
        all_skills_applied.dedup();

        Ok(ExecutionResult {
            request_id: request_id.to_string(),
            output: current_input,
            steps,
            total_latency_ms,
            cache_hits,
            cache_misses,
            used_workstack: true,
            resolved_agents: agents.iter().map(|a| a.id.clone()).collect(),
            skills_applied: all_skills_applied,
        })
    }

    fn hash(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    pub fn registry(&self) -> &Arc<AgentRegistry> {
        &self.registry
    }

    pub fn skill_registry(&self) -> &Arc<SkillRegistry> {
        &self.skill_registry
    }

    pub fn cache(&self) -> &Arc<WorkstackCache> {
        &self.cache
    }

    pub fn pattern_tracker(&self) -> &Arc<PatternTracker> {
        &self.pattern_tracker
    }
}
```

---

## Step 6: Update src/lib.rs

```rust
//! op-cache: BTRFS-based caching with agent orchestration and skills
//!
//! NO LAZY PATTERNS — all initialization is eager.

pub mod agent;
pub mod btrfs_cache;
pub mod numa;
pub mod orchestrator;
pub mod pattern_tracker;
pub mod skill;
pub mod snapshot_manager;
pub mod workstack_cache;

pub use agent::{Agent, AgentRegistry, Capability, Priority};
pub use btrfs_cache::BtrfsCache;
pub use numa::{NumaNode, NumaTopology};
pub use orchestrator::{CapabilityRequest, ExecutionResult, Orchestrator, OrchestratorConfig};
pub use pattern_tracker::PatternTracker;
pub use skill::{Skill, SkillConstraint, SkillContext, SkillMetadata, SkillRegistry, ConstraintType};
pub use snapshot_manager::SnapshotManager;
pub use workstack_cache::WorkstackCache;

// Generated protobuf code
pub mod proto {
    tonic::include_proto!("op_cache");
}

pub mod prelude {
    pub use super::agent::{Agent, AgentRegistry, Capability, Priority};
    pub use super::btrfs_cache::BtrfsCache;
    pub use super::orchestrator::{CapabilityRequest, ExecutionResult, Orchestrator};
    pub use super::skill::{Skill, SkillRegistry, ConstraintType};
    pub use super::workstack_cache::WorkstackCache;
}
```

---

## Step 7: Create gRPC Skill Service

### src/grpc/skill_service.rs

```rust
//! gRPC Skill Service implementation

use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, info};

use crate::proto::{
    skill_service_server::SkillService as SkillServiceTrait,
    ActivateSkillRequest, ActivateSkillResponse,
    CheckConstraintsRequest, CheckConstraintsResponse,
    DeactivateSkillRequest, DeactivateSkillResponse,
    GetSkillRequest, ListByCategoryRequest, ListSkillsRequest, ListSkillsResponse,
    RegisterSkillRequest, RegisterSkillResponse,
    Skill as ProtoSkill, SkillContext as ProtoSkillContext,
    SkillConstraint as ProtoConstraint, SkillMetadata as ProtoMetadata,
    TransformRequest, TransformResponse,
    UnregisterSkillRequest, UnregisterSkillResponse,
};
use op_cache::{Skill, SkillConstraint, SkillContext, SkillMetadata, SkillRegistry, ConstraintType};

pub struct SkillServiceImpl {
    registry: Arc<SkillRegistry>,
}

impl SkillServiceImpl {
    pub fn new(registry: Arc<SkillRegistry>) -> Self {
        Self { registry }
    }

    fn skill_to_proto(skill: &Skill) -> ProtoSkill {
        ProtoSkill {
            name: skill.name.clone(),
            metadata: Some(ProtoMetadata {
                description: skill.metadata.description.clone(),
                category: skill.metadata.category.clone(),
                tags: skill.metadata.tags.clone(),
                required_tools: skill.metadata.required_tools.clone(),
                version: skill.metadata.version.clone(),
            }),
            context: Some(ProtoSkillContext {
                system_prompt_additions: skill.context.system_prompt_additions.clone(),
                input_transformations: skill.context.input_transformations
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_string()))
                    .collect(),
                output_transformations: skill.context.output_transformations
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_string()))
                    .collect(),
                variables: skill.context.variables
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_string()))
                    .collect(),
                constraints: skill.context.constraints
                    .iter()
                    .map(|c| ProtoConstraint {
                        constraint_type: match c.constraint_type {
                            ConstraintType::RequireArgument => 1,
                            ConstraintType::ForbidArgument => 2,
                            ConstraintType::RequireBefore => 3,
                            ConstraintType::RequireAfter => 4,
                            ConstraintType::MaxExecutions => 5,
                            ConstraintType::RequireConfirmation => 6,
                        },
                        target: c.target.clone(),
                        value: c.value.to_string(),
                    })
                    .collect(),
            }),
            active: skill.active,
            priority: skill.priority,
        }
    }
}

#[tonic::async_trait]
impl SkillServiceTrait for SkillServiceImpl {
    async fn register(
        &self,
        request: Request<RegisterSkillRequest>,
    ) -> Result<Response<RegisterSkillResponse>, Status> {
        let req = request.into_inner();
        let proto_skill = req.skill
            .ok_or_else(|| Status::invalid_argument("Skill required"))?;

        // Convert proto to domain
        let skill = Skill {
            name: proto_skill.name.clone(),
            metadata: SkillMetadata {
                description: proto_skill.metadata.as_ref().map(|m| m.description.clone()).unwrap_or_default(),
                category: proto_skill.metadata.as_ref().map(|m| m.category.clone()).unwrap_or_default(),
                tags: proto_skill.metadata.as_ref().map(|m| m.tags.clone()).unwrap_or_default(),
                required_tools: proto_skill.metadata.as_ref().map(|m| m.required_tools.clone()).unwrap_or_default(),
                version: proto_skill.metadata.as_ref().map(|m| m.version.clone()).unwrap_or("1.0.0".to_string()),
            },
            context: SkillContext::default(),
            active: false,
            priority: proto_skill.priority,
        };

        self.registry.register(skill).await;

        info!("Registered skill via gRPC: {}", proto_skill.name);

        Ok(Response::new(RegisterSkillResponse {
            success: true,
            skill_name: proto_skill.name,
            error: String::new(),
        }))
    }

    async fn unregister(
        &self,
        request: Request<UnregisterSkillRequest>,
    ) -> Result<Response<UnregisterSkillResponse>, Status> {
        let req = request.into_inner();
        // Note: Would need to add unregister to SkillRegistry
        Ok(Response::new(UnregisterSkillResponse { success: true }))
    }

    async fn activate(
        &self,
        request: Request<ActivateSkillRequest>,
    ) -> Result<Response<ActivateSkillResponse>, Status> {
        let req = request.into_inner();
        match self.registry.activate(&req.skill_name).await {
            Ok(()) => Ok(Response::new(ActivateSkillResponse {
                success: true,
                error: String::new(),
            })),
            Err(e) => Ok(Response::new(ActivateSkillResponse {
                success: false,
                error: e.to_string(),
            })),
        }
    }

    async fn deactivate(
        &self,
        request: Request<DeactivateSkillRequest>,
    ) -> Result<Response<DeactivateSkillResponse>, Status> {
        let req = request.into_inner();
        match self.registry.deactivate(&req.skill_name).await {
            Ok(()) => Ok(Response::new(DeactivateSkillResponse { success: true })),
            Err(_) => Ok(Response::new(DeactivateSkillResponse { success: false })),
        }
    }

    async fn get_skill(
        &self,
        request: Request<GetSkillRequest>,
    ) -> Result<Response<ProtoSkill>, Status> {
        let req = request.into_inner();
        let skill = self.registry.get(&req.skill_name).await
            .ok_or_else(|| Status::not_found(format!("Skill not found: {}", req.skill_name)))?;
        Ok(Response::new(Self::skill_to_proto(&skill)))
    }

    async fn list_skills(
        &self,
        request: Request<ListSkillsRequest>,
    ) -> Result<Response<ListSkillsResponse>, Status> {
        let req = request.into_inner();
        let skills = if req.active_only {
            self.registry.active_skills().await
        } else {
            self.registry.list_all().await
        };

        Ok(Response::new(ListSkillsResponse {
            skills: skills.iter().map(Self::skill_to_proto).collect(),
        }))
    }

    async fn list_active_skills(
        &self,
        _request: Request<()>,
    ) -> Result<Response<ListSkillsResponse>, Status> {
        let skills = self.registry.active_skills().await;
        Ok(Response::new(ListSkillsResponse {
            skills: skills.iter().map(Self::skill_to_proto).collect(),
        }))
    }

    async fn list_by_category(
        &self,
        request: Request<ListByCategoryRequest>,
    ) -> Result<Response<ListSkillsResponse>, Status> {
        let req = request.into_inner();
        let skills = self.registry.list_by_category(&req.category).await;
        Ok(Response::new(ListSkillsResponse {
            skills: skills.iter().map(Self::skill_to_proto).collect(),
        }))
    }

    async fn get_combined_context(
        &self,
        _request: Request<()>,
    ) -> Result<Response<ProtoSkillContext>, Status> {
        let context = self.registry.combined_context().await;
        Ok(Response::new(ProtoSkillContext {
            system_prompt_additions: context.system_prompt_additions,
            input_transformations: context.input_transformations
                .into_iter()
                .map(|(k, v)| (k, v.to_string()))
                .collect(),
            output_transformations: context.output_transformations
                .into_iter()
                .map(|(k, v)| (k, v.to_string()))
                .collect(),
            variables: context.variables
                .into_iter()
                .map(|(k, v)| (k, v.to_string()))
                .collect(),
            constraints: context.constraints
                .into_iter()
                .map(|c| ProtoConstraint {
                    constraint_type: match c.constraint_type {
                        ConstraintType::RequireArgument => 1,
                        _ => 0,
                    },
                    target: c.target,
                    value: c.value.to_string(),
                })
                .collect(),
        }))
    }

    async fn check_constraints(
        &self,
        request: Request<CheckConstraintsRequest>,
    ) -> Result<Response<CheckConstraintsResponse>, Status> {
        let req = request.into_inner();
        let args: serde_json::Value = serde_json::from_slice(&req.arguments)
            .unwrap_or(serde_json::Value::Null);

        match self.registry.check_constraints(&req.tool_name, &args).await {
            Ok(warnings) => Ok(Response::new(CheckConstraintsResponse {
                valid: true,
                violations: vec![],
                warnings,
            })),
            Err(e) => Ok(Response::new(CheckConstraintsResponse {
                valid: false,
                violations: vec![e.to_string()],
                warnings: vec![],
            })),
        }
    }

    async fn transform_input(
        &self,
        request: Request<TransformRequest>,
    ) -> Result<Response<TransformResponse>, Status> {
        let req = request.into_inner();
        let data: serde_json::Value = serde_json::from_slice(&req.data)
            .unwrap_or(serde_json::Value::Null);

        let transformed = self.registry.transform_input(&req.tool_name, data).await;
        let active: Vec<String> = self.registry.active_skills().await
            .iter()
            .map(|s| s.name.clone())
            .collect();

        Ok(Response::new(TransformResponse {
            data: serde_json::to_vec(&transformed).unwrap_or_default(),
            skills_applied: active,
        }))
    }

    async fn transform_output(
        &self,
        request: Request<TransformRequest>,
    ) -> Result<Response<TransformResponse>, Status> {
        let req = request.into_inner();
        let data: serde_json::Value = serde_json::from_slice(&req.data)
            .unwrap_or(serde_json::Value::Null);

        let transformed = self.registry.transform_output(&req.tool_name, data).await;
        let active: Vec<String> = self.registry.active_skills().await
            .iter()
            .map(|s| s.name.clone())
            .collect();

        Ok(Response::new(TransformResponse {
            data: serde_json::to_vec(&transformed).unwrap_or_default(),
            skills_applied: active,
        }))
    }
}
```

---

## Usage Example

```rust
use op_cache::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create skill registry with defaults
    let skill_registry = Arc::new(SkillRegistry::with_defaults());
    
    // Create agent registry with skill support
    let agent_registry = Arc::new(AgentRegistry::new(skill_registry.clone()));
    
    // Register agents
    let analyzer = Agent::new("analyzer", "Code Analyzer")
        .with_capabilities(&[Capability::CodeAnalysis, Capability::SecurityAudit]);
    agent_registry.register(analyzer, Arc::new(|input| Ok(input.to_vec()))).await?;
    
    // Create orchestrator
    let orchestrator = Orchestrator::new(
        std::path::PathBuf::from("/var/cache/op-cache"),
        agent_registry.clone(),
        skill_registry.clone(),
        OrchestratorConfig::default(),
    ).await?;
    
    // Execute with skills
    let request = CapabilityRequest::new(
        vec![Capability::CodeAnalysis, Capability::SecurityAudit],
        b"fn main() { let x = 1; }".to_vec(),
    ).with_skills(vec!["security_audit".to_string()]);
    
    let result = orchestrator.execute(request).await?;
    
    println!("Agents: {:?}", result.resolved_agents);
    println!("Skills applied: {:?}", result.skills_applied);
    println!("Used workstack: {}", result.used_workstack);
    
    Ok(())
}
```

---

## Summary

### Key Points

1. **NO LAZY** — Everything initialized eagerly at startup
2. **Capabilities stored at registration** — Agents declare capabilities when registered
3. **Skills augment execution** — System prompts, transformations, constraints
4. **Skills integrated into agent execution** — Registry applies skills automatically
5. **gRPC exposes all functionality** — Including full skill management
6. **MCP via gRPC proxy** — Thin shim connects to daemon

### Skill Features

- **System prompt additions** — Inject domain knowledge
- **Input transformations** — Modify arguments before execution
- **Output transformations** — Post-process results
- **Constraints** — RequireArgument, ForbidArgument, RequireConfirmation, etc.
- **Variables** — Skill-specific configuration
- **Priority ordering** — Higher priority skills applied first

### Build Order

1. Update op-cache Cargo.toml
2. Create proto/op_cache.proto (with Skills)
3. Create build.rs
4. Create src/skill.rs
5. Create src/agent.rs (with skill integration)
6. Create src/orchestrator.rs (with skill support)
7. Create src/workstack_cache.rs
8. Create src/pattern_tracker.rs
9. Update src/lib.rs
10. Create gRPC services (including skill_service.rs)
11. Create mcp-proxy crate
