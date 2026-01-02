//! Plugin Schema Registry
//!
//! Provides schema definitions for all state plugins, enabling:
//! - Validation of plugin state against schemas
//! - Schema versioning and migration
//! - Documentation of plugin state structure
//! - Auto-generation of state templates

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Schema field type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    String,
    Integer,
    Float,
    Boolean,
    Array(Box<FieldType>),
    Object(HashMap<String, FieldSchema>),
    Enum(Vec<String>),
    Any,
}

/// Schema for a single field
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FieldSchema {
    /// Field type
    pub field_type: FieldType,
    /// Whether the field is required
    #[serde(default)]
    pub required: bool,
    /// Description of the field
    #[serde(default)]
    pub description: String,
    /// Default value if not provided
    #[serde(default)]
    pub default: Option<Value>,
    /// Example value for documentation
    #[serde(default)]
    pub example: Option<Value>,
    /// Validation constraints
    #[serde(default)]
    pub constraints: Vec<Constraint>,
}

/// Validation constraint
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Constraint {
    /// Minimum value (for numbers) or length (for strings/arrays)
    Min { value: f64 },
    /// Maximum value (for numbers) or length (for strings/arrays)
    Max { value: f64 },
    /// Regex pattern (for strings)
    Pattern { regex: String },
    /// Value must be one of these
    OneOf { values: Vec<Value> },
    /// Reference to another field that must exist
    RequiresField { field: String },
    /// Custom validation function name
    Custom { validator: String },
}

/// Plugin schema definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSchema {
    /// Plugin name
    pub name: String,
    /// Schema version
    pub version: String,
    /// Description
    pub description: String,
    /// Fields in the plugin state
    pub fields: HashMap<String, FieldSchema>,
    /// Dependencies on other plugins
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Example state for documentation
    #[serde(default)]
    pub example: Option<Value>,
}

impl PluginSchema {
    /// Create a new plugin schema builder
    pub fn builder(name: &str) -> PluginSchemaBuilder {
        PluginSchemaBuilder::new(name)
    }

    /// Validate a state value against this schema
    pub fn validate(&self, state: &Value) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Check required fields
        for (field_name, field_schema) in &self.fields {
            if field_schema.required {
                if state.get(field_name).is_none() {
                    errors.push(format!("Missing required field: {}", field_name));
                }
            }
        }

        // Validate present fields
        if let Some(obj) = state.as_object() {
            for (field_name, field_value) in obj {
                if let Some(field_schema) = self.fields.get(field_name) {
                    if let Err(e) = validate_field(field_name, field_value, field_schema) {
                        errors.push(e);
                    }
                } else {
                    warnings.push(format!("Unknown field: {}", field_name));
                }
            }
        }

        ValidationResult {
            valid: errors.is_empty(),
            errors,
            warnings,
        }
    }

    /// Generate a template state with default values
    pub fn generate_template(&self) -> Value {
        let mut template = serde_json::Map::new();

        for (field_name, field_schema) in &self.fields {
            let value = if let Some(default) = &field_schema.default {
                default.clone()
            } else if let Some(example) = &field_schema.example {
                example.clone()
            } else {
                default_for_type(&field_schema.field_type)
            };
            template.insert(field_name.clone(), value);
        }

        Value::Object(template)
    }

    /// Convert to JSON Schema format
    pub fn to_json_schema(&self) -> Value {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for (field_name, field_schema) in &self.fields {
            properties.insert(field_name.clone(), field_type_to_json_schema(&field_schema.field_type));
            if field_schema.required {
                required.push(Value::String(field_name.clone()));
            }
        }

        json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "title": self.name,
            "description": self.description,
            "type": "object",
            "properties": properties,
            "required": required
        })
    }
}

/// Validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Builder for creating plugin schemas
pub struct PluginSchemaBuilder {
    name: String,
    version: String,
    description: String,
    fields: HashMap<String, FieldSchema>,
    dependencies: Vec<String>,
    example: Option<Value>,
}

