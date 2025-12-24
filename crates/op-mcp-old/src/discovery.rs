// D-Bus Discovery Service
// Scans D-Bus, introspects services, generates MCP configs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zbus::Connection;

#[derive(Debug, Serialize, Deserialize)]
pub struct DbusService {
    pub name: String,
    pub interface: String,
    pub methods: Vec<DbusMethod>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DbusMethod {
    pub name: String,
    pub args: Vec<DbusArg>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DbusArg {
    pub name: String,
    pub arg_type: String,
    pub direction: String,
}

#[derive(Debug, Serialize)]
pub struct McpServerConfig {
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

pub struct DbusDiscovery {
    connection: Connection,
}

impl DbusDiscovery {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let connection = Connection::session().await?;
        Ok(Self { connection })
    }

    pub async fn scan_services(&self) -> Result<Vec<DbusService>, Box<dyn std::error::Error>> {
        let mut services = Vec::new();

        // Well-known services to expose
        let targets = vec![
            "org.freedesktop.systemd1",
            "org.freedesktop.NetworkManager",
            "org.freedesktop.login1",
        ];

        for service_name in targets {
            if let Ok(introspection) = self.introspect_service(service_name).await {
                let service = self.parse_introspection(service_name, &introspection);
                services.push(service);
            }
        }

        // Also scan for dbusmcp agents
        let proxy = zbus::fdo::DBusProxy::new(&self.connection).await?;
        let names = proxy.list_names().await?;

        for name in names {
            if name.starts_with("org.dbusmcp.Agent") {
                if let Ok(introspection) = self.introspect_service(&name).await {
                    let service = self.parse_introspection(&name, &introspection);
                    services.push(service);
                }
            }
        }

        Ok(services)
    }

    async fn introspect_service(
        &self,
        service_name: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // Determine the correct D-Bus path for introspection
        // Pattern: org.freedesktop.systemd1 -> /org/freedesktop/systemd1
        let path = if service_name.starts_with("org.") || service_name.starts_with("com.") {
            format!("/{}", service_name.replace('.', "/"))
        } else {
            "/".to_string()
        };

        eprintln!("  Introspecting {} at {}", service_name, path);

        let proxy = zbus::Proxy::new(
            &self.connection,
            service_name,
            path.as_str(),
            "org.freedesktop.DBus.Introspectable",
        )
        .await?;

        let xml: String = proxy.call("Introspect", &()).await?;
        Ok(xml)
    }

    fn parse_introspection(&self, service_name: &str, xml: &str) -> DbusService {
        // Simple XML parsing (would use proper parser in production)
        let mut methods = Vec::new();

        // Extract interface name
        let interface = self.extract_interface(xml, service_name);

        // Extract methods (simplified)
        for line in xml.lines() {
            if line.trim().starts_with("<method name=") {
                if let Some(method_name) = self.extract_attribute(line, "name") {
                    methods.push(DbusMethod {
                        name: method_name,
                        args: Vec::new(), // Would parse args in full implementation
                    });
                }
            }
        }

        DbusService {
            name: service_name.to_string(),
            interface: interface.to_string(),
            methods,
        }
    }

    fn extract_interface(&self, xml: &str, service_name: &str) -> String {
        for line in xml.lines() {
            if line.trim().starts_with("<interface name=") {
                if let Some(name) = self.extract_attribute(line, "name") {
                    // Skip standard D-Bus interfaces
                    if !name.starts_with("org.freedesktop.DBus") {
                        return name;
                    }
                }
            }
        }
        service_name.to_string()
    }

    fn extract_attribute(&self, line: &str, attr: &str) -> Option<String> {
        let pattern = format!("{}=\"", attr);
        if let Some(start) = line.find(&pattern) {
            let start = start + pattern.len();
            if let Some(end) = line[start..].find('"') {
                return Some(line[start..start + end].to_string());
            }
        }
        None
    }

    pub fn generate_mcp_config(&self, service: &DbusService) -> McpServerConfig {
        let friendly_name = self.service_to_friendly_name(&service.name);

        McpServerConfig {
            command: "./target/debug/dbus-mcp-bridge".to_string(),
            args: vec!["--service".to_string(), service.name.clone()],
            env: HashMap::from([
                ("DBUS_SERVICE".to_string(), service.name.clone()),
                ("MCP_NAME".to_string(), friendly_name),
            ]),
        }
    }

    fn service_to_friendly_name(&self, service: &str) -> String {
        service
            .replace("org.freedesktop.", "")
            .replace("org.dbusmcp.Agent.", "")
            .replace('.', "-")
            .to_lowercase()
    }

    pub fn write_config(
        &self,
        config: &McpServerConfig,
        output_dir: &str,
        name: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let filename = format!("{}/{}.json", output_dir, name);
        let json = serde_json::to_string_pretty(&config)?;
        std::fs::write(&filename, json)?;
        println!("✓ Generated: {}", filename);
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== D-Bus MCP Discovery Service ===\n");

    let discovery = DbusDiscovery::new().await?;

    println!("Scanning D-Bus for services...\n");
    let services = discovery.scan_services().await?;

    println!("Found {} services:\n", services.len());

    for service in &services {
        println!("  {} ({} methods)", service.name, service.methods.len());
        for method in &service.methods {
            println!("    - {}", method.name);
        }
        println!();
    }

    println!("\nGenerating MCP configurations...\n");

    let output_dir = "/tmp/mcp-servers";
    std::fs::create_dir_all(output_dir)?;

    for service in &services {
        let config = discovery.generate_mcp_config(service);
        let name = discovery.service_to_friendly_name(&service.name);
        discovery.write_config(&config, output_dir, &name)?;
    }

    println!("\n✅ Done! Configs written to: {}", output_dir);
    println!("\nTo use with Claude Code:");
    println!("  Copy {} to ~/.config/Claude/mcp-servers/", output_dir);

    Ok(())
}
