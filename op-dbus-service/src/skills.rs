//! Skill tools registration for MCP.

use anyhow::{bail, Result};
use async_trait::async_trait;
use op_chat::{Skill, SkillRegistry};
use op_tools::registry::ToolDefinition;
use op_tools::{Tool, ToolRegistry};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

pub async fn register_skill_tools(registry: &ToolRegistry) -> Result<()> {
    let skill_registry = Arc::new(RwLock::new(SkillRegistry::with_defaults()));

    let list_tool = Arc::new(SkillListTool {
        registry: Arc::clone(&skill_registry),
    });
    register_tool_with_definition(registry, list_tool).await?;

    let describe_tool = Arc::new(SkillDescribeTool {
        registry: Arc::clone(&skill_registry),
    });
    register_tool_with_definition(registry, describe_tool).await?;

    Ok(())
}

async fn register_tool_with_definition(
    registry: &ToolRegistry,
    tool: Arc<dyn Tool>,
) -> Result<()> {
    let definition = ToolDefinition {
        name: tool.name().to_string(),
        description: tool.description().to_string(),
        input_schema: tool.input_schema(),
        category: tool.category().to_string(),
        tags: tool.tags(),
        namespace: tool.namespace().to_string(),
    };

    registry
        .register(Arc::from(tool.name()), tool, definition)
        .await
        .map_err(Into::into)
}

struct SkillListTool {
    registry: Arc<RwLock<SkillRegistry>>,
}

#[async_trait]
impl Tool for SkillListTool {
    fn name(&self) -> &str {
        "skill_list"
    }

    fn description(&self) -> &str {
        "List registered skills with optional filters"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "category": {
                    "type": "string",
                    "description": "Filter by skill category"
                },
                "tag": {
                    "type": "string",
                    "description": "Filter by skill tag"
                }
            },
            "additionalProperties": false
        })
    }

    fn category(&self) -> &str {
        "skills"
    }

    fn tags(&self) -> Vec<String> {
        vec!["skills".to_string(), "metadata".to_string()]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let category = input
            .get("category")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_lowercase())
            .filter(|value| !value.is_empty());
        let tag = input
            .get("tag")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_lowercase())
            .filter(|value| !value.is_empty());

        let registry = self.registry.read().await;
        let mut skills = registry.list();

        if let Some(category_filter) = category {
            skills.retain(|skill| skill.metadata.category.to_lowercase() == category_filter);
        }

        if let Some(tag_filter) = tag {
            skills.retain(|skill| {
                skill
                    .metadata
                    .tags
                    .iter()
                    .any(|tag| tag.to_lowercase() == tag_filter)
            });
        }

        let payload: Vec<Value> = skills.into_iter().map(skill_metadata_value).collect();
        Ok(json!({ "skills": payload }))
    }
}

struct SkillDescribeTool {
    registry: Arc<RwLock<SkillRegistry>>,
}

#[async_trait]
impl Tool for SkillDescribeTool {
    fn name(&self) -> &str {
        "skill_describe"
    }

    fn description(&self) -> &str {
        "Describe a skill by name"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Skill identifier"
                },
                "level": {
                    "type": "string",
                    "enum": ["metadata", "instructions", "full"],
                    "default": "metadata"
                }
            },
            "required": ["name"],
            "additionalProperties": false
        })
    }

    fn category(&self) -> &str {
        "skills"
    }

    fn tags(&self) -> Vec<String> {
        vec![
            "skills".to_string(),
            "disclosure".to_string(),
            "detail".to_string(),
        ]
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let name = input
            .get("name")
            .and_then(|value| value.as_str())
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Missing skill 'name'"))?;
        let level = input
            .get("level")
            .and_then(|value| value.as_str())
            .unwrap_or("metadata")
            .to_lowercase();

        let registry = self.registry.read().await;
        let skill = registry
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Skill '{}' not found", name))?;

        let payload = match level.as_str() {
            "metadata" => json!({
                "name": skill.name,
                "metadata": skill.metadata,
                "level": "metadata"
            }),
            "instructions" | "full" => json!({
                "skill": skill,
                "level": level
            }),
            _ => bail!("level must be one of: metadata, instructions, full"),
        };

        Ok(payload)
    }
}

fn skill_metadata_value(skill: &Skill) -> Value {
    json!({
        "name": skill.name,
        "description": skill.metadata.description,
        "category": skill.metadata.category,
        "tags": skill.metadata.tags,
        "required_tools": skill.metadata.required_tools,
        "version": skill.metadata.version
    })
}