impl PluginSchemaBuilder {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            description: String::new(),
            fields: HashMap::new(),
            dependencies: Vec::new(),
            example: None,
        }
    }

    pub fn version(mut self, version: &str) -> Self {
        self.version = version.to_string();
        self
    }

    pub fn description(mut self, description: &str) -> Self {
        self.description = description.to_string();
        self
    }

    pub fn field(mut self, name: &str, schema: FieldSchema) -> Self {
        self.fields.insert(name.to_string(), schema);
        self
    }

    pub fn string_field(self, name: &str, required: bool, description: &str) -> Self {
        self.field(
            name,
            FieldSchema {
                field_type: FieldType::String,
                required,
                description: description.to_string(),
                default: None,
                example: None,
                constraints: Vec::new(),
            },
        )
    }

    pub fn integer_field(self, name: &str, required: bool, description: &str) -> Self {
        self.field(
            name,
            FieldSchema {
                field_type: FieldType::Integer,
                required,
                description: description.to_string(),
                default: None,
                example: None,
                constraints: Vec::new(),
            },
        )
    }

    pub fn boolean_field(self, name: &str, required: bool, description: &str) -> Self {
        self.field(
            name,
            FieldSchema {
                field_type: FieldType::Boolean,
                required,
                description: description.to_string(),
                default: None,
                example: None,
                constraints: Vec::new(),
            },
        )
    }

    pub fn array_field(self, name: &str, item_type: FieldType, required: bool, description: &str) -> Self {
        self.field(
            name,
            FieldSchema {
                field_type: FieldType::Array(Box::new(item_type)),
                required,
                description: description.to_string(),
                default: None,
                example: None,
                constraints: Vec::new(),
            },
        )
    }

    pub fn object_field(self, name: &str, fields: HashMap<String, FieldSchema>, required: bool, description: &str) -> Self {
        self.field(
            name,
            FieldSchema {
                field_type: FieldType::Object(fields),
                required,
                description: description.to_string(),
                default: None,
                example: None,
                constraints: Vec::new(),
            },
        )
    }

    pub fn dependency(mut self, plugin_name: &str) -> Self {
        self.dependencies.push(plugin_name.to_string());
        self
    }

    pub fn example(mut self, example: Value) -> Self {
        self.example = Some(example);
        self
    }

    pub fn build(self) -> PluginSchema {
        PluginSchema {
            name: self.name,
            version: self.version,
            description: self.description,
            fields: self.fields,
            dependencies: self.dependencies,
            example: self.example,
        }
    }
}

/// Registry of all plugin schemas
pub struct SchemaRegistry {
    schemas: HashMap<String, PluginSchema>,
}

impl SchemaRegistry {
    /// Create a new schema registry with built-in schemas
    pub fn new() -> Self {
        let mut registry = Self {
            schemas: HashMap::new(),
        };
        registry.register_builtin_schemas();
        registry
    }

    /// Register a plugin schema
    pub fn register(&mut self, schema: PluginSchema) {
        self.schemas.insert(schema.name.clone(), schema);
    }

    /// Get a plugin schema by name
    pub fn get(&self, name: &str) -> Option<&PluginSchema> {
        self.schemas.get(name)
    }

    /// List all registered schema names
    pub fn list(&self) -> Vec<&str> {
        self.schemas.keys().map(|s| s.as_str()).collect()
    }

    /// Validate state for a plugin
    pub fn validate(&self, plugin_name: &str, state: &Value) -> Option<ValidationResult> {
        self.schemas.get(plugin_name).map(|schema| schema.validate(state))
    }

