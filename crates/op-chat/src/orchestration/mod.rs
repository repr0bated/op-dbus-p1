//! Orchestration Module - Unified execution coordination
//!
//! This module provides:
//! - Skills: Knowledge/capability augmentation for tools
//! - Workstacks: Multi-phase execution plans (agents + tools)
//! - Workflows: Step-by-step execution with conditions (tool sequences)
//! - Multi-Agent Coordination: Parallel agent execution
//! - D-Bus Orchestrator Integration: System-level agent management
//!
//! ## Key Distinction: Workflows vs Workstacks
//!
//! ### Workflows (op-workflows crate)
//! - Flow-based programming with plugins/services as NODES
//! - Data flows through NodePorts
//! - PocketFlow-style visual programming
//! - Used for infrastructure automation
//!
//! ### Workstacks (this module)
//! - Multi-phase execution plans combining AGENTS + TOOLS
//! - Phase-based with dependencies
//! - Skills provide knowledge augmentation
//! - Used for LLM-orchestrated complex tasks
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                  OrchestratedExecutor                        │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                              │
//! │  ExecutionMode Detection:                                   │
//! │  • workstack_* → WorkstackExecutor (agents + tools)        │
//! │  • skill_*     → SkillRegistry + tool execution            │
//! │  • workflow_*  → WorkflowEngine (plugins/services as nodes)│
//! │  • direct      → TrackedToolExecutor                       │
//! │                                                              │
//! └─────────────────────────────────────────────────────────────┘
//! ```

pub mod coordinator;
pub mod dbus_orchestrator;
pub mod executor;
pub mod skills;
pub mod workflows;
pub mod workstacks;

pub use coordinator::{AgentCoordinator, AgentTask, CoordinationStrategy, TaskResult};
pub use dbus_orchestrator::{DbusOrchestrator, OrchestratorConfig};
pub use executor::{ExecutionMode, OrchestratedExecutor, OrchestratedResult};
pub use skills::{Skill, SkillContext, SkillMetadata, SkillRegistry, DisclosureLevel};
pub use workflows::{Workflow, WorkflowEngine, WorkflowStep, WorkflowVariable};
pub use workstacks::{
    Workstack, WorkstackExecutor, WorkstackPhase, WorkstackRegistry,
    WorkstackContext, PhaseToolCall, PhaseStatus, PhaseResult, WorkstackResult,
    ToolExecutorTrait,
};

/// Coordination mode for multi-agent execution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinationMode {
    /// Execute agents sequentially
    Sequential,
    /// Execute agents in parallel
    Parallel,
    /// Execute based on dependencies
    Dependency,
    /// Round-robin distribution
    RoundRobin,
}

