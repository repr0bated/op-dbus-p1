//! D-Bus Agent Launcher
//!
//! Universal binary to run any agent type as a D-Bus service.
//!
//! # Usage
//!
//! ```bash
//! # Run an agent on the session bus
//! dbus-agent python-pro
//!
//! # Run with a custom agent ID
//! dbus-agent python-pro my-python-agent
//!
//! # Run on the system bus (requires privileges)
//! dbus-agent --system rust-pro
//!
//! # List available agent types
//! dbus-agent --list
//! ```
//!
//! # D-Bus Registration
//!
//! The agent will be registered as:
//! - Service name: `org.dbusmcp.Agent.{AgentType}` (e.g., `org.dbusmcp.Agent.PythonPro`)
//! - Object path: `/org/dbusmcp/Agent/{AgentType}`
//! - Interface: `org.dbusmcp.Agent`
//!
//! # Discovery
//!
//! Once running, the agent can be discovered by the ChatActor's tool_loader
//! and registered as a tool that the LLM can call.

use std::env;

use op_core::BusType;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use op_agents::agents::{
    architecture::{BackendArchitectAgent, FrontendDeveloperAgent, GraphQLArchitectAgent},
    infrastructure::{
        CloudArchitectAgent, DeploymentAgent, KubernetesAgent, NetworkEngineerAgent,
        TerraformAgent,
    },
    language::{
        BashProAgent, CProAgent, CppProAgent, CSharpProAgent, ElixirProAgent, GolangProAgent,
        JavaProAgent, JavaScriptProAgent, JuliaProAgent, PhpProAgent, PythonProAgent, RubyProAgent,
        RustProAgent, ScalaProAgent, TypeScriptProAgent,
    },
    AgentTrait,
};
use op_agents::agent_catalog::builtin_agent_descriptors;
use op_agents::dbus_service::{generate_agent_id, start_agent};

fn print_usage() {
    eprintln!(
        "Usage:\n  dbus-agent [--system] <agent-type> [agent-id]\n  dbus-agent --list\n\nExamples:\n  dbus-agent python-pro\n  dbus-agent --system rust-pro\n  dbus-agent python-pro my-agent-id"
    );
}

fn normalize_agent_type(raw: &str) -> String {
    raw.trim().to_lowercase().replace('_', "-")
}

fn build_agent(agent_type: &str, agent_id: String) -> Option<Box<dyn AgentTrait>> {
    match agent_type {
        // Language
        "bash-pro" => Some(Box::new(BashProAgent::new(agent_id))),
        "c-pro" => Some(Box::new(CProAgent::new(agent_id))),
        "cpp-pro" => Some(Box::new(CppProAgent::new(agent_id))),
        "csharp-pro" => Some(Box::new(CSharpProAgent::new(agent_id))),
        "elixir-pro" => Some(Box::new(ElixirProAgent::new(agent_id))),
        "golang-pro" => Some(Box::new(GolangProAgent::new(agent_id))),
        "java-pro" => Some(Box::new(JavaProAgent::new(agent_id))),
        "javascript-pro" => Some(Box::new(JavaScriptProAgent::new(agent_id))),
        "julia-pro" => Some(Box::new(JuliaProAgent::new(agent_id))),
        "php-pro" => Some(Box::new(PhpProAgent::new(agent_id))),
        "python-pro" => Some(Box::new(PythonProAgent::new(agent_id))),
        "ruby-pro" => Some(Box::new(RubyProAgent::new(agent_id))),
        "rust-pro" => Some(Box::new(RustProAgent::new(agent_id))),
        "scala-pro" => Some(Box::new(ScalaProAgent::new(agent_id))),
        "typescript-pro" => Some(Box::new(TypeScriptProAgent::new(agent_id))),
        // Architecture
        "backend-architect" => Some(Box::new(BackendArchitectAgent::new(agent_id))),
        "frontend-developer" => Some(Box::new(FrontendDeveloperAgent::new(agent_id))),
        "graphql-architect" => Some(Box::new(GraphQLArchitectAgent::new(agent_id))),
        // Infrastructure
        "cloud-architect" => Some(Box::new(CloudArchitectAgent::new(agent_id))),
        "deployment-engineer" => Some(Box::new(DeploymentAgent::new(agent_id))),
        "kubernetes-architect" => Some(Box::new(KubernetesAgent::new(agent_id))),
        "network-engineer" => Some(Box::new(NetworkEngineerAgent::new(agent_id))),
        "terraform-specialist" => Some(Box::new(TerraformAgent::new(agent_id))),
        _ => None,
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("op_agents=info".parse()?))
        .init();

    let mut args = env::args().skip(1);
    let mut use_system = false;

    let mut raw = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--system" => use_system = true,
            "--list" => {
                for descriptor in builtin_agent_descriptors() {
                    println!(
                        "{} - {}",
                        descriptor.agent_type, descriptor.description
                    );
                }
                return Ok(());
            }
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            _ => raw.push(arg),
        }
    }

    if raw.is_empty() {
        print_usage();
        return Ok(());
    }

    let agent_type = normalize_agent_type(&raw[0]);
    let agent_id = raw
        .get(1)
        .cloned()
        .unwrap_or_else(|| generate_agent_id(&agent_type));

    let Some(agent) = build_agent(&agent_type, agent_id.clone()) else {
        error!("Unknown agent type: {}", agent_type);
        warn!("Use --list to see available agents.");
        return Ok(());
    };

    let bus_type = if use_system {
        BusType::System
    } else {
        BusType::Session
    };

    info!(
        "Starting agent '{}' with id '{}' on {:?} bus",
        agent_type, agent_id, bus_type
    );

    let _conn = start_agent(agent, &agent_id, bus_type).await?;
    tokio::signal::ctrl_c().await?;
    Ok(())
}
