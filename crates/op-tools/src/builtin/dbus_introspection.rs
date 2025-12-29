//! D-Bus introspection tools (granular APIs).
//!
//! These tools provide the public-facing D-Bus and introspection helpers that
//! show up in the tool registry.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use op_core::{BusType, InterfaceInfo, ObjectInfo};
use op_introspection::IntrospectionService;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use zbus::Connection;

use crate::{Tool, ToolRegistry};

fn parse_bus(input: &Value, key: &str) -> BusType {
    match input.get(key).and_then(|v| v.as_str()).unwrap_or("system") {
        "session" => BusType::Session,
        _ => BusType::System,
    }
}

fn bus_str(bus: BusType) -> &'static str {
    match bus {
        BusType::System => "system",
        BusType::Session => "session",
    }
}

fn find_interface<'a>(info: &'a ObjectInfo, interface: &str) -> Result<&'a InterfaceInfo> {
    info.interfaces
        .iter()
        .find(|iface| iface.name == interface)
        .ok_or_else(|| anyhow!("Interface not found: {}", interface))
}

fn parse_required_str(input: &Value, key: &str) -> Result<String> {
    input
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("Missing required parameter: {}", key))
}

fn json_to_owned_value(value: &Value) -> Result<zbus::zvariant::OwnedValue> {
    use zbus::zvariant::Str as ZStr;

    match value {
        Value::String(s) => Ok(zbus::zvariant::OwnedValue::from(ZStr::from(s.as_str()))),
        Value::Bool(b) => Ok(zbus::zvariant::OwnedValue::from(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(zbus::zvariant::OwnedValue::from(i))
            } else if let Some(u) = n.as_u64() {
                Ok(zbus::zvariant::OwnedValue::from(u))
            } else if let Some(f) = n.as_f64() {
                Ok(zbus::zvariant::OwnedValue::from(f))
            } else {
                Err(anyhow!("Unsupported numeric value"))
            }
        }
        _ => Err(anyhow!(
            "Unsupported argument type; use string/number/bool"
        )),
    }
}

pub async fn register_dbus_introspection_tools(registry: &ToolRegistry) -> Result<()> {
    let introspection = Arc::new(IntrospectionService::new());

    registry
        .register_tool(Arc::new(DbusListServicesTool::new(introspection.clone())))
        .await?;
    registry
        .register_tool(Arc::new(DbusIntrospectServiceTool::new(introspection.clone())))
        .await?;
    registry
        .register_tool(Arc::new(DbusListObjectsTool::new(introspection.clone())))
        .await?;
    registry
        .register_tool(Arc::new(DbusIntrospectObjectTool::new(introspection.clone())))
        .await?;
    registry
        .register_tool(Arc::new(DbusListInterfacesTool::new(introspection.clone())))
        .await?;
    registry
        .register_tool(Arc::new(DbusListMethodsTool::new(introspection.clone())))
        .await?;
    registry
        .register_tool(Arc::new(DbusListPropertiesTool::new(introspection.clone())))
        .await?;
    registry
        .register_tool(Arc::new(DbusListSignalsTool::new(introspection.clone())))
        .await?;
    registry
        .register_tool(Arc::new(DbusCallMethodTool))
        .await?;
    registry
        .register_tool(Arc::new(DbusGetPropertyTool))
        .await?;
    registry
        .register_tool(Arc::new(DbusSetPropertyTool))
        .await?;
    registry
        .register_tool(Arc::new(DbusGetAllPropertiesTool::new(introspection)))
        .await?;

    Ok(())
}

struct DbusListServicesTool {
    introspection: Arc<IntrospectionService>,
}

impl DbusListServicesTool {
    fn new(introspection: Arc<IntrospectionService>) -> Self {
        Self { introspection }
    }
}

#[async_trait]
impl Tool for DbusListServicesTool {
    fn name(&self) -> &str {
        "dbus_list_services"
    }

