//! Granular D-Bus Introspection Tools
//! Provides deep access to D-Bus services, methods, properties, and signals
//! All introspection is converted to JSON immediately via SQLite cache

use crate::introspection_cache::IntrospectionCache;
use crate::tool_registry::{DynamicToolBuilder, ToolContent, ToolResult};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use zbus::{Connection, Proxy};

/// Get introspection cache path
fn get_cache_path() -> PathBuf {
    std::env::var("OPDBUS_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/var/lib/op-dbus/cache"))
        .join("introspection.db")
}

/// Register granular D-Bus introspection tools
pub async fn register_dbus_granular_tools(
    registry: &crate::tool_registry::ToolRegistry,
) -> Result<()> {
    // List all D-Bus services
    register_list_services_tool(registry).await?;
    
    // Introspect a specific service
    register_introspect_service_tool(registry).await?;
    
    // Get service object paths
    register_list_objects_tool(registry).await?;
    
    // Introspect a specific object
    register_introspect_object_tool(registry).await?;
    
    // Get object interfaces
    register_list_interfaces_tool(registry).await?;
    
    // Get interface methods
    register_list_methods_tool(registry).await?;
    
    // Get interface properties
    register_list_properties_tool(registry).await?;
    
    // Get interface signals
    register_list_signals_tool(registry).await?;
    
    // Call a D-Bus method
    register_call_method_tool(registry).await?;
    
    // Get a property value
    register_get_property_tool(registry).await?;
    
    // Set a property value
    register_set_property_tool(registry).await?;
    
    // Get all properties of an object
    register_get_all_properties_tool(registry).await?;
    
    Ok(())
}

/// Tool: List all D-Bus services
async fn register_list_services_tool(
    registry: &crate::tool_registry::ToolRegistry,
) -> Result<()> {
    let tool = DynamicToolBuilder::new("dbus_list_services")
        .description("List all available D-Bus services on system or session bus")
        .schema(json!({
            "type": "object",
            "properties": {
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system",
                    "description": "D-Bus bus type"
                },
                "filter": {
                    "type": "string",
                    "description": "Optional filter pattern (e.g., 'org.freedesktop')"
                }
            }
        }))
        .handler(|params| {
            Box::pin(async move {
                let bus_type = params["bus"]
                    .as_str()
                    .unwrap_or("system");
                
                let filter = params["filter"]
                    .as_str()
                    .map(|s| s.to_string());
                
                let connection = if bus_type == "system" {
                    Connection::system().await?
                } else {
                    Connection::session().await?
                };
                
                let dbus_proxy = zbus::fdo::DBusProxy::new(&connection).await?;
                let mut names = dbus_proxy.list_names().await?;
                
                // Filter out internal D-Bus names
                names.retain(|name| !name.starts_with(':'));
                
                // Apply filter if provided
                if let Some(filter_pattern) = &filter {
                    names.retain(|name| name.contains(filter_pattern));
                }
                
                Ok(ToolResult {
                    content: vec![ToolContent::json(json!({
                        "bus": bus_type,
                        "services": names,
                        "count": names.len()
                    }))],
                    metadata: None,
                })
            })
        })
        .build();
    
    registry.register_tool(Box::new(tool)).await?;
    Ok(())
}

