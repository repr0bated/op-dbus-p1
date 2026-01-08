# Skills System Implementation

## Overview

Skills provide **domain-specific knowledge augmentation** for tool/agent execution:
- System prompt additions
- Input/output transformations
- Constraint enforcement
- Domain-specific variables

## File Structure

```
crates/op-cache/src/
├── skill.rs          # NEW: Core skill types and registry
└── lib.rs            # UPDATE: export skill module

crates/op-chat/src/orchestration/
├── skills.rs         # EXISTS: May need updates
└── mod.rs            # UPDATE: integrate with orchestrator

proto/
└── op_cache.proto    # UPDATE: add Skill service
```

## Step 1: Create src/skill.rs

### Core Types

```rust
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintType {
    RequireArgument,
    ForbidArgument,
    RequireBefore,
    RequireAfter,
    MaxExecutions,
    RequireConfirmation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillConstraint {
    pub constraint_type: ConstraintType,
    pub target: String,  // tool name or "*"
    pub value: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillContext {
    pub system_prompt_additions: Vec<String>,
    pub input_transformations: HashMap<String, Value>,
    pub output_transformations: HashMap<String, Value>,
    pub variables: HashMap<String, Value>,
    pub constraints: Vec<SkillConstraint>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
    pub required_tools: Vec<String>,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub metadata: SkillMetadata,
    pub context: SkillContext,
    pub active: bool,
    pub priority: i32,
}
```

### Skill Implementation

```rust
impl Skill {
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

    pub fn with_prompt(mut self, prompt: &str) -> Self {
        self.context.system_prompt_additions.push(prompt.to_string());
        self
    }

    pub fn with_required_tool(mut self, tool: &str) -> Self {
        self.metadata.required_tools.push(tool.to_string());
        self
    }

    pub fn with_constraint(mut self, constraint: SkillConstraint) -> Self {
        self.context.constraints.push(constraint);
        self
    }

    pub fn with_variable(mut self, name: &str, value: Value) -> Self {
        self.context.variables.insert(name.to_string(), value);
        self
    }

    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_input_transform(mut self, tool: &str, transform: Value) -> Self {
        self.context.input_transformations.insert(tool.to_string(), transform);
        self
    }

    pub fn activate(&mut self) {
        self.active = true;
        info!(skill = %self.name, "Skill activated");
    }

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
            // Apply transformation - extend as needed
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
                    if let Some(key) = constraint.value.as_str() {
                        if args.get(key).is_none() {
                            bail!("Skill '{}' requires argument '{}' for '{}'",
                                self.name, key, tool_name);
                        }
                    }
                }
                ConstraintType::ForbidArgument => {
                    if let Some(key) = constraint.value.as_str() {
                        if args.get(key).is_some() {
                            bail!("Skill '{}' forbids argument '{}' for '{}'",
                                self.name, key, tool_name);
                        }
                    }
                }
                ConstraintType::RequireConfirmation => {
                    warn!(skill = %self.name, tool = %tool_name, "Requires confirmation");
                }
                _ => {}
            }
        }
        Ok(())
    }
}
```

### Skill Registry

```rust
pub struct SkillRegistry {
    skills: RwLock<HashMap<String, Skill>>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self { skills: RwLock::new(HashMap::new()) }
    }

    pub fn with_defaults() -> Self {
        let registry = Self::new();
        let mut skills = HashMap::new();
        for skill in default_skills() {
            skills.insert(skill.name.clone(), skill);
        }
        Self { skills: RwLock::new(skills) }
    }

    pub async fn register(&self, skill: Skill) {
        info!(skill = %skill.name, "Registering skill");
        self.skills.write().await.insert(skill.name.clone(), skill);
    }

    pub async fn get(&self, name: &str) -> Option<Skill> {
        self.skills.read().await.get(name).cloned()
    }

    pub async fn list_all(&self) -> Vec<Skill> {
        self.skills.read().await.values().cloned().collect()
    }

    pub async fn active_skills(&self) -> Vec<Skill> {
        let mut active: Vec<Skill> = self.skills.read().await
            .values()
            .filter(|s| s.active)
            .cloned()
            .collect();
        active.sort_by(|a, b| b.priority.cmp(&a.priority));
        active
    }

    pub async fn activate(&self, name: &str) -> Result<()> {
        let mut skills = self.skills.write().await;
        if let Some(skill) = skills.get_mut(name) {
            skill.activate();
            Ok(())
        } else {
            bail!("Skill not found: {}", name)
        }
    }

    pub async fn deactivate(&self, name: &str) -> Result<()> {
        let mut skills = self.skills.write().await;
        if let Some(skill) = skills.get_mut(name) {
            skill.deactivate();
            Ok(())
        } else {
            bail!("Skill not found: {}", name)
        }
    }

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

    pub async fn check_constraints(&self, tool_name: &str, args: &Value) -> Result<()> {
        for skill in self.active_skills().await {
            skill.check_constraints(tool_name, args)?;
        }
        Ok(())
    }

    pub async fn transform_input(&self, tool_name: &str, mut args: Value) -> Value {
        for skill in self.active_skills().await {
            args = skill.transform_input(tool_name, args);
        }
        args
    }

    pub async fn transform_output(&self, tool_name: &str, mut output: Value) -> Value {
        for skill in self.active_skills().await {
            output = skill.transform_output(tool_name, output);
        }
        output
    }
}

impl Default for SkillRegistry {
    fn default() -> Self { Self::with_defaults() }
}
```

### Default Skills

```rust
fn default_skills() -> Vec<Skill> {
    vec![
        Skill::new("python_debugging", "Enhanced Python debugging", "debugging")
            .with_prompt("Use pdb breakpoints and inspect variables systematically.")
            .with_required_tool("agent_python_pro")
            .with_variable("debug_level", json!("verbose"))
            .with_priority(10),

        Skill::new("rust_optimization", "Rust performance optimization", "optimization")
            .with_prompt("Focus on zero-cost abstractions, avoid allocations.")
            .with_required_tool("agent_rust_pro")
            .with_priority(10),

        Skill::new("security_audit", "Security-focused code review", "security")
            .with_prompt("Check for: SQL injection, XSS, CSRF, path traversal, secrets.")
            .with_constraint(SkillConstraint {
                constraint_type: ConstraintType::RequireConfirmation,
                target: "*".to_string(),
                value: json!(true),
            })
            .with_priority(20),

        Skill::new("tdd_workflow", "Test-Driven Development", "methodology")
            .with_prompt("Red-Green-Refactor: Write failing test, minimal code, refactor.")
            .with_priority(15),

        Skill::new("ovs_networking", "Open vSwitch expertise", "networking")
            .with_prompt("Use OVSDB JSON-RPC. Never use ovs-vsctl CLI.")
            .with_required_tool("ovs_create_bridge")
            .with_priority(10),
    ]
}
```

## Step 2: Update lib.rs

```rust
pub mod skill;
pub use skill::{Skill, SkillConstraint, SkillContext, SkillRegistry, ConstraintType};
```

## Step 3: Add to Proto (if using gRPC)

See the full proto definition in the main IMPLEMENTATION.md for SkillService.

## Usage

```rust
let registry = SkillRegistry::with_defaults();

// Activate skill
registry.activate("security_audit").await?;

// Check constraints before tool call
registry.check_constraints("file_write", &args).await?;

// Transform input
let transformed = registry.transform_input("agent_python_pro", args).await;

// Execute tool...

// Transform output
let final_output = registry.transform_output("agent_python_pro", output).await;
```
