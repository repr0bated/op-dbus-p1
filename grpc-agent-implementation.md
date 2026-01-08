# Agent Capabilities + Workstack Orchestration + MCP-gRPC Integration

## Overview

Extend the existing `op-cache` crate and create supporting infrastructure:

1. **Agents with capabilities array** — declared at registration time
2. **Orchestrator** — resolves capabilities → agents, routes to workstack if 2+ agents
3. **Workstack cache** — caches intermediate step results
4. **Pattern tracker** — detects frequent sequences, suggests promotion
5. **gRPC services** — op-dbus daemon exposes everything via gRPC
6. **MCP proxy** — thin shim spawned by MCP clients, connects to daemon via gRPC
7. **NO LAZY PATTERNS** — eager initialization everywhere

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
│  ┌─────────────┬─────────────┬─────────────┬─────────────┐     │
│  │ AgentService│Orchestrator │ CacheService│ MCPService  │     │
│  │             │   Service   │             │             │     │
│  └─────────────┴─────────────┴─────────────┴─────────────┘     │
│                              │                                  │
│  ┌───────────────────────────┴───────────────────────────┐     │
│  │              Core Components (op-cache)                │     │
│  │  AgentRegistry │ Orchestrator │ WorkstackCache │ BTRFS │     │
│  └────────────────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────────────┘
```

---

## File Structure

```
crates/
├── op-cache/                      # Core library
│   ├── src/
│   │   ├── lib.rs                 # UPDATE: add modules, NO LAZY
│   │   ├── agent.rs               # NEW: Agent + capabilities array
│   │   ├── orchestrator.rs        # NEW: Routes requests
│   │   ├── workstack_cache.rs     # NEW: Step caching
│   │   ├── pattern_tracker.rs     # NEW: Sequence tracking
│   │   ├── btrfs_cache.rs         # EXISTING: add workstacks/ subvol
│   │   ├── numa.rs                # EXISTING: unchanged
│   │   └── snapshot_manager.rs    # EXISTING: unchanged
│   ├── proto/
│   │   └── op_cache.proto         # NEW: gRPC definitions
│   ├── build.rs                   # NEW: tonic-build
│   └── Cargo.toml                 # UPDATE: add deps
│
├── op-dbus/                       # Daemon (existing)
│   └── src/
│       ├── main.rs                # UPDATE: start gRPC server
│       └── grpc/                  # NEW: gRPC service impls
│           ├── mod.rs
│           ├── agent_service.rs
│           ├── orchestrator_service.rs
│           ├── cache_service.rs
│           ├── mcp_service.rs     # NEW: MCP tool handlers
│           └── server.rs
│
└── mcp-proxy/                     # NEW: Thin MCP shim
    ├── src/
    │   └── main.rs                # Spawned by MCP clients
    └── Cargo.toml
```

---

## IMPORTANT: NO LAZY PATTERNS

**DO NOT USE:**
- `lazy_static!`
- `once_cell::Lazy`
- `OnceCell`
- Deferred initialization

**INSTEAD USE:**
- Eager initialization in constructors
- Pass dependencies explicitly
- Initialize at startup, fail fast

```rust
// BAD - lazy
lazy_static! {
    static ref REGISTRY: AgentRegistry = AgentRegistry::new();
}

// GOOD - eager, explicit
pub struct App {
    registry: Arc<AgentRegistry>,  // Initialized at startup
}

impl App {
    pub fn new() -> Result<Self> {
        let registry = Arc::new(AgentRegistry::new());  // Eager
        Ok(Self { registry })
    }
}
```

---

## Step 1: Update Cargo.toml (op-cache)

```toml
[package]
name = "op-cache"
version = "0.1.0"
edition = "2021"

[dependencies]
# Existing
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

# New
uuid = { version = "1.0", features = ["v4"] }
prost = "0.12"
tonic = "0.11"
tokio-stream = "0.1"

[build-dependencies]
tonic-build = "0.11"

[dev-dependencies]
tempfile = "3.10"
```

---

## Step 2: Create proto/op_cache.proto

```protobuf
syntax = "proto3";
package op_cache;

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
// Agent Service
// ============================================================================