    fn description(&self) -> &str {
        "List all available D-Bus services on system or session bus"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system"
                },
                "filter": {
                    "type": "string",
                    "description": "Optional filter pattern (e.g., 'org.freedesktop')"
                }
            }
        })
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let bus = parse_bus(&input, "bus");
        let filter = input.get("filter").and_then(|v| v.as_str());
        let services = self.introspection.list_services(bus).await?;
        let mut names: Vec<String> = services.into_iter().map(|s| s.name).collect();

        names.retain(|name| !name.starts_with(':'));
        if let Some(pattern) = filter {
            names.retain(|name| name.contains(pattern));
        }

        Ok(json!({
            "bus": bus_str(bus),
            "count": names.len(),
            "services": names
        }))
    }

    fn category(&self) -> &str {
        "dbus"
    }
}

struct DbusIntrospectServiceTool {
    introspection: Arc<IntrospectionService>,
}

impl DbusIntrospectServiceTool {
    fn new(introspection: Arc<IntrospectionService>) -> Self {
        Self { introspection }
    }
}

#[async_trait]
impl Tool for DbusIntrospectServiceTool {
    fn name(&self) -> &str {
        "dbus_introspect_service"
    }

    fn description(&self) -> &str {
        "Get complete introspection data for a D-Bus service"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "service": { "type": "string" },
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system"
                },
                "path": {
                    "type": "string",
                    "default": "/"
                }
            },
            "required": ["service"]
        })
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let service = parse_required_str(&input, "service")?;
        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("/");
        let bus = parse_bus(&input, "bus");
        let data = self
            .introspection
            .introspect_json(bus, &service, path)
            .await?;

        Ok(json!({
            "bus": bus_str(bus),
            "service": service,
            "path": path,
            "data": data
        }))
    }

    fn category(&self) -> &str {
        "dbus"
    }
}

struct DbusListObjectsTool {
    introspection: Arc<IntrospectionService>,
}

impl DbusListObjectsTool {
    fn new(introspection: Arc<IntrospectionService>) -> Self {
        Self { introspection }
    }
}

#[async_trait]
impl Tool for DbusListObjectsTool {
    fn name(&self) -> &str {
        "dbus_list_objects"
    }

    fn description(&self) -> &str {
        "List object paths for a D-Bus service"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "service": { "type": "string" },
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system"
                },
                "path": {
                    "type": "string",
                    "default": "/"
                }
            },
            "required": ["service"]
        })
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let service = parse_required_str(&input, "service")?;
        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("/");
        let bus = parse_bus(&input, "bus");
        let info = self.introspection.introspect(bus, &service, path).await?;

        Ok(json!({
            "bus": bus_str(bus),
            "service": service,
            "path": path,
            "objects": info.children
        }))
    }

    fn category(&self) -> &str {
        "dbus"
    }
}

struct DbusIntrospectObjectTool {
    introspection: Arc<IntrospectionService>,
}

impl DbusIntrospectObjectTool {
    fn new(introspection: Arc<IntrospectionService>) -> Self {
        Self { introspection }
    }
}

#[async_trait]
impl Tool for DbusIntrospectObjectTool {
    fn name(&self) -> &str {
        "dbus_introspect_object"
    }

    fn description(&self) -> &str {
        "Introspect a specific D-Bus object path"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "service": { "type": "string" },
                "path": { "type": "string" },
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system"
                }
            },
            "required": ["service", "path"]
        })
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let service = parse_required_str(&input, "service")?;
        let path = parse_required_str(&input, "path")?;
        let bus = parse_bus(&input, "bus");
        let data = self
            .introspection
            .introspect_json(bus, &service, &path)
            .await?;

        Ok(json!({
            "bus": bus_str(bus),
            "service": service,
            "path": path,
            "data": data
        }))
    }

    fn category(&self) -> &str {
        "dbus"
    }
}

