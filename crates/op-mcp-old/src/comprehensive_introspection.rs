use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zbus::{Connection, Proxy};
use zbus::zvariant::OwnedValue;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComprehensiveIntrospection {
    pub system_services: Vec<ServiceInfo>,
    pub session_services: Vec<ServiceInfo>,
    pub total_objects: usize,
    pub total_interfaces: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub objects: Vec<ObjectInfo>,
    pub discovery_method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectInfo {
    pub path: String,
    pub interfaces: Vec<String>,
    pub introspectable: bool,
}

pub struct ComprehensiveIntrospector {
    system_conn: Connection,
    session_conn: Option<Connection>,
}

impl ComprehensiveIntrospector {
    pub async fn new() -> Result<Self> {
        let system_conn = Connection::system().await?;
        let session_conn = Connection::session().await.ok();
        
        Ok(Self { system_conn, session_conn })
    }

    pub async fn introspect_all(&self) -> Result<ComprehensiveIntrospection> {
        let mut system_services = Vec::new();
        let mut session_services = Vec::new();

        // System bus
        let system_names = self.list_services(&self.system_conn).await?;
        for name in system_names {
            if let Ok(service) = self.introspect_service(&self.system_conn, &name).await {
                system_services.push(service);
            }
        }

        // Session bus
        if let Some(ref conn) = self.session_conn {
            let session_names = self.list_services(conn).await?;
            for name in session_names {
                if let Ok(service) = self.introspect_service(conn, &name).await {
                    session_services.push(service);
                }
            }
        }

        let total_objects = system_services.iter().map(|s| s.objects.len()).sum::<usize>()
            + session_services.iter().map(|s| s.objects.len()).sum::<usize>();
        
        let total_interfaces = system_services.iter()
            .flat_map(|s| &s.objects)
            .map(|o| o.interfaces.len())
            .sum::<usize>()
            + session_services.iter()
                .flat_map(|s| &s.objects)
                .map(|o| o.interfaces.len())
                .sum::<usize>();

        Ok(ComprehensiveIntrospection {
            system_services,
            session_services,
            total_objects,
            total_interfaces,
        })
    }

    async fn list_services(&self, conn: &Connection) -> Result<Vec<String>> {
        let proxy = zbus::fdo::DBusProxy::new(conn).await?;
        let names = proxy.list_names().await?;
        
        Ok(names.into_iter()
            .filter(|name| !name.starts_with(':') && name.contains('.'))
            .map(|name| name.to_string())
            .collect())
    }

    async fn introspect_service(&self, conn: &Connection, service_name: &str) -> Result<ServiceInfo> {
        let mut objects = Vec::new();
        let mut discovery_method = "introspection".to_string();

        // Try ObjectManager first (most efficient)
        if let Ok(managed_objects) = self.get_managed_objects(conn, service_name).await {
            discovery_method = "ObjectManager".to_string();
            for (path, interfaces) in managed_objects {
                objects.push(ObjectInfo {
                    path: path.to_string(),
                    interfaces: interfaces.keys().map(|k| k.to_string()).collect(),
                    introspectable: true,
                });
            }
        } else {
            // Fallback to recursive introspection
            objects = self.discover_by_introspection(conn, service_name).await?;
        }

        Ok(ServiceInfo {
            name: service_name.to_string(),
            objects,
            discovery_method,
        })
    }

    async fn get_managed_objects(&self, conn: &Connection, service_name: &str) -> Result<HashMap<String, HashMap<String, OwnedValue>>> {
        // Try common ObjectManager paths
        let path1 = format!("/{}", service_name.replace('.', "/"));
        let path2 = format!("/{}", service_name.replace('.', "/").to_lowercase());
        let paths = vec![
            "/",
            &path1,
            &path2,
        ];

        for path in paths {
            if let Ok(proxy) = Proxy::new(conn, service_name, path, "org.freedesktop.DBus.ObjectManager").await {
                if let Ok(result) = proxy.call("GetManagedObjects", &()).await {
                    return Ok(result);
                }
            }
        }

        Err(anyhow::anyhow!("No ObjectManager found"))
    }

    async fn discover_by_introspection(&self, conn: &Connection, service_name: &str) -> Result<Vec<ObjectInfo>> {
        let mut objects = Vec::new();
        let mut visited = std::collections::HashSet::new();

        // Try multiple starting points
        let default_path = format!("/{}", service_name.replace('.', "/"));
        let start_paths = vec![
            "/",
            &default_path,
        ];

        for start_path in start_paths {
            self.introspect_recursive(conn, service_name, start_path, &mut objects, &mut visited).await;
        }

        Ok(objects)
    }

    async fn introspect_recursive(
        &self,
        conn: &Connection,
        service_name: &str,
        path: &str,
        objects: &mut Vec<ObjectInfo>,
        visited: &mut std::collections::HashSet<String>,
    ) {
        if visited.contains(path) || visited.len() > 1000 {
            return;
        }
        visited.insert(path.to_string());

        match self.introspect_path(conn, service_name, path).await {
            Ok((interfaces, children)) => {
                if !interfaces.is_empty() {
                    objects.push(ObjectInfo {
                        path: path.to_string(),
                        interfaces,
                        introspectable: true,
                    });
                }

                // Recurse into children
                for child in children {
                    let child_path = if path == "/" {
                        format!("/{}", child)
                    } else {
                        format!("{}/{}", path, child)
                    };
                    
                    Box::pin(self.introspect_recursive(conn, service_name, &child_path, objects, visited)).await;
                }
            }
            Err(_) => {
                // Non-introspectable object - still record it
                objects.push(ObjectInfo {
                    path: path.to_string(),
                    interfaces: vec![],
                    introspectable: false,
                });
            }
        }
    }

    async fn introspect_path(&self, conn: &Connection, service_name: &str, path: &str) -> Result<(Vec<String>, Vec<String>)> {
        let proxy = Proxy::new(conn, service_name, path, "org.freedesktop.DBus.Introspectable").await?;
        let xml: String = proxy.call("Introspect", &()).await?;

        let interfaces = self.extract_interfaces(&xml);
        let children = self.extract_children(&xml);

        Ok((interfaces, children))
    }

    fn extract_interfaces(&self, xml: &str) -> Vec<String> {
        let mut interfaces = Vec::new();
        for line in xml.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("<interface name=\"") {
                if let Some(name) = self.extract_xml_attr(trimmed, "name") {
                    interfaces.push(name);
                }
            }
        }
        interfaces
    }

    fn extract_children(&self, xml: &str) -> Vec<String> {
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

    pub async fn get_object_xml(&self, service_name: &str, object_path: &str) -> Result<String> {
        let proxy = Proxy::new(&self.system_conn, service_name, object_path, "org.freedesktop.DBus.Introspectable").await?;
        let xml: String = proxy.call("Introspect", &()).await?;
        Ok(xml)
    }

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
}