    /// Register all built-in plugin schemas
    fn register_builtin_schemas(&mut self) {
        // LXC Container Schema
        self.register(create_lxc_schema());

        // Network Schema
        self.register(create_net_schema());

        // OpenFlow Schema
        self.register(create_openflow_schema());

        // Systemd Schema
        self.register(create_systemd_schema());

        // Privacy Router Schema
        self.register(create_privacy_router_schema());

        // Netmaker Schema
        self.register(create_netmaker_schema());
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Built-in Schema Definitions
// ============================================================================

fn create_lxc_schema() -> PluginSchema {
    let container_fields = {
        let mut fields = HashMap::new();
        fields.insert("id".to_string(), FieldSchema {
            field_type: FieldType::String,
            required: true,
            description: "Container VMID".to_string(),
            default: None,
            example: Some(json!("100")),
            constraints: vec![Constraint::Pattern { regex: r"^\d+$".to_string() }],
        });
        fields.insert("veth".to_string(), FieldSchema {
            field_type: FieldType::String,
            required: false,
            description: "Veth interface name".to_string(),
            default: None,
            example: Some(json!("vi100")),
            constraints: Vec::new(),
        });
        fields.insert("bridge".to_string(), FieldSchema {
            field_type: FieldType::String,
            required: false,
            description: "OVS bridge name".to_string(),
            default: Some(json!("ovs-br0")),
            example: Some(json!("ovs-br0")),
            constraints: Vec::new(),
        });
        fields.insert("running".to_string(), FieldSchema {
            field_type: FieldType::Boolean,
            required: false,
            description: "Whether container is running".to_string(),
            default: Some(json!(false)),
            example: Some(json!(true)),
            constraints: Vec::new(),
        });
        fields.insert("properties".to_string(), FieldSchema {
            field_type: FieldType::Any,
            required: false,
            description: "Container properties (hostname, memory, cores, etc.)".to_string(),
            default: Some(json!({})),
            example: Some(json!({
                "hostname": "my-container",
                "memory": 512,
                "cores": 2,
                "template": "local:vztmpl/debian-13.tar.zst"
            })),
            constraints: Vec::new(),
        });
        fields
    };

    PluginSchema::builder("lxc")
        .version("2.0.0")
        .description("LXC container management via native Proxmox API")
        .array_field("containers", FieldType::Object(container_fields), true, "List of containers")
        .example(json!({
            "containers": [
                {
                    "id": "100",
                    "veth": "vi100",
                    "bridge": "ovs-br0",
                    "running": true,
                    "properties": {
                        "hostname": "wireguard-gateway",
                        "memory": 512,
                        "cores": 1,
                        "network_type": "bridge"
                    }
                }
            ]
        }))
        .build()
}

fn create_net_schema() -> PluginSchema {
    let interface_fields = {
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), FieldSchema {
            field_type: FieldType::String,
            required: true,
            description: "Interface name".to_string(),
            default: None,
            example: Some(json!("eth0")),
            constraints: Vec::new(),
        });
        fields.insert("type".to_string(), FieldSchema {
            field_type: FieldType::Enum(vec![
                "ethernet".to_string(),
                "bridge".to_string(),
                "veth".to_string(),
                "vlan".to_string(),
                "bond".to_string(),
            ]),
            required: true,
            description: "Interface type".to_string(),
            default: Some(json!("ethernet")),
            example: Some(json!("ethernet")),
            constraints: Vec::new(),
        });
        fields.insert("state".to_string(), FieldSchema {
            field_type: FieldType::Enum(vec!["up".to_string(), "down".to_string()]),
            required: false,
            description: "Interface state".to_string(),
            default: Some(json!("up")),
            example: Some(json!("up")),
            constraints: Vec::new(),
        });
        fields.insert("addresses".to_string(), FieldSchema {
            field_type: FieldType::Array(Box::new(FieldType::String)),
            required: false,
            description: "IP addresses".to_string(),
            default: Some(json!([])),
            example: Some(json!(["192.168.1.100/24"])),
            constraints: Vec::new(),
        });
        fields
    };

    PluginSchema::builder("net")
        .version("1.0.0")
        .description("Network interface management via rtnetlink")
        .array_field("interfaces", FieldType::Object(interface_fields), true, "List of network interfaces")
        .example(json!({
            "interfaces": [
                {
                    "name": "eth0",
                    "type": "ethernet",
                    "state": "up",
                    "addresses": ["192.168.1.100/24"]
                }
            ]
        }))
        .build()
}

