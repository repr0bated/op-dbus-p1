//! System Introspection Tool
//! Automatically discovers and introspects all D-Bus services
//! Generates MCP tool schemas for each discovered service

use crate::introspection_cache::IntrospectionCache;
use crate::introspection_parser::{InterfaceInfo, IntrospectionParser};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use zbus::{Connection, Proxy};

/// Discovered D-Bus service with full introspection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredService {
    pub service_name: String,
    pub object_paths: Vec<String>,
    pub interfaces: HashMap<String, Vec<InterfaceInfo>>, // path -> interfaces
}

/// System introspection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemIntrospection {
    pub services: Vec<DiscoveredService>,
    pub timestamp: i64,
    pub total_services: usize,
    pub total_interfaces: usize,
    pub total_methods: usize,
}

pub struct SystemIntrospector {
    connection: Connection,
    cache: Option<Arc<Mutex<IntrospectionCache>>>,
}

impl SystemIntrospector {
    /// Create new system introspector with caching
    pub async fn new() -> Result<Self> {
        let connection = Connection::system()
            .await
            .context("Failed to connect to system D-Bus")?;

        // Initialize cache at /var/cache/dbus-introspection.db
        let cache_path = PathBuf::from("/var/cache/dbus-introspection.db");
        let cache = match IntrospectionCache::new(&cache_path) {
            Ok(c) => {
                log::info!("D-Bus introspection cache enabled at {:?}", cache_path);
                Some(Arc::new(Mutex::new(c)))
            }
            Err(e) => {
                log::warn!(
                    "Failed to initialize introspection cache, running without cache: {}",
                    e
                );
                None
            }
        };

        Ok(Self { connection, cache })
    }

    /// Create new system introspector without caching (for testing)
    pub async fn new_without_cache() -> Result<Self> {
        let connection = Connection::system()
            .await
            .context("Failed to connect to system D-Bus")?;

        Ok(Self {
            connection,
            cache: None,
        })
    }

    /// List all D-Bus service names on the system bus
    pub async fn list_all_services(&self) -> Result<Vec<String>> {
        let proxy = zbus::fdo::DBusProxy::new(&self.connection)
            .await
            .context("Failed to create D-Bus proxy")?;

        let names = proxy
            .list_names()
            .await
            .context("Failed to list D-Bus names")?;

        // Filter to real services (skip :1.xxx temporary names)
        let services: Vec<String> = names
            .into_iter()
            .filter(|name| !name.starts_with(':'))
            .filter(|name| name.contains('.')) // Must be well-formed
            .map(|name| name.to_string())
            .collect();

        Ok(services)
    }

    /// Get activatable services (not currently running)
    pub async fn list_activatable_services(&self) -> Result<Vec<String>> {
        let proxy = zbus::fdo::DBusProxy::new(&self.connection).await?;

        let names = proxy.list_activatable_names().await?;

        Ok(names.into_iter().map(|name| name.to_string()).collect())
    }

    /// Introspect a specific service at a given path (with caching)
    pub async fn introspect_service_at_path(
        &self,
        service_name: &str,
        object_path: &str,
    ) -> Result<String> {
        // Try cache first
        if let Some(ref cache) = self.cache {
            if let Ok(cache_guard) = cache.lock() {
                if let Ok(Some(_cached_json)) =
                    cache_guard.get_introspection_json(service_name, object_path, None)
                {
                    log::debug!("Cache hit for {} at {}", service_name, object_path);
                    // Convert JSON back to XML for compatibility
                    // (Ideally we'd return JSON directly, but existing code expects XML)
                    // For now, we'll do a D-Bus call anyway if cache exists but we need XML format
                    // TODO: Refactor to use JSON throughout
                }
            }
        }

        // Cache miss or no cache - do D-Bus introspection
        log::debug!(
            "Cache miss for {} at {}, performing D-Bus introspection",
            service_name,
            object_path
        );

        let proxy = Proxy::new(
            &self.connection,
            service_name,
            object_path,
            "org.freedesktop.DBus.Introspectable",
        )
        .await
        .context(format!(
            "Failed to create proxy for {} at {}",
            service_name, object_path
        ))?;

        let xml: String = proxy.call("Introspect", &()).await.context(format!(
            "Failed to introspect {} at {}",
            service_name, object_path
        ))?;

        // Store in cache for next time
        if let Some(ref cache) = self.cache {
            if let Ok(cache_guard) = cache.lock() {
                if let Err(e) = cache_guard.store_introspection(service_name, object_path, &xml) {
                    log::warn!(
                        "Failed to cache introspection for {} at {}: {}",
                        service_name,
                        object_path,
                        e
                    );
                } else {
                    log::debug!(
                        "Cached introspection for {} at {}",
                        service_name,
                        object_path
                    );
                }
            }
        }

        Ok(xml)
    }