/// Tool: Introspect a D-Bus service
async fn register_introspect_service_tool(
    registry: &crate::tool_registry::ToolRegistry,
) -> Result<()> {
    let tool = DynamicToolBuilder::new("dbus_introspect_service")
        .description("Get complete introspection data for a D-Bus service")
        .schema(json!({
            "type": "object",
            "properties": {
                "service": {
                    "type": "string",
                    "description": "Service name (e.g., org.freedesktop.systemd1)"
                },
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system"
                },
                "path": {
                    "type": "string",
                    "default": "/",
                    "description": "Object path to introspect"
                }
            },
            "required": ["service"]
        }))
        .handler(|params| {
            Box::pin(async move {
                let service = params["service"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing service parameter"))?;
                
                let bus_type = params["bus"].as_str().unwrap_or("system");
                let path = params["path"].as_str().unwrap_or("/");
                
                let connection = if bus_type == "system" {
                    Connection::system().await?
                } else {
                    Connection::session().await?
                };
                
                // Use cache for JSON conversion (XML → JSON happens once)
                let cache_path = get_cache_path();
                let cache = IntrospectionCache::new(&cache_path)?;
                
                // Check cache first
                let cached_json = cache.get_introspection_json(service, path, None)?;
                
                if let Some(json_data) = cached_json {
                    // Return cached JSON directly
                    Ok(ToolResult {
                        content: vec![ToolContent::json(json!({
                            "service": service,
                            "path": path,
                            "bus": bus_type,
                            "cached": true,
                            "data": json_data
                        }))],
                        metadata: None,
                    })
                } else {
                    // Not cached - introspect and cache
                    // Handle non-introspectable objects gracefully
                    let proxy_result = Proxy::new(
                        &connection,
                        service,
                        path,
                        "org.freedesktop.DBus.Introspectable",
                    ).await;
                    
                    match proxy_result {
                        Ok(proxy) => {
                            match proxy.call::<_, String>("Introspect", &()).await {
                                Ok(xml) => {
                                    // Store in cache (converts XML → JSON automatically)
                                    cache.store_introspection(service, path, &xml)?;
                                    
                                    // Get JSON from cache
                                    let json_data = cache.get_introspection_json(service, path, None)?
                                        .ok_or_else(|| anyhow::anyhow!("Failed to retrieve cached JSON"))?;
                                    
                                    Ok(ToolResult {
                                        content: vec![ToolContent::json(json!({
                                            "service": service,
                                            "path": path,
                                            "bus": bus_type,
                                            "cached": false,
                                            "data": json_data
                                        }))],
                                        metadata: None,
                                    })
                                }
                                Err(e) => {
                                    // Non-introspectable object - return error info
                                    Ok(ToolResult {
                                        content: vec![ToolContent::json(json!({
                                            "service": service,
                                            "path": path,
                                            "bus": bus_type,
                                            "error": "non_introspectable",
                                            "error_type": format!("{:?}", e),
                                            "message": "Object exists but cannot be introspected. May be missing Introspectable interface, have permission restrictions, or be a dynamic/proxy object."
                                        }))],
                                        metadata: None,
                                    })
                                }
                            }
                        }
                        Err(e) => {
                            // Proxy creation failed
                            Ok(ToolResult {
                                content: vec![ToolContent::json(json!({
                                    "service": service,
                                    "path": path,
                                    "bus": bus_type,
                                    "error": "proxy_creation_failed",
                                    "error_type": format!("{:?}", e),
                                    "message": "Failed to create proxy. Object may not exist or service may be unavailable."
                                }))],
                                metadata: None,
                            })
                        }
                    }
                }
            })
        })
        .build();
    
    registry.register_tool(Box::new(tool)).await?;
    Ok(())
}

/// Tool: List object paths in a service
async fn register_list_objects_tool(
    registry: &crate::tool_registry::ToolRegistry,
) -> Result<()> {
    let tool = DynamicToolBuilder::new("dbus_list_objects")
        .description("List all object paths in a D-Bus service (uses ObjectManager if available)")
        .schema(json!({
            "type": "object",
            "properties": {
                "service": {
                    "type": "string",
                    "description": "Service name"
                },
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system"
                },
                "path": {
                    "type": "string",
                    "default": "/",
                    "description": "Starting path for recursive search"
                }
            },
            "required": ["service"]
        }))
        .handler(|params| {
            Box::pin(async move {
                let service = params["service"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing service parameter"))?;
                
                let bus_type = params["bus"].as_str().unwrap_or("system");
                let start_path = params["path"].as_str().unwrap_or("/");
                
                let connection = if bus_type == "system" {
                    Connection::system().await?
                } else {
                    Connection::session().await?
                };
                
                let mut objects = Vec::new();
                let mut non_introspectable = Vec::new();
                
                // Strategy 1: Try ObjectManager first (fastest, most complete)
                let object_manager_paths = vec![
                    start_path.to_string(),
                    format!("/{}", service.replace('.', "/")),
                    "/".to_string(),
                ];
                
                for om_path in object_manager_paths {
                    if let Ok(proxy) = Proxy::new(
                        &connection,
                        service,
                        &om_path,
                        "org.freedesktop.DBus.ObjectManager",
                    ).await {
                        if let Ok(managed) = proxy.call::<_, HashMap<String, HashMap<String, Value>>>(
                            "GetManagedObjects",
                            &(),
                        ).await {
                            // Extract all object paths from ObjectManager
                            objects.extend(managed.keys().cloned());
                            break; // Success, no need to try other paths
                        }
                    }
                }
                
                // Strategy 2: Fallback to recursive introspection if ObjectManager unavailable
                if objects.is_empty() {
                    let (found, non_intro) = recursive_introspect_with_errors(&connection, service, start_path).await?;
                    objects = found;
                    non_introspectable = non_intro;
                }
                
                Ok(ToolResult {
                    content: vec![ToolContent::json(json!({
                        "service": service,
                        "objects": objects,
                        "count": objects.len(),
                        "non_introspectable": non_introspectable,
                        "non_introspectable_count": non_introspectable.len()
                    }))],
                    metadata: None,
                })
            })
        })
        .build();
    
    registry.register_tool(Box::new(tool)).await?;
    Ok(())
}