service AgentService {
    rpc Register(RegisterAgentRequest) returns (RegisterAgentResponse);
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

message ExecuteAgentRequest {
    string agent_id = 1;
    bytes input = 2;
}

message ExecuteAgentResponse {
    bytes output = 1;
    uint64 latency_ms = 2;
    bool success = 3;
    string error = 4;
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
}

message ExecuteAgentsRequest {
    repeated string agent_ids = 1;
    bytes input = 2;
}

message StepResult {
    uint32 step_index = 1;
    string agent_id = 2;
    uint64 latency_ms = 3;
    bool cached = 4;
    uint64 output_size = 5;
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
// Cache Service (Memory Functions)
// ============================================================================

service CacheService {
    // Embedding cache
    rpc GetEmbedding(GetEmbeddingRequest) returns (GetEmbeddingResponse);
    rpc PutEmbedding(PutEmbeddingRequest) returns (PutEmbeddingResponse);
    
    // Workstack cache
    rpc GetWorkstackStep(GetWorkstackStepRequest) returns (GetWorkstackStepResponse);
    rpc PutWorkstackStep(PutWorkstackStepRequest) returns (PutWorkstackStepResponse);
    rpc InvalidateWorkstack(InvalidateWorkstackRequest) returns (InvalidateWorkstackResponse);
    
    // Stats and management
    rpc GetStats(GetStatsRequest) returns (CacheStats);
    rpc Cleanup(CleanupRequest) returns (CleanupResponse);
    rpc Clear(ClearRequest) returns (ClearResponse);
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
    uint64 embeddings_size_bytes = 5;
    uint64 workstack_cache_entries = 6;
    double hit_rate = 7;
}

message CleanupRequest {
    int64 max_age_days = 1;
}

message CleanupResponse {
    uint32 entries_removed = 1;
    uint64 bytes_freed = 2;
}

message ClearRequest {
    bool embeddings = 1;
    bool workstacks = 2;
    bool blocks = 3;
}

message ClearResponse {
    bool success = 1;
}

// ============================================================================
// MCP Service (Tool Handlers)
// ============================================================================

service MCPService {
    // MCP JSON-RPC passthrough
    rpc HandleRequest(MCPRequest) returns (MCPResponse);
    
    // Tool list for MCP initialization
    rpc ListTools(ListToolsRequest) returns (ListToolsResponse);
}

message MCPRequest {
    string jsonrpc = 1;  // "2.0"
    string method = 2;   // "tools/call", "tools/list", etc.
    string id = 3;
    bytes params = 4;    // JSON params as bytes
}

message MCPResponse {
    string jsonrpc = 1;
    string id = 2;
    bytes result = 3;    // JSON result as bytes
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
    bytes input_schema = 3;  // JSON schema as bytes
}
```

---

## Step 3: Create build.rs (op-cache)

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile(&["proto/op_cache.proto"], &["proto"])?;
    Ok(())
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

/// Capability enum — what an agent can do
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Priority {
    High = 0,
    Normal = 1,
    Low = 2,
}

impl Default for Priority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Agent definition with capabilities ARRAY
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub description: String,
    pub capabilities: Vec<Capability>,  // STORED AT REGISTRATION
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
            capabilities: Vec::new(),  // EAGER empty vec
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

/// Agent registry — NO LAZY, all eager
pub struct AgentRegistry {
    // Primary storage
    agents: RwLock<HashMap<String, RegisteredAgent>>,
    // Reverse index: capability -> agent IDs
    capability_index: RwLock<HashMap<Capability, Vec<String>>>,
}

impl AgentRegistry {
    /// Create new EMPTY registry — eager initialization
    pub fn new() -> Self {
        Self {
            agents: RwLock::new(HashMap::new()),           // EAGER
            capability_index: RwLock::new(HashMap::new()), // EAGER
        }
    }
    
    /// Register agent — stores capabilities at registration time
    pub async fn register(&self, agent: Agent, executor: AgentExecutor) -> Result<()> {
        let agent_id = agent.id.clone();
        let capabilities = agent.capabilities.clone();
        
        // Store agent
        {
            let mut agents = self.agents.write().await;
            agents.insert(
                agent_id.clone(),
                RegisteredAgent { definition: agent, executor },
            );
        }
        
        // Update capability index — THIS IS THE KEY
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
    
    /// Find agents by capability — O(1) lookup via index
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
    
    /// Find agents by multiple capabilities
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
    
    /// Execute agent
    pub async fn execute(&self, agent_id: &str, input: &[u8]) -> Result<Vec<u8>> {
        let executor = {
            let agents = self.agents.read().await;
            agents
                .get(agent_id)
                .map(|a| a.executor.clone())
                .context(format!("Agent not found: {}", agent_id))?
        };
        
        executor(input)
    }
    
    /// List all agents
    pub async fn list_all(&self) -> Vec<Agent> {
        let agents = self.agents.read().await;
        agents.values().map(|a| a.definition.clone()).collect()
    }
    
    /// List available capabilities
    pub async fn list_capabilities(&self) -> Vec<Capability> {
        let index = self.capability_index.read().await;
        index.keys().copied().collect()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

---

## Step 5: Create src/orchestrator.rs

```rust
//! Orchestrator — routes requests based on agent count
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
use crate::workstack_cache::WorkstackCache;

/// Request with capabilities
pub struct CapabilityRequest {
    pub required_capabilities: Vec<Capability>,
    pub input: Vec<u8>,
    pub preferred_agents: Vec<String>,
    pub excluded_agents: Vec<String>,
}

impl CapabilityRequest {
    pub fn new(capabilities: Vec<Capability>, input: Vec<u8>) -> Self {
        Self {
            required_capabilities: capabilities,
            input,
            preferred_agents: Vec::new(),
            excluded_agents: Vec::new(),
        }
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
}

#[derive(Debug, Clone)]
pub struct StepResult {
    pub step_index: usize,
    pub agent_id: String,
    pub latency_ms: u64,
    pub cached: bool,
    pub output_size: usize,
}

/// Orchestrator config
pub struct OrchestratorConfig {
    pub workstack_threshold: usize,
    pub enable_caching: bool,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            workstack_threshold: 2,
            enable_caching: true,
        }
    }
}

/// Orchestrator — NO LAZY, all components initialized eagerly
pub struct Orchestrator {
    registry: Arc<AgentRegistry>,
    cache: Arc<WorkstackCache>,
    pattern_tracker: Arc<PatternTracker>,
    config: OrchestratorConfig,
}

impl Orchestrator {
    /// Create orchestrator — EAGER initialization of all components
    pub async fn new(
        cache_dir: PathBuf,
        registry: Arc<AgentRegistry>,
        config: OrchestratorConfig,
    ) -> Result<Self> {
        // EAGER: Create cache immediately
        let cache = WorkstackCache::new(cache_dir.clone()).await?;
        
        // EAGER: Create tracker immediately  
        let pattern_tracker = PatternTracker::new(cache_dir).await?;
        
        Ok(Self {
            registry,
            cache: Arc::new(cache),
            pattern_tracker: Arc::new(pattern_tracker),
            config,
        })
    }
    
    /// Main entry point
    pub async fn execute(&self, request: CapabilityRequest) -> Result<ExecutionResult> {
        let start = Instant::now();
        let request_id = uuid::Uuid::new_v4().to_string();
        
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
            });
        }
        
        let agent_ids: Vec<String> = agents.iter().map(|a| a.id.clone()).collect();
        
        // Route based on count
        if agents.len() >= self.config.workstack_threshold {
            self.execute_workstack(&request_id, &agents, request.input, start).await
        } else {
            self.execute_single(&request_id, &agents[0], request.input, start).await
        }
    }
    
    /// Execute explicit agent sequence
    pub async fn execute_agents(
        &self,
        agent_ids: &[&str],
        input: Vec<u8>,
    ) -> Result<ExecutionResult> {
        let start = Instant::now();
        let request_id = uuid::Uuid::new_v4().to_string();
        
        if agent_ids.is_empty() {
            return Ok(ExecutionResult {
                request_id,
                output: input,
                steps: Vec::new(),
                total_latency_ms: 0,
                cache_hits: 0,
                cache_misses: 0,
                used_workstack: false,
                resolved_agents: Vec::new(),
            });
        }
        
        // Get agent definitions
        let mut agents = Vec::new();
        for id in agent_ids {
            if let Some(agent) = self.registry.get(id).await {
                agents.push(agent);
            }
        }
        
        if agents.len() >= self.config.workstack_threshold {
            self.execute_workstack(&request_id, &agents, input, start).await
        } else if !agents.is_empty() {
            self.execute_single(&request_id, &agents[0], input, start).await
        } else {
            anyhow::bail!("No valid agents found")
        }
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
        let output = self.registry.execute(&agent.id, &input).await?;
        
        Ok(ExecutionResult {
            request_id: request_id.to_string(),
            output,
            steps: vec![StepResult {
                step_index: 0,
                agent_id: agent.id.clone(),
                latency_ms: step_start.elapsed().as_millis() as u64,
                cached: false,
                output_size: 0,
            }],
            total_latency_ms: start.elapsed().as_millis() as u64,
            cache_hits: 0,
            cache_misses: 1,
            used_workstack: false,
            resolved_agents: vec![agent.id.clone()],
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
        
        for (step_index, agent) in agents.iter().enumerate() {
            let input_hash = Self::hash(&current_input);
            let step_start = Instant::now();
            
            let (output, cached) = if self.config.enable_caching {
                match self.cache.get(&workstack_id, step_index, &input_hash)? {
                    Some(cached) => {
                        cache_hits += 1;
                        (cached, true)
                    }
                    None => {
                        cache_misses += 1;
                        let output = self.registry.execute(&agent.id, &current_input).await?;
                        self.cache.put(&workstack_id, step_index, &input_hash, &output, None)?;
                        (output, false)
                    }
                }
            } else {
                (self.registry.execute(&agent.id, &current_input).await?, false)
            };
            
            steps.push(StepResult {
                step_index,
                agent_id: agent.id.clone(),
                latency_ms: step_start.elapsed().as_millis() as u64,
                cached,
                output_size: output.len(),
            });
            
            current_input = output;
        }
        
        let total_latency_ms = start.elapsed().as_millis() as u64;
        
        // Track pattern
        let agent_ids: Vec<&str> = agents.iter().map(|a| a.id.as_str()).collect();
        let _ = self.pattern_tracker.record_sequence(&agent_ids, total_latency_ms);
        
        info!(
            "Workstack {} completed: {} agents, {} cache hits",
            workstack_id, agents.len(), cache_hits
        );
        
        Ok(ExecutionResult {
            request_id: request_id.to_string(),
            output: current_input,
            steps,
            total_latency_ms,
            cache_hits,
            cache_misses,
            used_workstack: true,
            resolved_agents: agents.iter().map(|a| a.id.clone()).collect(),
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
    
    pub fn cache(&self) -> &Arc<WorkstackCache> {
        &self.cache
    }
    
    pub fn pattern_tracker(&self) -> &Arc<PatternTracker> {
        &self.pattern_tracker
    }
}
```

---

## Step 6: Create src/workstack_cache.rs

```rust
//! Workstack step caching
//!
//! NO LAZY.

use anyhow::{Context, Result};
use rusqlite::OptionalExtension;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::debug;

pub struct WorkstackCache {
    cache_dir: PathBuf,
    db: Mutex<rusqlite::Connection>,
    default_ttl_secs: i64,
}

impl WorkstackCache {
    /// Create cache — EAGER initialization
    pub async fn new(cache_dir: PathBuf) -> Result<Self> {
        let workstacks_dir = cache_dir.join("workstacks");
        let data_dir = workstacks_dir.join("data");
        
        // EAGER: Create dirs immediately
        tokio::fs::create_dir_all(&data_dir).await?;
        
        let db_path = workstacks_dir.join("cache.db");
        
        // EAGER: Open DB immediately
        let db = rusqlite::Connection::open(&db_path)?;
        
        // EAGER: Create tables immediately
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS step_cache (
                cache_key TEXT PRIMARY KEY,
                workstack_id TEXT NOT NULL,
                step_index INTEGER NOT NULL,
                input_hash TEXT NOT NULL,
                output_file TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                access_count INTEGER DEFAULT 1,
                size_bytes INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_workstack ON step_cache(workstack_id);
            CREATE INDEX IF NOT EXISTS idx_expires ON step_cache(expires_at);"
        )?;
        
        Ok(Self {
            cache_dir: workstacks_dir,
            db: Mutex::new(db),
            default_ttl_secs: 3600,
        })
    }
    
    pub fn get(
        &self,
        workstack_id: &str,
        step_index: usize,
        input_hash: &str,
    ) -> Result<Option<Vec<u8>>> {
        let cache_key = self.make_key(workstack_id, step_index, input_hash);
        let now = chrono::Utc::now().timestamp();
        
        let db = self.db.lock().unwrap();
        
        let entry: Option<(String, i64)> = db
            .query_row(
                "SELECT output_file, expires_at FROM step_cache WHERE cache_key = ?1",
                [&cache_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        
        let (output_file, expires_at) = match entry {
            Some(e) => e,
            None => return Ok(None),
        };
        
        if now > expires_at {
            return Ok(None);
        }
        
        db.execute(
            "UPDATE step_cache SET access_count = access_count + 1 WHERE cache_key = ?1",
            [&cache_key],
        )?;
        
        drop(db);
        
        let data_path = self.cache_dir.join("data").join(&output_file);
        let data = std::fs::read(&data_path)?;
        
        debug!("Cache hit: {} step {}", workstack_id, step_index);
        Ok(Some(data))
    }
    
    pub fn put(
        &self,
        workstack_id: &str,
        step_index: usize,
        input_hash: &str,
        output: &[u8],
        ttl_secs: Option<i64>,
    ) -> Result<()> {
        let cache_key = self.make_key(workstack_id, step_index, input_hash);
        let now = chrono::Utc::now().timestamp();
        let ttl = ttl_secs.unwrap_or(self.default_ttl_secs);
        let expires_at = now + ttl;
        
        let output_file = format!("{}.dat", cache_key);
        let data_path = self.cache_dir.join("data").join(&output_file);
        std::fs::write(&data_path, output)?;
        
        let db = self.db.lock().unwrap();
        db.execute(
            "INSERT INTO step_cache (cache_key, workstack_id, step_index, input_hash, output_file, created_at, expires_at, size_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(cache_key) DO UPDATE SET expires_at = ?7, access_count = access_count + 1",
            rusqlite::params![cache_key, workstack_id, step_index, input_hash, output_file, now, expires_at, output.len()],
        )?;
        
        Ok(())
    }
    
    pub fn invalidate_workstack(&self, workstack_id: &str) -> Result<usize> {
        let db = self.db.lock().unwrap();
        
        let mut stmt = db.prepare("SELECT output_file FROM step_cache WHERE workstack_id = ?1")?;
        let files: Vec<String> = stmt
            .query_map([workstack_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        
        let count = files.len();
        db.execute("DELETE FROM step_cache WHERE workstack_id = ?1", [workstack_id])?;
        drop(stmt);
        drop(db);
        
        for file in files {
            let _ = std::fs::remove_file(self.cache_dir.join("data").join(&file));
        }
        
        Ok(count)
    }
    
    pub fn stats(&self) -> Result<WorkstackCacheStats> {
        let db = self.db.lock().unwrap();
        let total: u64 = db.query_row("SELECT COUNT(*) FROM step_cache", [], |row| row.get(0))?;
        let size: u64 = db.query_row("SELECT COALESCE(SUM(size_bytes), 0) FROM step_cache", [], |row| row.get(0))?;
        Ok(WorkstackCacheStats { total_entries: total, total_size_bytes: size })
    }
    
    fn make_key(&self, workstack_id: &str, step_index: usize, input_hash: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(format!("{}:{}:{}", workstack_id, step_index, input_hash).as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

#[derive(Debug, Clone)]
pub struct WorkstackCacheStats {
    pub total_entries: u64,
    pub total_size_bytes: u64,
}
```

---

## Step 7: Create src/pattern_tracker.rs

```rust
//! Pattern tracking for promotion suggestions
//!
//! NO LAZY.

use anyhow::Result;
use rusqlite::OptionalExtension;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::info;

#[derive(Debug, Clone)]
pub struct TrackedPattern {
    pub pattern_id: String,
    pub agent_sequence: Vec<String>,
    pub call_count: u32,
    pub avg_latency_ms: u64,
}

#[derive(Debug, Clone)]
pub struct PromotionSuggestion {
    pub pattern: TrackedPattern,
    pub suggested_name: String,
}

pub struct PatternTracker {
    db: Mutex<rusqlite::Connection>,
    promotion_threshold: u32,
}

impl PatternTracker {
    /// Create tracker — EAGER initialization
    pub async fn new(cache_dir: PathBuf) -> Result<Self> {
        tokio::fs::create_dir_all(&cache_dir).await?;
        
        let db_path = cache_dir.join("patterns.db");
        
        // EAGER: Open DB immediately
        let db = rusqlite::Connection::open(&db_path)?;
        
        // EAGER: Create tables immediately
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS patterns (
                pattern_hash TEXT PRIMARY KEY,
                agent_sequence TEXT NOT NULL,
                call_count INTEGER DEFAULT 1,
                first_seen INTEGER NOT NULL,
                last_called INTEGER NOT NULL,
                total_latency_ms INTEGER DEFAULT 0,
                promoted INTEGER DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_count ON patterns(call_count DESC);"
        )?;
        
        Ok(Self {
            db: Mutex::new(db),
            promotion_threshold: 3,
        })
    }
    
    pub fn record_sequence(
        &self,
        agents: &[&str],
        total_latency_ms: u64,
    ) -> Result<Option<PromotionSuggestion>> {
        if agents.len() < 2 {
            return Ok(None);
        }
        
        let hash = self.hash_sequence(agents);
        let seq_json = serde_json::to_string(agents)?;
        let now = chrono::Utc::now().timestamp();
        
        let db = self.db.lock().unwrap();
        
        let existing: Option<(u32, i64, bool)> = db
            .query_row(
                "SELECT call_count, total_latency_ms, promoted FROM patterns WHERE pattern_hash = ?1",
                [&hash],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        
        let (count, total_lat, promoted) = if let Some((c, l, p)) = existing {
            db.execute(
                "UPDATE patterns SET call_count = call_count + 1, last_called = ?1, total_latency_ms = total_latency_ms + ?2 WHERE pattern_hash = ?3",
                rusqlite::params![now, total_latency_ms, hash],
            )?;
            (c + 1, l + total_latency_ms as i64, p)
        } else {
            db.execute(
                "INSERT INTO patterns (pattern_hash, agent_sequence, first_seen, last_called, total_latency_ms) VALUES (?1, ?2, ?3, ?3, ?4)",
                rusqlite::params![hash, seq_json, now, total_latency_ms],
            )?;
            (1, total_latency_ms as i64, false)
        };
        
        if count >= self.promotion_threshold && !promoted {
            let pattern = TrackedPattern {
                pattern_id: hash,
                agent_sequence: agents.iter().map(|s| s.to_string()).collect(),
                call_count: count,
                avg_latency_ms: (total_lat / count as i64) as u64,
            };
            
            info!("Pattern detected: {:?} ({} calls)", pattern.agent_sequence, count);
            
            return Ok(Some(PromotionSuggestion {
                suggested_name: self.generate_name(&pattern.agent_sequence),
                pattern,
            }));
        }
        
        Ok(None)
    }
    
    pub fn get_candidates(&self) -> Result<Vec<PromotionSuggestion>> {
        let db = self.db.lock().unwrap();
        let mut stmt = db.prepare(
            "SELECT pattern_hash, agent_sequence, call_count, total_latency_ms FROM patterns WHERE call_count >= ?1 AND promoted = 0"
        )?;
        
        let patterns = stmt
            .query_map([self.promotion_threshold], |row| {
                let hash: String = row.get(0)?;
                let seq_json: String = row.get(1)?;
                let count: u32 = row.get(2)?;
                let total: i64 = row.get(3)?;
                Ok((hash, seq_json, count, total))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        
        Ok(patterns
            .into_iter()
            .filter_map(|(hash, seq_json, count, total)| {
                let agents: Vec<String> = serde_json::from_str(&seq_json).ok()?;
                Some(PromotionSuggestion {
                    suggested_name: self.generate_name(&agents),
                    pattern: TrackedPattern {
                        pattern_id: hash,
                        agent_sequence: agents,
                        call_count: count,
                        avg_latency_ms: if count > 0 { (total / count as i64) as u64 } else { 0 },
                    },
                })
            })
            .collect())
    }
    
    fn hash_sequence(&self, agents: &[&str]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(agents.join("→").as_bytes());
        format!("{:x}", hasher.finalize())
    }
    
    fn generate_name(&self, agents: &[String]) -> String {
        if agents.len() < 2 {
            return "unnamed".to_string();
        }
        format!("{}-to-{}", agents[0], agents.last().unwrap())
    }
}
```

---

## Step 8: Update src/lib.rs

```rust
//! op-cache: BTRFS-based caching with agent orchestration
//!
//! NO LAZY PATTERNS — all initialization is eager.

pub mod agent;
pub mod btrfs_cache;
pub mod numa;
pub mod orchestrator;
pub mod pattern_tracker;
pub mod snapshot_manager;
pub mod workstack_cache;

pub use agent::{Agent, AgentRegistry, Capability, Priority};
pub use btrfs_cache::BtrfsCache;
pub use numa::{NumaNode, NumaTopology};
pub use orchestrator::{CapabilityRequest, ExecutionResult, Orchestrator, OrchestratorConfig};
pub use pattern_tracker::PatternTracker;
pub use snapshot_manager::SnapshotManager;
pub use workstack_cache::WorkstackCache;

// Include generated protobuf code
pub mod proto {
    tonic::include_proto!("op_cache");
}

pub mod prelude {
    pub use super::agent::{Agent, AgentRegistry, Capability, Priority};
    pub use super::btrfs_cache::BtrfsCache;
    pub use super::orchestrator::{CapabilityRequest, ExecutionResult, Orchestrator};
    pub use super::workstack_cache::WorkstackCache;
}
```

---

## Step 9: Create crates/mcp-proxy/

### Cargo.toml

```toml
[package]
name = "mcp-proxy"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.0", features = ["full"] }
tonic = "0.11"
tracing = "0.1"
tracing-subscriber = "0.3"

op-cache = { path = "../op-cache" }
```

### src/main.rs

```rust
//! MCP Proxy — spawned by MCP clients, connects to op-dbus via gRPC
//!
//! This is a THIN SHIM:
//! - Reads JSON-RPC from stdin
//! - Forwards to op-dbus daemon via gRPC
//! - Writes responses to stdout
//!
//! NO STATE — all state lives in the daemon.
//! NO LAZY — connects immediately on startup.

use std::io::{BufRead, Write};
use tonic::transport::Channel;
use tracing::{error, info};

use op_cache::proto::mcp_service_client::McpServiceClient;
use op_cache::proto::McpRequest;

const DEFAULT_DAEMON_ADDR: &str = "http://[::1]:50051";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Setup logging to stderr (stdout is for MCP protocol)
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    
    let daemon_addr = std::env::var("OP_DBUS_ADDR")
        .unwrap_or_else(|_| DEFAULT_DAEMON_ADDR.to_string());
    
    info!("Connecting to op-dbus at {}", daemon_addr);
    
    // EAGER: Connect immediately, fail fast if daemon not running
    let channel = Channel::from_shared(daemon_addr.clone())?
        .connect()
        .await
        .map_err(|e| {
            error!("Failed to connect to op-dbus daemon at {}: {}", daemon_addr, e);
            error!("Make sure op-dbus daemon is running");
            e
        })?;
    
    let mut client = McpServiceClient::new(channel);
    
    info!("Connected to op-dbus daemon");
    
    // Read from stdin, forward to daemon, write to stdout
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                error!("Failed to read from stdin: {}", e);
                break;
            }
        };
        
        if line.trim().is_empty() {
            continue;
        }
        
        // Parse JSON-RPC request
        let json_request: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let error_response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {}", e)
                    }
                });
                writeln!(stdout, "{}", error_response)?;
                stdout.flush()?;
                continue;
            }
        };
        
        // Build gRPC request
        let grpc_request = McpRequest {
            jsonrpc: "2.0".to_string(),
            method: json_request["method"].as_str().unwrap_or("").to_string(),
            id: json_request["id"].to_string(),
            params: serde_json::to_vec(&json_request["params"]).unwrap_or_default(),
        };
        
        // Call daemon
        let response = match client.handle_request(grpc_request).await {
            Ok(resp) => resp.into_inner(),
            Err(e) => {
                let error_response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": json_request["id"],
                    "error": {
                        "code": -32603,
                        "message": format!("Internal error: {}", e)
                    }
                });
                writeln!(stdout, "{}", error_response)?;
                stdout.flush()?;
                continue;
            }
        };
        
        // Build JSON-RPC response
        let json_response = if let Some(error) = response.error {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": serde_json::from_str::<serde_json::Value>(&response.id).unwrap_or(serde_json::Value::Null),
                "error": {
                    "code": error.code,
                    "message": error.message
                }
            })
        } else {
            let result: serde_json::Value = serde_json::from_slice(&response.result)
                .unwrap_or(serde_json::Value::Null);
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": serde_json::from_str::<serde_json::Value>(&response.id).unwrap_or(serde_json::Value::Null),
                "result": result
            })
        };
        
        writeln!(stdout, "{}", json_response)?;
        stdout.flush()?;
    }
    
    Ok(())
}
```

---

## Step 10: MCP Client Configuration

### claude_desktop_config.json

```json
{
  "mcpServers": {
    "op-dbus": {
      "command": "/usr/local/bin/mcp-proxy",
      "args": [],
      "env": {
        "OP_DBUS_ADDR": "http://[::1]:50051"
      }
    }
  }
}
```

---

## Step 11: op-dbus Daemon gRPC Server

In the existing op-dbus daemon, add gRPC server startup:

### src/grpc/mod.rs

```rust
pub mod agent_service;
pub mod cache_service;
pub mod mcp_service;
pub mod orchestrator_service;
pub mod server;