struct DbusListInterfacesTool {
    introspection: Arc<IntrospectionService>,
}

impl DbusListInterfacesTool {
    fn new(introspection: Arc<IntrospectionService>) -> Self {
        Self { introspection }
    }
}

#[async_trait]
impl Tool for DbusListInterfacesTool {
    fn name(&self) -> &str {
        "dbus_list_interfaces"
    }

    fn description(&self) -> &str {
        "List interfaces for a D-Bus object"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "service": { "type": "string" },
                "path": { "type": "string", "default": "/" },
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system"
                }
            },
            "required": ["service"]
        })
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let service = parse_required_str(&input, "service")?;
        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("/");
        let bus = parse_bus(&input, "bus");
        let info = self.introspection.introspect(bus, &service, path).await?;
        let interfaces: Vec<String> = info.interfaces.into_iter().map(|i| i.name).collect();

        Ok(json!({
            "bus": bus_str(bus),
            "service": service,
            "path": path,
            "interfaces": interfaces
        }))
    }

    fn category(&self) -> &str {
        "dbus"
    }
}

struct DbusListMethodsTool {
    introspection: Arc<IntrospectionService>,
}

impl DbusListMethodsTool {
    fn new(introspection: Arc<IntrospectionService>) -> Self {
        Self { introspection }
    }
}

#[async_trait]
impl Tool for DbusListMethodsTool {
    fn name(&self) -> &str {
        "dbus_list_methods"
    }

    fn description(&self) -> &str {
        "List methods for a D-Bus interface"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "service": { "type": "string" },
                "path": { "type": "string", "default": "/" },
                "interface": { "type": "string" },
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system"
                }
            },
            "required": ["service", "interface"]
        })
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let service = parse_required_str(&input, "service")?;
        let interface = parse_required_str(&input, "interface")?;
        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("/");
        let bus = parse_bus(&input, "bus");
        let info = self.introspection.introspect(bus, &service, path).await?;
        let iface = find_interface(&info, &interface)?;
        let methods: Vec<String> = iface.methods.iter().map(|m| m.name.clone()).collect();

        Ok(json!({
            "bus": bus_str(bus),
            "service": service,
            "path": path,
            "interface": interface,
            "methods": methods
        }))
    }

    fn category(&self) -> &str {
        "dbus"
    }
}

struct DbusListPropertiesTool {
    introspection: Arc<IntrospectionService>,
}

impl DbusListPropertiesTool {
    fn new(introspection: Arc<IntrospectionService>) -> Self {
        Self { introspection }
    }
}

#[async_trait]
impl Tool for DbusListPropertiesTool {
    fn name(&self) -> &str {
        "dbus_list_properties"
    }

    fn description(&self) -> &str {
        "List properties for a D-Bus interface"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "service": { "type": "string" },
                "path": { "type": "string", "default": "/" },
                "interface": { "type": "string" },
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system"
                }
            },
            "required": ["service", "interface"]
        })
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let service = parse_required_str(&input, "service")?;
        let interface = parse_required_str(&input, "interface")?;
        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("/");
        let bus = parse_bus(&input, "bus");
        let info = self.introspection.introspect(bus, &service, path).await?;
        let iface = find_interface(&info, &interface)?;
        let properties: Vec<String> = iface.properties.iter().map(|p| p.name.clone()).collect();

        Ok(json!({
            "bus": bus_str(bus),
            "service": service,
            "path": path,
            "interface": interface,
            "properties": properties
        }))
    }

    fn category(&self) -> &str {
        "dbus"
    }
}

struct DbusListSignalsTool {
    introspection: Arc<IntrospectionService>,
}

impl DbusListSignalsTool {
    fn new(introspection: Arc<IntrospectionService>) -> Self {
        Self { introspection }
    }
}

#[async_trait]
impl Tool for DbusListSignalsTool {
    fn name(&self) -> &str {
        "dbus_list_signals"
    }