/// Tool: Introspect a specific object
async fn register_introspect_object_tool(
    registry: &crate::tool_registry::ToolRegistry,
) -> Result<()> {
    let tool = DynamicToolBuilder::new("dbus_introspect_object")
        .description("Get complete introspection for a specific D-Bus object path")
        .schema(json!({
            "type": "object",
            "properties": {
                "service": {
                    "type": "string",
                    "description": "Service name"
                },
                "path": {
                    "type": "string",
                    "description": "Object path (e.g., /org/freedesktop/systemd1/unit/ssh_2eservice)"
                },
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system"
                }
            },
            "required": ["service", "path"]
        }))
        .handler(|params| {
            Box::pin(async move {
                let service = params["service"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing service parameter"))?;
                
                let path = params["path"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing path parameter"))?;
                
                let bus_type = params["bus"].as_str().unwrap_or("system");
                
                let connection = if bus_type == "system" {
                    Connection::system().await?
                } else {
                    Connection::session().await?
                };
                
                // Use cache for JSON conversion
                let cache_path = get_cache_path();
                let cache = IntrospectionCache::new(&cache_path)?;
                
                // Check cache first
                let cached_json = cache.get_introspection_json(service, path, None)?;
                
                if let Some(json_data) = cached_json {
                    Ok(ToolResult {
                        content: vec![ToolContent::json(json!({
                            "service": service,
                            "path": path,
                            "cached": true,
                            "data": json_data
                        }))],
                        metadata: None,
                    })
                } else {
                    // Not cached - introspect and cache
                    // Handle non-introspectable objects gracefully
                    let proxy_result = Proxy::new(
                        &connection,
                        service,
                        path,
                        "org.freedesktop.DBus.Introspectable",
                    ).await;
                    
                    match proxy_result {
                        Ok(proxy) => {
                            match proxy.call::<_, String>("Introspect", &()).await {
                                Ok(xml) => {
                                    cache.store_introspection(service, path, &xml)?;
                                    
                                    let json_data = cache.get_introspection_json(service, path, None)?
                                        .ok_or_else(|| anyhow::anyhow!("Failed to retrieve cached JSON"))?;
                                    
                                    Ok(ToolResult {
                                        content: vec![ToolContent::json(json!({
                                            "service": service,
                                            "path": path,
                                            "cached": false,
                                            "data": json_data
                                        }))],
                                        metadata: None,
                                    })
                                }
                                Err(e) => {
                                    // Non-introspectable object - return error info
                                    let error_name = if e.to_string().contains("UnknownMethod") {
                                        "missing_introspectable_interface"
                                    } else if e.to_string().contains("AccessDenied") || e.to_string().contains("Access denied") {
                                        "permission_denied"
                                    } else if e.to_string().contains("UnknownObject") {
                                        "object_not_found"
                                    } else {
                                        "introspection_failed"
                                    };
                                    
                                    Ok(ToolResult {
                                        content: vec![ToolContent::json(json!({
                                            "service": service,
                                            "path": path,
                                            "error": error_name,
                                            "error_type": format!("{:?}", e),
                                            "message": "Object exists but cannot be introspected. May be missing Introspectable interface, have permission restrictions, or be a dynamic/proxy object.",
                                            "workarounds": [
                                                "Try using ObjectManager.GetManagedObjects if available",
                                                "Check if object implements org.freedesktop.DBus.Properties and use GetAll",
                                                "Verify D-Bus policy allows introspection",
                                                "Check service documentation for object structure"
                                            ]
                                        }))],
                                        metadata: None,
                                    })
                                }
                            }
                        }
                        Err(e) => {
                            // Proxy creation failed
                            Ok(ToolResult {
                                content: vec![ToolContent::json(json!({
                                    "service": service,
                                    "path": path,
                                    "error": "proxy_creation_failed",
                                    "error_type": format!("{:?}", e),
                                    "message": "Failed to create proxy. Object may not exist or service may be unavailable."
                                }))],
                                metadata: None,
                            })
                        }
                    }
                }
            })
        })
        .build();
    
    registry.register_tool(Box::new(tool)).await?;
    Ok(())
}

/// Tool: List interfaces on an object
async fn register_list_interfaces_tool(
    registry: &crate::tool_registry::ToolRegistry,
) -> Result<()> {
    let tool = DynamicToolBuilder::new("dbus_list_interfaces")
        .description("List all interfaces implemented by a D-Bus object")
        .schema(json!({
            "type": "object",
            "properties": {
                "service": {"type": "string"},
                "path": {"type": "string"},
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system"
                }
            },
            "required": ["service", "path"]
        }))
        .handler(|params| {
            Box::pin(async move {
                let service = params["service"].as_str().unwrap();
                let path = params["path"].as_str().unwrap();
                let bus_type = params["bus"].as_str().unwrap_or("system");
                
                let connection = if bus_type == "system" {
                    Connection::system().await?
                } else {
                    Connection::session().await?
                };
                
                // Use cache for JSON (no XML parsing)
                let cache_path = get_cache_path();
                let cache = IntrospectionCache::new(&cache_path)?;
                
                // Get from cache or introspect and cache
                let json_data = if let Some(cached) = cache.get_introspection_json(service, path, None)? {
                    cached
                } else {
                    let proxy = Proxy::new(
                        &connection,
                        service,
                        path,
                        "org.freedesktop.DBus.Introspectable",
                    ).await?;
                    let xml: String = proxy.call("Introspect", &()).await?;
                    cache.store_introspection(service, path, &xml)?;
                    cache.get_introspection_json(service, path, None)?
                        .ok_or_else(|| anyhow::anyhow!("Failed to get cached JSON"))?
                };
                
                // Extract interface names from JSON
                let interfaces: Vec<String> = json_data["interfaces"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|i| i["name"].as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                
                Ok(ToolResult {
                    content: vec![ToolContent::json(json!({
                        "service": service,
                        "path": path,
                        "interfaces": interfaces
                    }))],
                    metadata: None,
                })
            })
        })
        .build();
    
    registry.register_tool(Box::new(tool)).await?;
    Ok(())
}

/// Tool: List methods in an interface
async fn register_list_methods_tool(
    registry: &crate::tool_registry::ToolRegistry,
) -> Result<()> {
    let tool = DynamicToolBuilder::new("dbus_list_methods")
        .description("List all methods in a D-Bus interface")
        .schema(json!({
            "type": "object",
            "properties": {
                "service": {"type": "string"},
                "path": {"type": "string"},
                "interface": {"type": "string"},
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system"
                }
            },
            "required": ["service", "path", "interface"]
        }))
        .handler(|params| {
            Box::pin(async move {
                let service = params["service"].as_str().unwrap();
                let path = params["path"].as_str().unwrap();
                let interface = params["interface"].as_str().unwrap();
                let bus_type = params["bus"].as_str().unwrap_or("system");
                
                let connection = if bus_type == "system" {
                    Connection::system().await?
                } else {
                    Connection::session().await?
                };
                
                // Use cache for JSON (no XML parsing)
                let cache_path = get_cache_path();
                let cache = IntrospectionCache::new(&cache_path)?;
                
                // Get methods directly from cache (fast indexed lookup)
                let methods_json = cache.get_methods_json(service, interface)?;
                
                Ok(ToolResult {
                    content: vec![ToolContent::json(json!({
                        "service": service,
                        "path": path,
                        "interface": interface,
                        "methods": methods_json["methods"]
                    }))],
                    metadata: None,
                })
            })
        })
        .build();
    
    registry.register_tool(Box::new(tool)).await?;
    Ok(())
}