pub use server::start_grpc_server;
```

### src/grpc/server.rs

```rust
use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use tonic::transport::Server;
use tracing::info;

use op_cache::proto::{
    agent_service_server::AgentServiceServer,
    cache_service_server::CacheServiceServer,
    mcp_service_server::McpServiceServer,
    orchestrator_service_server::OrchestratorServiceServer,
};

use super::agent_service::AgentServiceImpl;
use super::cache_service::CacheServiceImpl;
use super::mcp_service::McpServiceImpl;
use super::orchestrator_service::OrchestratorServiceImpl;

pub async fn start_grpc_server(
    addr: SocketAddr,
    // Pass in your existing components
    btrfs_cache: Arc<op_cache::BtrfsCache>,
    orchestrator: Arc<op_cache::Orchestrator>,
) -> Result<()> {
    let agent_service = AgentServiceImpl::new(orchestrator.registry().clone());
    let cache_service = CacheServiceImpl::new(btrfs_cache, orchestrator.cache().clone());
    let orchestrator_service = OrchestratorServiceImpl::new(orchestrator.clone());
    let mcp_service = McpServiceImpl::new(
        orchestrator.clone(),
        btrfs_cache.clone(),
    );
    
    info!("Starting gRPC server on {}", addr);
    
    Server::builder()
        .add_service(AgentServiceServer::new(agent_service))
        .add_service(CacheServiceServer::new(cache_service))
        .add_service(OrchestratorServiceServer::new(orchestrator_service))
        .add_service(McpServiceServer::new(mcp_service))
        .serve(addr)
        .await?;
    
    Ok(())
}
```

### src/grpc/mcp_service.rs

```rust
//! MCP Service — handles MCP tool calls
//!
//! This exposes all memory functions (cache, embeddings) via MCP tools.