    fn description(&self) -> &str {
        "List signals for a D-Bus interface"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "service": { "type": "string" },
                "path": { "type": "string", "default": "/" },
                "interface": { "type": "string" },
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system"
                }
            },
            "required": ["service", "interface"]
        })
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let service = parse_required_str(&input, "service")?;
        let interface = parse_required_str(&input, "interface")?;
        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("/");
        let bus = parse_bus(&input, "bus");
        let info = self.introspection.introspect(bus, &service, path).await?;
        let iface = find_interface(&info, &interface)?;
        let signals: Vec<String> = iface.signals.iter().map(|s| s.name.clone()).collect();

        Ok(json!({
            "bus": bus_str(bus),
            "service": service,
            "path": path,
            "interface": interface,
            "signals": signals
        }))
    }

    fn category(&self) -> &str {
        "dbus"
    }
}

struct DbusCallMethodTool;

#[async_trait]
impl Tool for DbusCallMethodTool {
    fn name(&self) -> &str {
        "dbus_call_method"
    }

    fn description(&self) -> &str {
        "Call a D-Bus method with arguments"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "service": { "type": "string" },
                "path": { "type": "string" },
                "interface": { "type": "string" },
                "method": { "type": "string" },
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
        })
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let service = parse_required_str(&input, "service")?;
        let path = parse_required_str(&input, "path")?;
        let interface = parse_required_str(&input, "interface")?;
        let method = parse_required_str(&input, "method")?;
        let bus = parse_bus(&input, "bus");
        let args = input.get("args").and_then(|v| v.as_array()).cloned().unwrap_or_default();

        let connection = match bus {
            BusType::System => Connection::system().await?,
            BusType::Session => Connection::session().await?,
        };

        let proxy = zbus::Proxy::new(
            &connection,
            service.as_str(),
            path.as_str(),
            interface.as_str(),
        )
        .await?;
        let zbus_args: Vec<zbus::zvariant::OwnedValue> = args
            .iter()
            .map(json_to_owned_value)
            .collect::<Result<Vec<_>>>()?;

        let result: zbus::zvariant::OwnedValue =
            proxy.call(method.as_str(), &zbus_args).await?;
        let result_json = serde_json::to_value(&result)?;

        Ok(json!({
            "bus": bus_str(bus),
            "service": service,
            "path": path,
            "interface": interface,
            "method": method,
            "result": result_json
        }))
    }

    fn category(&self) -> &str {
        "dbus"
    }
}

struct DbusGetPropertyTool;

#[async_trait]
impl Tool for DbusGetPropertyTool {
    fn name(&self) -> &str {
        "dbus_get_property"
    }

    fn description(&self) -> &str {
        "Get the value of a D-Bus property"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "service": { "type": "string" },
                "path": { "type": "string" },
                "interface": { "type": "string" },
                "property": { "type": "string" },
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system"
                }
            },
            "required": ["service", "path", "interface", "property"]
        })
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let service = parse_required_str(&input, "service")?;
        let path = parse_required_str(&input, "path")?;
        let interface = parse_required_str(&input, "interface")?;
        let property = parse_required_str(&input, "property")?;
        let bus = parse_bus(&input, "bus");

        let connection = match bus {
            BusType::System => Connection::system().await?,
            BusType::Session => Connection::session().await?,
        };

        let interface_name = zbus::names::InterfaceName::try_from(interface.as_str())?;
        let properties_proxy = zbus::fdo::PropertiesProxy::builder(&connection)
            .destination(service.as_str())?
            .path(path.as_str())?
            .build()
            .await?;

        let value: zbus::zvariant::OwnedValue =
            properties_proxy.get(interface_name, property.as_str()).await?;
        let value_json = serde_json::to_value(&value)?;

        Ok(json!({
            "bus": bus_str(bus),
            "service": service,
            "path": path,
            "interface": interface,
            "property": property,
            "value": value_json
        }))
    }

    fn category(&self) -> &str {
        "dbus"
    }
}