/// Tool: List properties in an interface
async fn register_list_properties_tool(
    registry: &crate::tool_registry::ToolRegistry,
) -> Result<()> {
    let tool = DynamicToolBuilder::new("dbus_list_properties")
        .description("List all properties in a D-Bus interface")
        .schema(json!({
            "type": "object",
            "properties": {
                "service": {"type": "string"},
                "path": {"type": "string"},
                "interface": {"type": "string"},
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system"
                }
            },
            "required": ["service", "path", "interface"]
        }))
        .handler(|params| {
            Box::pin(async move {
                let service = params["service"].as_str().unwrap();
                let path = params["path"].as_str().unwrap();
                let interface = params["interface"].as_str().unwrap();
                let bus_type = params["bus"].as_str().unwrap_or("system");
                
                let connection = if bus_type == "system" {
                    Connection::system().await?
                } else {
                    Connection::session().await?
                };
                
                // Use cache for JSON
                let cache_path = get_cache_path();
                let cache = IntrospectionCache::new(&cache_path)?;
                
                // Get from cache or introspect and cache
                let json_data = if let Some(cached) = cache.get_introspection_json(service, path, Some(interface))? {
                    cached
                } else {
                    let proxy = Proxy::new(
                        &connection,
                        service,
                        path,
                        "org.freedesktop.DBus.Introspectable",
                    ).await?;
                    let xml: String = proxy.call("Introspect", &()).await?;
                    cache.store_introspection(service, path, &xml)?;
                    cache.get_introspection_json(service, path, Some(interface))?
                        .ok_or_else(|| anyhow::anyhow!("Failed to get cached JSON"))?
                };
                
                // Extract properties from JSON
                let properties = json_data["interfaces"]
                    .as_array()
                    .and_then(|arr| arr.iter().find(|i| i["name"].as_str() == Some(interface)))
                    .and_then(|iface| iface["properties"].as_array())
                    .cloned()
                    .unwrap_or_default();
                
                Ok(ToolResult {
                    content: vec![ToolContent::json(json!({
                        "service": service,
                        "path": path,
                        "interface": interface,
                        "properties": properties
                    }))],
                    metadata: None,
                })
            })
        })
        .build();
    
    registry.register_tool(Box::new(tool)).await?;
    Ok(())
}