use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::debug;

use op_cache::proto::{
    mcp_service_server::McpService,
    ListToolsRequest, ListToolsResponse, McpError, McpRequest, McpResponse, McpTool,
};
use op_cache::{BtrfsCache, Orchestrator};

pub struct McpServiceImpl {
    orchestrator: Arc<Orchestrator>,
    btrfs_cache: Arc<BtrfsCache>,
}

impl McpServiceImpl {
    pub fn new(orchestrator: Arc<Orchestrator>, btrfs_cache: Arc<BtrfsCache>) -> Self {
        Self { orchestrator, btrfs_cache }
    }
}

#[tonic::async_trait]
impl McpService for McpServiceImpl {
    async fn handle_request(
        &self,
        request: Request<McpRequest>,
    ) -> Result<Response<McpResponse>, Status> {
        let req = request.into_inner();
        debug!("MCP request: method={}", req.method);
        
        let result = match req.method.as_str() {
            "tools/list" => {
                self.handle_tools_list().await
            }
            "tools/call" => {
                let params: serde_json::Value = serde_json::from_slice(&req.params)
                    .unwrap_or(serde_json::Value::Null);
                self.handle_tool_call(&params).await
            }
            _ => {
                Err(McpError {
                    code: -32601,
                    message: format!("Method not found: {}", req.method),
                    data: Vec::new(),
                })
            }
        };
        
        match result {
            Ok(result_bytes) => Ok(Response::new(McpResponse {
                jsonrpc: "2.0".to_string(),
                id: req.id,
                result: result_bytes,
                error: None,
            })),
            Err(error) => Ok(Response::new(McpResponse {
                jsonrpc: "2.0".to_string(),
                id: req.id,
                result: Vec::new(),
                error: Some(error),
            })),
        }
    }
    