fn create_openflow_schema() -> PluginSchema {
    let bridge_fields = {
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), FieldSchema {
            field_type: FieldType::String,
            required: true,
            description: "Bridge name".to_string(),
            default: None,
            example: Some(json!("ovs-br0")),
            constraints: Vec::new(),
        });
        fields.insert("datapath_id".to_string(), FieldSchema {
            field_type: FieldType::String,
            required: false,
            description: "Datapath ID".to_string(),
            default: None,
            example: Some(json!("0000000000000001")),
            constraints: Vec::new(),
        });
        fields.insert("protocols".to_string(), FieldSchema {
            field_type: FieldType::Array(Box::new(FieldType::String)),
            required: false,
            description: "Supported OpenFlow protocols".to_string(),
            default: Some(json!(["OpenFlow13"])),
            example: Some(json!(["OpenFlow10", "OpenFlow13"])),
            constraints: Vec::new(),
        });
        fields
    };

    let flow_fields = {
        let mut fields = HashMap::new();
        fields.insert("priority".to_string(), FieldSchema {
            field_type: FieldType::Integer,
            required: true,
            description: "Flow priority (higher = more specific)".to_string(),
            default: Some(json!(100)),
            example: Some(json!(100)),
            constraints: vec![Constraint::Min { value: 0.0 }, Constraint::Max { value: 65535.0 }],
        });
        fields.insert("match".to_string(), FieldSchema {
            field_type: FieldType::Any,
            required: true,
            description: "Flow match criteria".to_string(),
            default: None,
            example: Some(json!({"in_port": "vi100"})),
            constraints: Vec::new(),
        });
        fields.insert("actions".to_string(), FieldSchema {
            field_type: FieldType::Array(Box::new(FieldType::Any)),
            required: true,
            description: "Actions to perform".to_string(),
            default: None,
            example: Some(json!([{"type": "output", "port": "vi101"}])),
            constraints: Vec::new(),
        });
        fields
    };

    PluginSchema::builder("openflow")
        .version("1.0.0")
        .description("OpenFlow flow table management")
        .dependency("net")
        .array_field("bridges", FieldType::Object(bridge_fields), true, "OVS bridges")
        .array_field("flows", FieldType::Object(flow_fields), false, "OpenFlow rules")
        .string_field("controller_endpoint", false, "OpenFlow controller endpoint")
        .boolean_field("auto_discover_containers", false, "Auto-create flows for containers")
        .example(json!({
            "bridges": [
                {
                    "name": "ovs-br0",
                    "protocols": ["OpenFlow13"]
                }
            ],
            "flows": [
                {
                    "priority": 100,
                    "match": {"in_port": "vi100"},
                    "actions": [{"type": "output", "port": "vi101"}]
                }
            ],
            "auto_discover_containers": true
        }))
        .build()
}

fn create_systemd_schema() -> PluginSchema {
    let unit_fields = {
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), FieldSchema {
            field_type: FieldType::String,
            required: true,
            description: "Unit name".to_string(),
            default: None,
            example: Some(json!("nginx.service")),
            constraints: Vec::new(),
        });
        fields.insert("state".to_string(), FieldSchema {
            field_type: FieldType::Enum(vec![
                "active".to_string(),
                "inactive".to_string(),
                "failed".to_string(),
            ]),
            required: false,
            description: "Desired unit state".to_string(),
            default: Some(json!("active")),
            example: Some(json!("active")),
            constraints: Vec::new(),
        });
        fields.insert("enabled".to_string(), FieldSchema {
            field_type: FieldType::Boolean,
            required: false,
            description: "Whether unit is enabled at boot".to_string(),
            default: Some(json!(true)),
            example: Some(json!(true)),
            constraints: Vec::new(),
        });
        fields
    };

    PluginSchema::builder("systemd")
        .version("1.0.0")
        .description("Systemd unit management via D-Bus")
        .array_field("units", FieldType::Object(unit_fields), true, "Systemd units")
        .example(json!({
            "units": [
                {
                    "name": "nginx.service",
                    "state": "active",
                    "enabled": true
                }
            ]
        }))
        .build()
}