/// Tool: List signals in an interface
async fn register_list_signals_tool(
    registry: &crate::tool_registry::ToolRegistry,
) -> Result<()> {
    let tool = DynamicToolBuilder::new("dbus_list_signals")
        .description("List all signals in a D-Bus interface")
        .schema(json!({
            "type": "object",
            "properties": {
                "service": {"type": "string"},
                "path": {"type": "string"},
                "interface": {"type": "string"},
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system"
                }
            },
            "required": ["service", "path", "interface"]
        }))
        .handler(|params| {
            Box::pin(async move {
                let service = params["service"].as_str().unwrap();
                let path = params["path"].as_str().unwrap();
                let interface = params["interface"].as_str().unwrap();
                let bus_type = params["bus"].as_str().unwrap_or("system");
                
                let connection = if bus_type == "system" {
                    Connection::system().await?
                } else {
                    Connection::session().await?
                };
                
                // Use cache for JSON
                let cache_path = get_cache_path();
                let cache = IntrospectionCache::new(&cache_path)?;
                
                // Get from cache or introspect and cache
                let json_data = if let Some(cached) = cache.get_introspection_json(service, path, Some(interface))? {
                    cached
                } else {
                    let proxy = Proxy::new(
                        &connection,
                        service,
                        path,
                        "org.freedesktop.DBus.Introspectable",
                    ).await?;
                    let xml: String = proxy.call("Introspect", &()).await?;
                    cache.store_introspection(service, path, &xml)?;
                    cache.get_introspection_json(service, path, Some(interface))?
                        .ok_or_else(|| anyhow::anyhow!("Failed to get cached JSON"))?
                };
                
                // Extract signals from JSON
                let signals = json_data["interfaces"]
                    .as_array()
                    .and_then(|arr| arr.iter().find(|i| i["name"].as_str() == Some(interface)))
                    .and_then(|iface| iface["signals"].as_array())
                    .cloned()
                    .unwrap_or_default();
                
                Ok(ToolResult {
                    content: vec![ToolContent::json(json!({
                        "service": service,
                        "path": path,
                        "interface": interface,
                        "signals": signals
                    }))],
                    metadata: None,
                })
            })
        })
        .build();
    
    registry.register_tool(Box::new(tool)).await?;
    Ok(())
}

