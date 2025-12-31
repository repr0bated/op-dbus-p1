// JSON Introspection Parser
// For custom services that use JSON instead of D-Bus XML

use serde::{Deserialize, Serialize};

/// JSON introspection format (your custom services)
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonIntrospection {
    pub service: String,
    pub version: String,
    pub description: Option<String>,
    pub methods: Vec<JsonMethod>,
    #[serde(default)]
    pub properties: Vec<JsonProperty>,
    #[serde(default)]
    pub signals: Vec<JsonSignal>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonMethod {
    pub name: String,
    pub description: Option<String>,
    pub inputs: Vec<JsonArg>,
    pub outputs: Vec<JsonArg>,
    #[serde(default)]
    pub async_: bool, // For long-running methods
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonArg {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String, // "string", "number", "boolean", "object", "array"
    pub description: Option<String>,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonProperty {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub access: String, // "read", "write", "readwrite"
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonSignal {
    pub name: String,
    pub description: Option<String>,
    pub args: Vec<JsonArg>,
}

/// Parser for JSON introspection
pub struct JsonIntrospectionParser;

impl JsonIntrospectionParser {
    /// Parse JSON introspection data
    pub fn parse(json: &str) -> Result<JsonIntrospection, Box<dyn std::error::Error>> {
        let data: JsonIntrospection = serde_json::from_str(json)?;
        Ok(data)
    }

    /// Fetch JSON introspection from HTTP endpoint
    pub async fn fetch_http(url: &str) -> Result<JsonIntrospection, Box<dyn std::error::Error>> {
        // Would use reqwest in production
        // For now, return error
        Err("HTTP fetching not yet implemented".into())
    }

    /// Load JSON introspection from file
    pub fn load_file(path: &str) -> Result<JsonIntrospection, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        Self::parse(&content)
    }

    /// Convert to MCP tool schema
    pub fn to_mcp_tools(&self, data: &JsonIntrospection) -> Vec<serde_json::Value> {
        use serde_json::json;

        data.methods
            .iter()
            .map(|method| {
                let mut properties = serde_json::Map::new();
                let mut required = Vec::new();

                for input in &method.inputs {
                    properties.insert(
                        input.name.clone(),
                        json!({
                            "type": input.type_,
                            "description": input.description.as_ref().unwrap_or(&input.name)
                        }),
                    );
                    if !input.optional {
                        required.push(input.name.clone());
                    }
                }

                json!({
                    "name": Self::method_name_to_snake_case(&method.name),
                    "description": method.description.as_ref().unwrap_or(&method.name),
                    "inputSchema": {
                        "type": "object",
                        "properties": properties,
                        "required": required
                    }
                })
            })
            .collect()
    }

    fn method_name_to_snake_case(name: &str) -> String {
        let mut result = String::new();
        for (i, c) in name.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        }
        result
    }
}

// Example JSON introspection file
#[cfg(test)]
mod example {
    use super::*;

    pub const EXAMPLE_JSON: &str = r#"
{
  "service": "com.example.MyService",
  "version": "1.0.0",
  "description": "My custom service with JSON introspection",
  "methods": [
    {
      "name": "GetUserInfo",
      "description": "Retrieve user information",
      "inputs": [
        {
          "name": "user_id",
          "type": "string",
          "description": "User ID to query"
        }
      ],
      "outputs": [
        {
          "name": "result",
          "type": "object",
          "description": "User information"
        }
      ]
    },
    {
      "name": "UpdateSettings",
      "description": "Update service settings",
      "async": true,
      "inputs": [
        {
          "name": "key",
          "type": "string",
          "description": "Setting key"
        },
        {
          "name": "value",
          "type": "string",
          "description": "Setting value"
        },
        {
          "name": "persist",
          "type": "boolean",
          "description": "Save to disk",
          "optional": true
        }
      ],
      "outputs": [
        {
          "name": "success",
          "type": "boolean"
        }
      ]
    }
  ],
  "properties": [
    {
      "name": "Status",
      "type": "string",
      "access": "read",
      "description": "Current service status"
    },
    {
      "name": "LogLevel",
      "type": "string",
      "access": "readwrite",
      "description": "Logging level (debug, info, warn, error)"
    }
  ],
  "signals": [
    {
      "name": "StatusChanged",
      "description": "Emitted when status changes",
      "args": [
        {
          "name": "old_status",
          "type": "string"
        },
        {
          "name": "new_status",
          "type": "string"
        }
      ]
    }
  ]
}
"#;

    #[test]
    fn test_parse_example() {
        let data = JsonIntrospectionParser::parse(EXAMPLE_JSON).unwrap();

        assert_eq!(data.service, "com.example.MyService");
        assert_eq!(data.methods.len(), 2);
        assert_eq!(data.properties.len(), 2);
        assert_eq!(data.signals.len(), 1);

        // Test MCP tool generation
        let parser = JsonIntrospectionParser;
        let tools = parser.to_mcp_tools(&data);
        assert_eq!(tools.len(), 2);

        println!("Generated MCP tools:");
        for tool in &tools {
            println!("{}", serde_json::to_string_pretty(tool).unwrap());
        }
    }
}