fn create_privacy_router_schema() -> PluginSchema {
    let wireguard_fields = {
        let mut fields = HashMap::new();
        fields.insert("enabled".to_string(), FieldSchema {
            field_type: FieldType::Boolean,
            required: true,
            description: "Enable WireGuard tunnel".to_string(),
            default: Some(json!(true)),
            example: Some(json!(true)),
            constraints: Vec::new(),
        });
        fields.insert("container_id".to_string(), FieldSchema {
            field_type: FieldType::String,
            required: false,
            description: "Container VMID for WireGuard".to_string(),
            default: Some(json!("100")),
            example: Some(json!("100")),
            constraints: Vec::new(),
        });
        fields.insert("listen_port".to_string(), FieldSchema {
            field_type: FieldType::Integer,
            required: false,
            description: "WireGuard listen port".to_string(),
            default: Some(json!(51820)),
            example: Some(json!(51820)),
            constraints: vec![Constraint::Min { value: 1.0 }, Constraint::Max { value: 65535.0 }],
        });
        fields
    };

    let warp_fields = {
        let mut fields = HashMap::new();
        fields.insert("enabled".to_string(), FieldSchema {
            field_type: FieldType::Boolean,
            required: true,
            description: "Enable Cloudflare WARP tunnel".to_string(),
            default: Some(json!(true)),
            example: Some(json!(true)),
            constraints: Vec::new(),
        });
        fields.insert("container_id".to_string(), FieldSchema {
            field_type: FieldType::String,
            required: false,
            description: "Container VMID for WARP".to_string(),
            default: Some(json!("101")),
            example: Some(json!("101")),
            constraints: Vec::new(),
        });
        fields
    };

    let xray_fields = {
        let mut fields = HashMap::new();
        fields.insert("enabled".to_string(), FieldSchema {
            field_type: FieldType::Boolean,
            required: true,
            description: "Enable XRay tunnel".to_string(),
            default: Some(json!(true)),
            example: Some(json!(true)),
            constraints: Vec::new(),
        });
        fields.insert("container_id".to_string(), FieldSchema {
            field_type: FieldType::String,
            required: false,
            description: "Container VMID for XRay".to_string(),
            default: Some(json!("102")),
            example: Some(json!("102")),
            constraints: Vec::new(),
        });
        fields.insert("protocol".to_string(), FieldSchema {
            field_type: FieldType::Enum(vec![
                "vless".to_string(),
                "vmess".to_string(),
                "trojan".to_string(),
            ]),
            required: false,
            description: "XRay protocol".to_string(),
            default: Some(json!("vless")),
            example: Some(json!("vless")),
            constraints: Vec::new(),
        });
        fields
    };

    PluginSchema::builder("privacy_router")
        .version("1.0.0")
        .description("Multi-hop privacy tunnel chain (WireGuard → WARP → XRay)")
        .dependency("lxc")
        .dependency("openflow")
        .string_field("bridge_name", true, "OVS bridge for privacy network")
        .object_field("wireguard", wireguard_fields, true, "WireGuard tunnel config")
        .object_field("warp", warp_fields, true, "Cloudflare WARP config")
        .object_field("xray", xray_fields, true, "XRay tunnel config")
        .example(json!({
            "bridge_name": "ovs-br0",
            "wireguard": {
                "enabled": true,
                "container_id": "100",
                "listen_port": 51820
            },
            "warp": {
                "enabled": true,
                "container_id": "101"
            },
            "xray": {
                "enabled": true,
                "container_id": "102",
                "protocol": "vless"
            }
        }))
        .build()
}

fn create_netmaker_schema() -> PluginSchema {
    PluginSchema::builder("netmaker")
        .version("1.0.0")
        .description("Netmaker mesh network management")
        .dependency("net")
        .string_field("network_name", true, "Netmaker network name")
        .string_field("interface", false, "WireGuard interface name (e.g., nm0)")
        .string_field("server_url", false, "Netmaker server URL")
        .string_field("enrollment_token", false, "Enrollment token for joining network")
        .boolean_field("auto_enroll", false, "Auto-enroll containers in mesh")
        .example(json!({
            "network_name": "container-mesh",
            "interface": "nm0",
            "auto_enroll": true
        }))
        .build()
}

// ============================================================================
// Helper Functions
// ============================================================================