/// Tool: Call a D-Bus method
async fn register_call_method_tool(
    registry: &crate::tool_registry::ToolRegistry,
) -> Result<()> {
    let tool = DynamicToolBuilder::new("dbus_call_method")
        .description("Call a D-Bus method with arguments")
        .schema(json!({
            "type": "object",
            "properties": {
                "service": {"type": "string"},
                "path": {"type": "string"},
                "interface": {"type": "string"},
                "method": {"type": "string"},
                "args": {
                    "type": "array",
                    "description": "Method arguments (as JSON values)"
                },
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system"
                }
            },
            "required": ["service", "path", "interface", "method"]
        }))
        .handler(|params| {
            Box::pin(async move {
                let service = params["service"].as_str().unwrap();
                let path = params["path"].as_str().unwrap();
                let interface = params["interface"].as_str().unwrap();
                let method = params["method"].as_str().unwrap();
                let bus_type = params["bus"].as_str().unwrap_or("system");
                
                let connection = if bus_type == "system" {
                    Connection::system().await?
                } else {
                    Connection::session().await?
                };
                
                let proxy = Proxy::new(
                    &connection,
                    service,
                    path,
                    interface,
                ).await?;
                
                // Parse arguments
                let args: Vec<Value> = params["args"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                
                // Convert JSON values to zbus::Value
                // This is simplified - in production would need proper type conversion
                let zbus_args: Vec<zbus::zvariant::Value> = args.iter()
                    .map(|v| zbus::zvariant::to_value(v).unwrap())
                    .collect();
                
                let result: zbus::zvariant::Value = proxy.call(method, &zbus_args).await?;
                let result_json: Value = serde_json::to_value(&result)?;
                
                Ok(ToolResult {
                    content: vec![ToolContent::json(json!({
                        "service": service,
                        "path": path,
                        "interface": interface,
                        "method": method,
                        "result": result_json
                    }))],
                    metadata: None,
                })
            })
        })
        .build();
    
    registry.register_tool(Box::new(tool)).await?;
    Ok(())
}

/// Tool: Get a property value
async fn register_get_property_tool(
    registry: &crate::tool_registry::ToolRegistry,
) -> Result<()> {
    let tool = DynamicToolBuilder::new("dbus_get_property")
        .description("Get the value of a D-Bus property")
        .schema(json!({
            "type": "object",
            "properties": {
                "service": {"type": "string"},
                "path": {"type": "string"},
                "interface": {"type": "string"},
                "property": {"type": "string"},
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system"
                }
            },
            "required": ["service", "path", "interface", "property"]
        }))
        .handler(|params| {
            Box::pin(async move {
                let service = params["service"].as_str().unwrap();
                let path = params["path"].as_str().unwrap();
                let interface = params["interface"].as_str().unwrap();
                let property = params["property"].as_str().unwrap();
                let bus_type = params["bus"].as_str().unwrap_or("system");
                
                let connection = if bus_type == "system" {
                    Connection::system().await?
                } else {
                    Connection::session().await?
                };
                
                let proxy = Proxy::new(
                    &connection,
                    service,
                    path,
                    interface,
                ).await?;
                
                // Use Properties interface
                let properties_proxy = zbus::fdo::PropertiesProxy::builder(&connection)
                    .destination(service)?
                    .path(path)?
                    .build()
                    .await?;
                
                let value: zbus::zvariant::Value = properties_proxy
                    .get(interface, property)
                    .await?;
                
                let value_json: Value = serde_json::to_value(&value)?;
                
                Ok(ToolResult {
                    content: vec![ToolContent::json(json!({
                        "service": service,
                        "path": path,
                        "interface": interface,
                        "property": property,
                        "value": value_json
                    }))],
                    metadata: None,
                })
            })
        })
        .build();
    
    registry.register_tool(Box::new(tool)).await?;
    Ok(())
}