    async fn list_tools(
        &self,
        _request: Request<ListToolsRequest>,
    ) -> Result<Response<ListToolsResponse>, Status> {
        let tools = self.get_tool_definitions();
        Ok(Response::new(ListToolsResponse { tools }))
    }
}

impl McpServiceImpl {
    fn get_tool_definitions(&self) -> Vec<McpTool> {
        vec![
            // Memory/Cache tools
            McpTool {
                name: "cache_get_embedding".to_string(),
                description: "Get cached embedding for text".to_string(),
                input_schema: serde_json::to_vec(&serde_json::json!({
                    "type": "object",
                    "properties": {
                        "text": { "type": "string" }
                    },
                    "required": ["text"]
                })).unwrap(),
            },
            McpTool {
                name: "cache_put_embedding".to_string(),
                description: "Store embedding in cache".to_string(),
                input_schema: serde_json::to_vec(&serde_json::json!({
                    "type": "object",
                    "properties": {
                        "text": { "type": "string" },
                        "vector": { "type": "array", "items": { "type": "number" } }
                    },
                    "required": ["text", "vector"]
                })).unwrap(),
            },
            McpTool {
                name: "cache_stats".to_string(),
                description: "Get cache statistics".to_string(),
                input_schema: serde_json::to_vec(&serde_json::json!({
                    "type": "object",
                    "properties": {}
                })).unwrap(),
            },
            McpTool {
                name: "cache_clear".to_string(),
                description: "Clear cache".to_string(),
                input_schema: serde_json::to_vec(&serde_json::json!({
                    "type": "object",
                    "properties": {
                        "embeddings": { "type": "boolean" },
                        "workstacks": { "type": "boolean" }
                    }
                })).unwrap(),
            },
            // Agent/Orchestrator tools
            McpTool {
                name: "execute_capabilities".to_string(),
                description: "Execute request by required capabilities".to_string(),
                input_schema: serde_json::to_vec(&serde_json::json!({
                    "type": "object",
                    "properties": {
                        "capabilities": { "type": "array", "items": { "type": "string" } },
                        "input": { "type": "string" }
                    },
                    "required": ["capabilities", "input"]
                })).unwrap(),
            },
            McpTool {
                name: "list_agents".to_string(),
                description: "List registered agents and their capabilities".to_string(),
                input_schema: serde_json::to_vec(&serde_json::json!({
                    "type": "object",
                    "properties": {}
                })).unwrap(),
            },
        ]
    }
    