/// Built-in workstacks for common tasks
pub fn builtin_workstacks() -> Vec<Workstack> {
    vec![
        // OVS Network Setup
        Workstack::new(
            "ovs_network_setup",
            "OVS Network Setup",
            "Set up OVS bridge with ports and flows",
        )
        .with_phase(WorkstackPhase {
            id: "create_bridge".to_string(),
            name: "Create Bridge".to_string(),
            description: "Create the OVS bridge".to_string(),
            tools: vec![PhaseToolCall {
                tool: "ovs_create_bridge".to_string(),
                arguments: serde_json::json!({ "bridge": "${bridge_name}" }),
                store_as: Some("bridge_result".to_string()),
                retries: 1,
            }],
            agents: vec![],
            depends_on: vec![],
            condition: None,
            rollback: vec![PhaseToolCall {
                tool: "ovs_delete_bridge".to_string(),
                arguments: serde_json::json!({ "bridge": "${bridge_name}" }),
                store_as: None,
                retries: 0,
            }],
            continue_on_failure: false,
            timeout_secs: 60,
            status: PhaseStatus::Pending,
            result: None,
            error: None,
        })
        .with_phase(WorkstackPhase {
            id: "verify".to_string(),
            name: "Verify Setup".to_string(),
            description: "Verify the bridge configuration".to_string(),
            tools: vec![PhaseToolCall {
                tool: "ovs_list_bridges".to_string(),
                arguments: serde_json::json!({}),
                store_as: Some("final_bridges".to_string()),
                retries: 0,
            }],
            agents: vec![],
            depends_on: vec!["create_bridge".to_string()],
            condition: None,
            rollback: vec![],
            continue_on_failure: false,
            timeout_secs: 30,
            status: PhaseStatus::Pending,
            result: None,
            error: None,
        }),

        // Full Stack Feature Development
        Workstack::new(
            "full_stack_feature",
            "Full Stack Feature",
            "Develop a full-stack feature with multiple agents",
        )
        .with_phase(WorkstackPhase {
            id: "analyze".to_string(),
            name: "Analyze Requirements".to_string(),
            description: "Analyze feature requirements".to_string(),
            tools: vec![PhaseToolCall {
                tool: "file_read".to_string(),
                arguments: serde_json::json!({ "path": "${requirements_file}" }),
                store_as: Some("requirements".to_string()),
                retries: 0,
            }],
            agents: vec!["python-pro".to_string()],
            depends_on: vec![],
            condition: None,
            rollback: vec![],
            continue_on_failure: false,
            timeout_secs: 120,
            status: PhaseStatus::Pending,
            result: None,
            error: None,
        })
        .with_phase(WorkstackPhase {
            id: "implement".to_string(),
            name: "Implement Feature".to_string(),
            description: "Generate and write code".to_string(),
            tools: vec![],
            agents: vec!["rust-pro".to_string(), "python-pro".to_string()],
            depends_on: vec!["analyze".to_string()],
            condition: None,
            rollback: vec![],
            continue_on_failure: false,
            timeout_secs: 300,
            status: PhaseStatus::Pending,
            result: None,
            error: None,
        })
        .with_phase(WorkstackPhase {
            id: "test".to_string(),
            name: "Test Implementation".to_string(),
            description: "Run tests on the implementation".to_string(),
            tools: vec![PhaseToolCall {
                tool: "exec_command".to_string(),
                arguments: serde_json::json!({ "command": "cargo test" }),
                store_as: Some("test_results".to_string()),
                retries: 2,
            }],
            agents: vec!["tdd".to_string()],
            depends_on: vec!["implement".to_string()],
            condition: None,
            rollback: vec![],
            continue_on_failure: true,
            timeout_secs: 180,
            status: PhaseStatus::Pending,
            result: None,
            error: None,
        }),

        // Code Review Workstack
        Workstack::new(
            "code_review",
            "Code Review",
            "Comprehensive code review with multiple perspectives",
        )
        .with_phase(WorkstackPhase {
            id: "security_review".to_string(),
            name: "Security Review".to_string(),
            description: "Review code for security issues".to_string(),
            tools: vec![],
            agents: vec!["security-pro".to_string()],
            depends_on: vec![],
            condition: None,
            rollback: vec![],
            continue_on_failure: true,
            timeout_secs: 120,
            status: PhaseStatus::Pending,
            result: None,
            error: None,
        })
        .with_phase(WorkstackPhase {
            id: "performance_review".to_string(),
            name: "Performance Review".to_string(),
            description: "Review code for performance issues".to_string(),
            tools: vec![],
            agents: vec!["rust-pro".to_string()],
            depends_on: vec![],
            condition: None,
            rollback: vec![],
            continue_on_failure: true,
            timeout_secs: 120,
            status: PhaseStatus::Pending,
            result: None,
            error: None,
        })
        .with_phase(WorkstackPhase {
            id: "consolidate".to_string(),
            name: "Consolidate Reviews".to_string(),
            description: "Combine all review feedback".to_string(),
            tools: vec![],
            agents: vec!["code-review".to_string()],
            depends_on: vec!["security_review".to_string(), "performance_review".to_string()],
            condition: None,
            rollback: vec![],
            continue_on_failure: false,
            timeout_secs: 60,
            status: PhaseStatus::Pending,
            result: None,
            error: None,
        }),
    ]
}