/// Tool: Set a property value
async fn register_set_property_tool(
    registry: &crate::tool_registry::ToolRegistry,
) -> Result<()> {
    let tool = DynamicToolBuilder::new("dbus_set_property")
        .description("Set the value of a D-Bus property")
        .schema(json!({
            "type": "object",
            "properties": {
                "service": {"type": "string"},
                "path": {"type": "string"},
                "interface": {"type": "string"},
                "property": {"type": "string"},
                "value": {
                    "description": "Property value (as JSON)"
                },
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system"
                }
            },
            "required": ["service", "path", "interface", "property", "value"]
        }))
        .handler(|params| {
            Box::pin(async move {
                let service = params["service"].as_str().unwrap();
                let path = params["path"].as_str().unwrap();
                let interface = params["interface"].as_str().unwrap();
                let property = params["property"].as_str().unwrap();
                let value = params.get("value").unwrap();
                let bus_type = params["bus"].as_str().unwrap_or("system");
                
                let connection = if bus_type == "system" {
                    Connection::system().await?
                } else {
                    Connection::session().await?
                };
                
                let properties_proxy = zbus::fdo::PropertiesProxy::builder(&connection)
                    .destination(service)?
                    .path(path)?
                    .build()
                    .await?;
                
                let zbus_value: zbus::zvariant::Value = zbus::zvariant::to_value(value)?;
                
                properties_proxy
                    .set(interface, property, zbus_value)
                    .await?;
                
                Ok(ToolResult {
                    content: vec![ToolContent::json(json!({
                        "success": true,
                        "service": service,
                        "path": path,
                        "interface": interface,
                        "property": property
                    }))],
                    metadata: None,
                })
            })
        })
        .build();
    
    registry.register_tool(Box::new(tool)).await?;
    Ok(())
}

/// Tool: Get all properties of an object
async fn register_get_all_properties_tool(
    registry: &crate::tool_registry::ToolRegistry,
) -> Result<()> {
    let tool = DynamicToolBuilder::new("dbus_get_all_properties")
        .description("Get all properties of a D-Bus object (all interfaces)")
        .schema(json!({
            "type": "object",
            "properties": {
                "service": {"type": "string"},
                "path": {"type": "string"},
                "interface": {
                    "type": "string",
                    "description": "Optional: specific interface, otherwise all interfaces"
                },
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system"
                }
            },
            "required": ["service", "path"]
        }))
        .handler(|params| {
            Box::pin(async move {
                let service = params["service"].as_str().unwrap();
                let path = params["path"].as_str().unwrap();
                let interface_filter = params["interface"].as_str();
                let bus_type = params["bus"].as_str().unwrap_or("system");
                
                let connection = if bus_type == "system" {
                    Connection::system().await?
                } else {
                    Connection::session().await?
                };
                
                // Use cache for JSON (get interface list)
                let cache_path = get_cache_path();
                let cache = IntrospectionCache::new(&cache_path)?;
                
                // Get from cache or introspect and cache
                let json_data = if let Some(cached) = cache.get_introspection_json(service, path, None)? {
                    cached
                } else {
                    let proxy = Proxy::new(
                        &connection,
                        service,
                        path,
                        "org.freedesktop.DBus.Introspectable",
                    ).await?;
                    let xml: String = proxy.call("Introspect", &()).await?;
                    cache.store_introspection(service, path, &xml)?;
                    cache.get_introspection_json(service, path, None)?
                        .ok_or_else(|| anyhow::anyhow!("Failed to get cached JSON"))?
                };
                
                // Get property values via Properties interface
                let properties_proxy = zbus::fdo::PropertiesProxy::builder(&connection)
                    .destination(service)?
                    .path(path)?
                    .build()
                    .await?;
                
                let mut all_properties = json!({});
                
                // Extract interfaces from JSON
                if let Some(interfaces) = json_data["interfaces"].as_array() {
                    for iface in interfaces {
                        let iface_name = iface["name"].as_str()
                            .ok_or_else(|| anyhow::anyhow!("Invalid interface name"))?;
                        
                        if let Some(filter) = interface_filter {
                            if iface_name != filter {
                                continue;
                            }
                        }
                        
                        // Get all properties for this interface
                        let props: HashMap<String, zbus::zvariant::Value> = properties_proxy
                            .get_all(iface_name)
                            .await
                            .unwrap_or_default();
                        
                        let mut iface_props = json!({});
                        for (prop_name, prop_value) in props {
                            let value_json: Value = serde_json::to_value(&prop_value)?;
                            iface_props[prop_name] = value_json;
                        }
                        
                        all_properties[iface_name] = iface_props;
                    }
                }
                
                Ok(ToolResult {
                    content: vec![ToolContent::json(json!({
                        "service": service,
                        "path": path,
                        "properties": all_properties
                    }))],
                    metadata: None,
                })
            })
        })
        .build();
    
    registry.register_tool(Box::new(tool)).await?;
    Ok(())
}