struct DbusSetPropertyTool;

#[async_trait]
impl Tool for DbusSetPropertyTool {
    fn name(&self) -> &str {
        "dbus_set_property"
    }

    fn description(&self) -> &str {
        "Set the value of a D-Bus property"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "service": { "type": "string" },
                "path": { "type": "string" },
                "interface": { "type": "string" },
                "property": { "type": "string" },
                "value": { "description": "Property value (as JSON)" },
                "bus": {
                    "type": "string",
                    "enum": ["system", "session"],
                    "default": "system"
                }
            },
            "required": ["service", "path", "interface", "property", "value"]
        })
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let service = parse_required_str(&input, "service")?;
        let path = parse_required_str(&input, "path")?;
        let interface = parse_required_str(&input, "interface")?;
        let property = parse_required_str(&input, "property")?;
        let value = input
            .get("value")
            .ok_or_else(|| anyhow!("Missing required parameter: value"))?;
        let bus = parse_bus(&input, "bus");

        let connection = match bus {
            BusType::System => Connection::system().await?,
            BusType::Session => Connection::session().await?,
        };

        let interface_name = zbus::names::InterfaceName::try_from(interface.as_str())?;
        let properties_proxy = zbus::fdo::PropertiesProxy::builder(&connection)
            .destination(service.as_str())?
            .path(path.as_str())?
            .build()
            .await?;

        let zbus_value = json_to_owned_value(value)?;
        properties_proxy
            .set(interface_name, property.as_str(), zbus::zvariant::Value::from(zbus_value))
            .await?;

        Ok(json!({
            "bus": bus_str(bus),
            "success": true,
            "service": service,
            "path": path,
            "interface": interface,
            "property": property
        }))
    }

    fn category(&self) -> &str {
        "dbus"
    }
}

struct DbusGetAllPropertiesTool {
    introspection: Arc<IntrospectionService>,
}

impl DbusGetAllPropertiesTool {
    fn new(introspection: Arc<IntrospectionService>) -> Self {
        Self { introspection }
    }
}

#[async_trait]
impl Tool for DbusGetAllPropertiesTool {
    fn name(&self) -> &str {
        "dbus_get_all_properties"
    }

    fn description(&self) -> &str {
        "Get all properties of a D-Bus object (optionally filter by interface)"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "service": { "type": "string" },
                "path": { "type": "string" },
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
        })
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        let service = parse_required_str(&input, "service")?;
        let path = parse_required_str(&input, "path")?;
        let interface_filter = input.get("interface").and_then(|v| v.as_str());
        let bus = parse_bus(&input, "bus");

        let connection = match bus {
            BusType::System => Connection::system().await?,
            BusType::Session => Connection::session().await?,
        };

        let info = self.introspection.introspect(bus, &service, &path).await?;
        let properties_proxy = zbus::fdo::PropertiesProxy::builder(&connection)
            .destination(service.as_str())?
            .path(path.as_str())?
            .build()
            .await?;

        let mut all_properties = json!({});
        for iface in info.interfaces {
            if let Some(filter) = interface_filter {
                if iface.name != filter {
                    continue;
                }
            }

            let interface_name = zbus::names::InterfaceName::try_from(iface.name.as_str())?;
            let props: HashMap<String, zbus::zvariant::OwnedValue> =
                properties_proxy.get_all(interface_name).await.unwrap_or_default();

            let mut iface_props = json!({});
            for (prop_name, prop_value) in props {
                let value_json = serde_json::to_value(&prop_value)?;
                iface_props[prop_name] = value_json;
            }
            all_properties[iface.name] = iface_props;
        }

        Ok(json!({
            "bus": bus_str(bus),
            "service": service,
            "path": path,
            "properties": all_properties
        }))
    }

    fn category(&self) -> &str {
        "dbus"
    }
}