fn validate_field(name: &str, value: &Value, schema: &FieldSchema) -> Result<(), String> {
    match &schema.field_type {
        FieldType::String => {
            if !value.is_string() {
                return Err(format!("Field '{}' must be a string", name));
            }
        }
        FieldType::Integer => {
            if !value.is_i64() && !value.is_u64() {
                return Err(format!("Field '{}' must be an integer", name));
            }
        }
        FieldType::Float => {
            if !value.is_f64() && !value.is_i64() {
                return Err(format!("Field '{}' must be a number", name));
            }
        }
        FieldType::Boolean => {
            if !value.is_boolean() {
                return Err(format!("Field '{}' must be a boolean", name));
            }
        }
        FieldType::Array(_) => {
            if !value.is_array() {
                return Err(format!("Field '{}' must be an array", name));
            }
        }
        FieldType::Object(_) => {
            if !value.is_object() {
                return Err(format!("Field '{}' must be an object", name));
            }
        }
        FieldType::Enum(valid_values) => {
            if let Some(s) = value.as_str() {
                if !valid_values.contains(&s.to_string()) {
                    return Err(format!(
                        "Field '{}' must be one of: {:?}",
                        name, valid_values
                    ));
                }
            } else {
                return Err(format!("Field '{}' must be a string enum value", name));
            }
        }
        FieldType::Any => {}
    }

    // Validate constraints
    for constraint in &schema.constraints {
        match constraint {
            Constraint::Min { value: min } => {
                if let Some(n) = value.as_f64() {
                    if n < *min {
                        return Err(format!("Field '{}' must be >= {}", name, min));
                    }
                }
                if let Some(s) = value.as_str() {
                    if (s.len() as f64) < *min {
                        return Err(format!("Field '{}' length must be >= {}", name, min));
                    }
                }
            }
            Constraint::Max { value: max } => {
                if let Some(n) = value.as_f64() {
                    if n > *max {
                        return Err(format!("Field '{}' must be <= {}", name, max));
                    }
                }
                if let Some(s) = value.as_str() {
                    if (s.len() as f64) > *max {
                        return Err(format!("Field '{}' length must be <= {}", name, max));
                    }
                }
            }
            Constraint::Pattern { regex } => {
                if let Some(s) = value.as_str() {
                    if let Ok(re) = regex::Regex::new(regex) {
                        if !re.is_match(s) {
                            return Err(format!(
                                "Field '{}' must match pattern: {}",
                                name, regex
                            ));
                        }
                    }
                }
            }
            Constraint::OneOf { values } => {
                if !values.contains(value) {
                    return Err(format!("Field '{}' must be one of: {:?}", name, values));
                }
            }
            _ => {}
        }
    }

    Ok(())
}

fn default_for_type(field_type: &FieldType) -> Value {
    match field_type {
        FieldType::String => json!(""),
        FieldType::Integer => json!(0),
        FieldType::Float => json!(0.0),
        FieldType::Boolean => json!(false),
        FieldType::Array(_) => json!([]),
        FieldType::Object(_) => json!({}),
        FieldType::Enum(values) => values.first().map(|s| json!(s)).unwrap_or(json!("")),
        FieldType::Any => json!(null),
    }
}

fn field_type_to_json_schema(field_type: &FieldType) -> Value {
    match field_type {
        FieldType::String => json!({"type": "string"}),
        FieldType::Integer => json!({"type": "integer"}),
        FieldType::Float => json!({"type": "number"}),
        FieldType::Boolean => json!({"type": "boolean"}),
        FieldType::Array(item_type) => json!({
            "type": "array",
            "items": field_type_to_json_schema(item_type)
        }),
        FieldType::Object(fields) => {
            let mut properties = serde_json::Map::new();
            for (name, schema) in fields {
                properties.insert(name.clone(), field_type_to_json_schema(&schema.field_type));
            }
            json!({
                "type": "object",
                "properties": properties
            })
        }
        FieldType::Enum(values) => json!({
            "type": "string",
            "enum": values
        }),
        FieldType::Any => json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_registry() {
        let registry = SchemaRegistry::new();
        assert!(registry.get("lxc").is_some());
        assert!(registry.get("net").is_some());
        assert!(registry.get("openflow").is_some());
        assert!(registry.get("systemd").is_some());
        assert!(registry.get("privacy_router").is_some());
        assert!(registry.get("netmaker").is_some());
    }

    #[test]
    fn test_lxc_validation() {
        let registry = SchemaRegistry::new();
        let schema = registry.get("lxc").unwrap();

        // Valid state
        let valid_state = json!({
            "containers": [
                {
                    "id": "100",
                    "veth": "vi100",
                    "bridge": "ovs-br0",
                    "running": true
                }
            ]
        });
        let result = schema.validate(&valid_state);
        assert!(result.valid, "Errors: {:?}", result.errors);

        // Missing required field
        let invalid_state = json!({});
        let result = schema.validate(&invalid_state);
        assert!(!result.valid);
    }

    #[test]
    fn test_template_generation() {
        let registry = SchemaRegistry::new();
        let schema = registry.get("lxc").unwrap();
        let template = schema.generate_template();
        assert!(template.get("containers").is_some());
    }

    #[test]
    fn test_json_schema_export() {
        let registry = SchemaRegistry::new();
        let schema = registry.get("lxc").unwrap();
        let json_schema = schema.to_json_schema();
        assert_eq!(json_schema["title"], "lxc");
        assert!(json_schema["properties"].is_object());
    }
}