/// Helper: Recursively introspect object paths with error handling
/// Returns (successful objects, non-introspectable objects with errors)
async fn recursive_introspect_with_errors(
    connection: &Connection,
    service: &str,
    start_path: &str,
) -> Result<(Vec<String>, Vec<serde_json::Value>)> {
    let mut objects = vec![start_path.to_string()];
    let mut non_introspectable = Vec::new();
    let mut to_process = vec![start_path.to_string()];
    let mut processed = std::collections::HashSet::new();
    
    while let Some(current_path) = to_process.pop() {
        if processed.contains(&current_path) {
            continue;
        }
        processed.insert(current_path.clone());
        
        // Try to introspect this object
        let proxy_result = Proxy::new(
            connection,
            service,
            &current_path,
            "org.freedesktop.DBus.Introspectable",
        ).await;
        
        match proxy_result {
            Ok(proxy) => {
                match proxy.call::<_, String>("Introspect", &()).await {
                    Ok(xml) => {
                        // Parse XML to find child nodes using zbus_xml
                        match zbus_xml::Node::from_reader(xml.as_bytes()) {
                            Ok(node) => {
                                // Add child nodes to process queue
                                for child in node.nodes() {
                                    if let Some(child_name) = child.name() {
                                        let child_path = if current_path == "/" {
                                            format!("/{}", child_name)
                                        } else {
                                            format!("{}/{}", current_path, child_name)
                                        };
                                        
                                        if !processed.contains(&child_path) {
                                            objects.push(child_path.clone());
                                            to_process.push(child_path);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                non_introspectable.push(json!({
                                    "path": current_path,
                                    "error": "xml_parse_failed",
                                    "message": format!("Failed to parse introspection XML: {}", e)
                                }));
                            }
                        }
                    }
                    Err(e) => {
                        // Non-introspectable object
                        let error_name = if e.to_string().contains("UnknownMethod") {
                            "missing_introspectable_interface"
                        } else if e.to_string().contains("AccessDenied") || e.to_string().contains("Access denied") {
                            "permission_denied"
                        } else if e.to_string().contains("UnknownObject") {
                            "object_not_found"
                        } else {
                            "introspection_failed"
                        };
                        
                        non_introspectable.push(json!({
                            "path": current_path,
                            "error": error_name,
                            "error_type": format!("{:?}", e),
                            "message": format!("Cannot introspect object: {}", e)
                        }));
                    }
                }
            }
            Err(e) => {
                // Proxy creation failed - object may not exist
                non_introspectable.push(json!({
                    "path": current_path,
                    "error": "proxy_creation_failed",
                    "error_type": format!("{:?}", e),
                    "message": format!("Failed to create proxy: {}", e)
                }));
            }
        }
    }
    
    Ok((objects, non_introspectable))
}