    /// Discover object paths for a service by introspecting recursively
    pub fn discover_object_paths<'a>(
        &'a self,
        service_name: &'a str,
        start_path: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<String>>> + Send + 'a>> {
        Box::pin(async move {
            let mut discovered = vec![start_path.to_string()];

            // Try to introspect the starting path
            match self
                .introspect_service_at_path(service_name, start_path)
                .await
            {
                Ok(xml) => {
                    // Extract child nodes from XML
                    let children = self.extract_child_nodes(&xml);

                    for child in children {
                        let child_path = if start_path == "/" {
                            format!("/{}", child)
                        } else {
                            format!("{}/{}", start_path, child)
                        };

                        discovered.push(child_path.clone());

                        // Recursively discover children (limited depth)
                        if discovered.len() < 100 {
                            // Safety limit
                            match self.discover_object_paths(service_name, &child_path).await {
                                Ok(sub_paths) => {
                                    for sub_path in sub_paths {
                                        if !discovered.contains(&sub_path) {
                                            discovered.push(sub_path);
                                        }
                                    }
                                }
                                Err(_) => {
                                    // Continue on error
                                }
                            }
                        }
                    }
                }
                Err(_) => {
                    // If introspection fails, return just the start path
                }
            }

            Ok(discovered)
        })
    }

    /// Extract child node names from introspection XML
    fn extract_child_nodes(&self, xml: &str) -> Vec<String> {
        let mut children = Vec::new();

        for line in xml.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("<node name=\"") {
                if let Some(name) = self.extract_xml_attr(trimmed, "name") {
                    if !name.is_empty() && !name.starts_with('/') {
                        children.push(name);
                    }
                }
            }
        }

        children
    }

    /// Extract XML attribute value
    fn extract_xml_attr(&self, line: &str, attr: &str) -> Option<String> {
        let pattern = format!("{}=\"", attr);
        if let Some(start) = line.find(&pattern) {
            let start = start + pattern.len();
            if let Some(end) = line[start..].find('"') {
                return Some(line[start..start + end].to_string());
            }
        }
        None
    }

    /// Fully introspect a service (all paths)
    pub async fn introspect_service(&self, service_name: &str) -> Result<DiscoveredService> {
        log::info!("Introspecting service: {}", service_name);

        // Derive default path from service name
        let default_path = if service_name.starts_with("org.") || service_name.starts_with("com.") {
            format!("/{}", service_name.replace('.', "/"))
        } else {
            "/".to_string()
        };

        // Discover all object paths
        let mut object_paths = self
            .discover_object_paths(service_name, &default_path)
            .await?;

        // Also try root path if not already included
        if !object_paths.contains(&"/".to_string()) {
            if let Ok(root_paths) = self.discover_object_paths(service_name, "/").await {
                for path in root_paths {
                    if !object_paths.contains(&path) {
                        object_paths.push(path);
                    }
                }
            }
        }

        // Introspect each path and parse interfaces
        let mut interfaces: HashMap<String, Vec<InterfaceInfo>> = HashMap::new();

        for path in &object_paths {
            match self.introspect_service_at_path(service_name, path).await {
                Ok(xml) => {
                    let introspection = IntrospectionParser::parse_xml(&xml);
                    if !introspection.interfaces.is_empty() {
                        interfaces.insert(path.clone(), introspection.interfaces);
                    }
                }
                Err(e) => {
                    log::debug!("Failed to introspect {} at {}: {}", service_name, path, e);
                }
            }
        }

        Ok(DiscoveredService {
            service_name: service_name.to_string(),
            object_paths,
            interfaces,
        })
    }

    /// Introspect all services on the system
    pub async fn introspect_all_services(&self) -> Result<SystemIntrospection> {
        let service_names = self.list_all_services().await?;

        log::info!("Found {} D-Bus services", service_names.len());

        let mut services = Vec::new();
        let mut total_interfaces = 0;
        let mut total_methods = 0;

        // Well-known important services to prioritize
        let priority_services = vec![
            "org.freedesktop.systemd1",
            "org.freedesktop.PackageKit",
            "org.freedesktop.NetworkManager",
            "org.freedesktop.login1",
            "org.freedesktop.UDisks2",
            "org.freedesktop.UPower",
        ];

        // Introspect priority services first
        for service_name in &priority_services {
            if service_names.contains(&service_name.to_string()) {
                match self.introspect_service(service_name).await {
                    Ok(service) => {
                        // Count interfaces and methods
                        for ifaces in service.interfaces.values() {
                            total_interfaces += ifaces.len();
                            for iface in ifaces {
                                total_methods += iface.methods.len();
                            }
                        }

                        services.push(service);
                    }
                    Err(e) => {
                        log::warn!("Failed to introspect {}: {}", service_name, e);
                    }
                }
            }
        }

        // Introspect remaining services (limit to avoid overload)
        let remaining: Vec<String> = service_names
            .into_iter()
            .filter(|name| !priority_services.contains(&name.as_str()))
            .take(50) // Limit to 50 additional services
            .collect();

        for service_name in remaining {
            match self.introspect_service(&service_name).await {
                Ok(service) => {
                    // Count interfaces and methods
                    for ifaces in service.interfaces.values() {
                        total_interfaces += ifaces.len();
                        for iface in ifaces {
                            total_methods += iface.methods.len();
                        }
                    }

                    services.push(service);
                }
                Err(e) => {
                    log::debug!("Failed to introspect {}: {}", service_name, e);
                }
            }
        }

        Ok(SystemIntrospection {
            total_services: services.len(),
            total_interfaces,
            total_methods,
            services,
            timestamp: chrono::Utc::now().timestamp(),
        })
    }

    /// Generate MCP tool schema for a service
    pub fn generate_mcp_tools(&self, service: &DiscoveredService) -> Vec<serde_json::Value> {
        use serde_json::json;

        let mut tools = Vec::new();

        for (path, interfaces) in &service.interfaces {
            for interface in interfaces {
                for method in &interface.methods {
                    // Generate MCP tool for each method
                    let tool_name = format!(
                        "{}__{}",
                        service.service_name.replace('.', "_"),
                        method.name.to_lowercase()
                    );

                    let mut properties = serde_json::Map::new();
                    let mut required = Vec::new();

                    // Add method inputs as tool parameters
                    for input in &method.inputs {
                        let schema = IntrospectionParser::dbus_type_to_mcp_schema(&input.type_sig);
                        properties.insert(input.name.clone(), schema);
                        required.push(input.name.clone());
                    }

                    let tool = json!({
                        "name": tool_name,
                        "description": format!("{}.{} on {} ({})",
                            interface.name,
                            method.name,
                            service.service_name,
                            path
                        ),
                        "inputSchema": {
                            "type": "object",
                            "properties": properties,
                            "required": required
                        },
                        "metadata": {
                            "service": service.service_name,
                            "interface": interface.name,
                            "path": path,
                            "method": method.name
                        }
                    });

                    tools.push(tool);
                }
            }
        }

        tools
    }

    /// Query cached methods for a service/interface (fast lookup without D-Bus)
    pub fn query_cached_methods(
        &self,
        service_name: &str,
        interface_name: &str,
    ) -> Result<Option<serde_json::Value>> {
        if let Some(ref cache) = self.cache {
            if let Ok(cache_guard) = cache.lock() {
                cache_guard
                    .get_methods_json(service_name, interface_name)
                    .map(Some)
                    .or_else(|e| {
                        log::debug!(
                            "Cache query failed for {}.{}: {}",
                            service_name,
                            interface_name,
                            e
                        );
                        Ok(None)
                    })
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Search cached methods by name pattern (fast search without D-Bus)
    pub fn search_cached_methods(&self, pattern: &str) -> Result<Vec<serde_json::Value>> {
        if let Some(ref cache) = self.cache {
            if let Ok(cache_guard) = cache.lock() {
                cache_guard.search_methods(pattern)
            } else {
                Ok(Vec::new())
            }
        } else {
            Ok(Vec::new())
        }
    }

    /// Get cache statistics
    pub fn get_cache_stats(&self) -> Option<serde_json::Value> {
        if let Some(ref cache) = self.cache {
            if let Ok(cache_guard) = cache.lock() {
                cache_guard.get_stats().ok()
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Clear old cache entries (older than specified days)
    pub fn clear_old_cache(&self, days: u64) -> Result<usize> {
        if let Some(ref cache) = self.cache {
            if let Ok(cache_guard) = cache.lock() {
                cache_guard.clear_old_cache(days)
            } else {
                Ok(0)
            }
        } else {
            Ok(0)
        }
    }
}

/// CLI tool for system introspection
#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        println!("Usage:");
        println!(
            "  {} list                   - List all D-Bus services",
            args[0]
        );
        println!(
            "  {} introspect <service>   - Introspect specific service",
            args[0]
        );
        println!(
            "  {} introspect-all         - Introspect all services",
            args[0]
        );
        println!(
            "  {} mcp-tools <service>    - Generate MCP tools for service",
            args[0]
        );
        println!(
            "  {} mcp-tools-all          - Generate MCP tools for all services",
            args[0]
        );
        return Ok(());
    }

    let introspector = SystemIntrospector::new().await?;

    match args[1].as_str() {
        "list" => {
            let services = introspector.list_all_services().await?;
            println!("Found {} D-Bus services:\n", services.len());
            for service in services {
                println!("  {}", service);
            }
        }
        "introspect" => {
            if args.len() < 3 {
                eprintln!("Error: Service name required");
                return Ok(());
            }

            let service_name = &args[2];
            let service = introspector.introspect_service(service_name).await?;

            println!("Service: {}", service.service_name);
            println!("Object paths: {}", service.object_paths.len());
            println!("\nInterfaces:");

            for (path, interfaces) in &service.interfaces {
                println!("\n  Path: {}", path);
                for interface in interfaces {
                    println!("    Interface: {}", interface.name);
                    println!("      Methods: {}", interface.methods.len());
                    for method in &interface.methods {
                        println!("        - {}", method.name);
                    }
                    println!("      Properties: {}", interface.properties.len());
                    println!("      Signals: {}", interface.signals.len());
                }
            }

            // Output JSON
            let json = serde_json::to_string_pretty(&service)?;
            println!("\nJSON:\n{}", json);
        }
        "introspect-all" => {
            let result = introspector.introspect_all_services().await?;

            println!("System Introspection Results:");
            println!("  Total services: {}", result.total_services);
            println!("  Total interfaces: {}", result.total_interfaces);
            println!("  Total methods: {}", result.total_methods);

            let json = serde_json::to_string_pretty(&result)?;
            std::fs::write("/tmp/system-introspection.json", &json)?;
            println!("\nFull results written to: /tmp/system-introspection.json");
        }
        "mcp-tools" => {
            if args.len() < 3 {
                eprintln!("Error: Service name required");
                return Ok(());
            }

            let service_name = &args[2];
            let service = introspector.introspect_service(service_name).await?;
            let tools = introspector.generate_mcp_tools(&service);

            println!("Generated {} MCP tools for {}\n", tools.len(), service_name);

            let json = serde_json::to_string_pretty(&tools)?;
            println!("{}", json);
        }
        "mcp-tools-all" => {
            let result = introspector.introspect_all_services().await?;

            let mut all_tools = Vec::new();
            for service in &result.services {
                let tools = introspector.generate_mcp_tools(service);
                all_tools.extend(tools);
            }

            println!("Generated {} total MCP tools\n", all_tools.len());

            let json = serde_json::to_string_pretty(&all_tools)?;
            std::fs::write("/tmp/mcp-tools-all.json", &json)?;
            println!("MCP tools written to: /tmp/mcp-tools-all.json");
        }
        _ => {
            eprintln!("Unknown command: {}", args[1]);
        }
    }

    Ok(())
}