    async fn handle_tools_list(&self) -> Result<Vec<u8>, McpError> {
        let tools = self.get_tool_definitions();
        let result = serde_json::json!({ "tools": tools });
        Ok(serde_json::to_vec(&result).unwrap())
    }
    
    async fn handle_tool_call(&self, params: &serde_json::Value) -> Result<Vec<u8>, McpError> {
        let tool_name = params["name"].as_str().unwrap_or("");
        let arguments = &params["arguments"];
        
        match tool_name {
            "cache_get_embedding" => {
                let text = arguments["text"].as_str().unwrap_or("");
                match self.btrfs_cache.get_embedding(text) {
                    Ok(Some(vector)) => {
                        let result = serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": serde_json::to_string(&vector).unwrap()
                            }]
                        });
                        Ok(serde_json::to_vec(&result).unwrap())
                    }
                    Ok(None) => {
                        let result = serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": "Not found"
                            }]
                        });
                        Ok(serde_json::to_vec(&result).unwrap())
                    }
                    Err(e) => Err(McpError {
                        code: -32603,
                        message: format!("Cache error: {}", e),
                        data: Vec::new(),
                    }),
                }
            }
            
            "cache_put_embedding" => {
                let text = arguments["text"].as_str().unwrap_or("");
                let vector: Vec<f32> = arguments["vector"]
                    .as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect())
                    .unwrap_or_default();
                
                match self.btrfs_cache.put_embedding(text, &vector) {
                    Ok(()) => {
                        let result = serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": "Stored"
                            }]
                        });
                        Ok(serde_json::to_vec(&result).unwrap())
                    }
                    Err(e) => Err(McpError {
                        code: -32603,
                        message: format!("Cache error: {}", e),
                        data: Vec::new(),
                    }),
                }
            }
            
            "cache_stats" => {
                match self.btrfs_cache.stats() {
                    Ok(stats) => {
                        let result = serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": format!(
                                    "Total entries: {}\nHot entries: {}\nDisk usage: {} bytes",
                                    stats.total_entries,
                                    stats.hot_entries,
                                    stats.disk_usage_bytes
                                )
                            }]
                        });
                        Ok(serde_json::to_vec(&result).unwrap())
                    }
                    Err(e) => Err(McpError {
                        code: -32603,
                        message: format!("Stats error: {}", e),
                        data: Vec::new(),
                    }),
                }
            }
            
            "list_agents" => {
                let agents = self.orchestrator.registry().list_all().await;
                let result = serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&agents).unwrap()
                    }]
                });
                Ok(serde_json::to_vec(&result).unwrap())
            }
            
            _ => Err(McpError {
                code: -32601,
                message: format!("Unknown tool: {}", tool_name),
                data: Vec::new(),
            }),
        }
    }
}
```

---

## Summary

### Key Points

1. **NO LAZY** — Everything initialized eagerly at startup
2. **Capabilities stored at registration** — Agents declare capabilities when registered
3. **capability_index** — O(1) lookup for "which agents provide X"
4. **Orchestrator routes by count** — 1 agent = direct, 2+ = workstack
5. **MCP via gRPC** — mcp-proxy spawned by clients, connects to daemon
6. **Memory functions exposed** — Cache, embeddings available as MCP tools

### Build Order

1. Update op-cache Cargo.toml
2. Create proto/op_cache.proto
3. Create build.rs
4. Create src/agent.rs
5. Create src/orchestrator.rs
6. Create src/workstack_cache.rs
7. Create src/pattern_tracker.rs
8. Update src/lib.rs
9. Create crates/mcp-proxy/
10. Add gRPC server to op-dbus daemon

### Test

```bash
# Start daemon
cargo run --bin op-dbus

# Test mcp-proxy
echo '{"jsonrpc":"2.0","method":"tools/list","id":1}' | cargo run --bin mcp-proxy
```
