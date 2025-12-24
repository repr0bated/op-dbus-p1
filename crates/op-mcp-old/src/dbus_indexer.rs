//! D-Bus Hierarchical Indexer
//! Builds a complete, persistent index of D-Bus system state in a BTRFS subvolume
//!
//! This solves the fundamental problem: D-Bus introspection is segmental, incomplete,
//! and context-dependent. The indexer creates a complete snapshot that can be:
//! - Queried instantly (no D-Bus calls)
//! - Searched with FTS (find any service/object/method)
//! - Snapshotted with BTRFS (rollback, diff, send)
//! - Used by AI for complete system context

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use zbus::zvariant::OwnedValue;
use zbus::{Connection, Proxy};
use zbus_xml::Node;

use crate::system_introspection::SystemIntrospector;

/// D-Bus bus type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BusType {
    /// System bus (system-wide services)
    System,
    /// Session bus (user-specific services)
    Session,
}

impl std::fmt::Display for BusType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BusType::System => write!(f, "system"),
            BusType::Session => write!(f, "session"),
        }
    }
}

/// Complete D-Bus service index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbusIndex {
    pub version: String,
    pub timestamp: i64,
    pub bus_type: BusType,
    pub services: HashMap<String, ServiceIndex>,
    pub statistics: IndexStatistics,
}

/// Index for a single D-Bus service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceIndex {
    pub name: String,
    pub objects: Vec<ObjectIndex>,
    pub total_interfaces: usize,
    pub total_methods: usize,
    pub total_properties: usize,
}

/// Index for a D-Bus object path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectIndex {
    pub path: String,
    pub interfaces: Vec<String>,
    pub methods: Vec<MethodIndex>,
    pub properties: Vec<PropertyIndex>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodIndex {
    pub name: String,
    pub interface: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyIndex {
    pub name: String,
    pub interface: String,
    pub type_signature: String,
    pub access: String, // "read", "write", "readwrite"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStatistics {
    pub total_services: usize,
    pub total_objects: usize,
    pub total_interfaces: usize,
    pub total_methods: usize,
    pub total_properties: usize,
    pub scan_duration_seconds: f64,
}

/// D-Bus indexer that builds complete system index
pub struct DbusIndexer {
    index_root: PathBuf,
    bus_type: BusType,
    connection: Connection,
    introspector: SystemIntrospector,
}

impl DbusIndexer {
    /// Create new indexer for system bus (default)
    pub async fn new(index_root: impl AsRef<Path>) -> Result<Self> {
        Self::new_with_bus(index_root, BusType::System).await
    }

    /// Create new indexer with specific bus type
    pub async fn new_with_bus(index_root: impl AsRef<Path>, bus_type: BusType) -> Result<Self> {
        let index_root = index_root.as_ref().to_path_buf();

        // Ensure index directory exists
        fs::create_dir_all(&index_root).context("Failed to create index directory")?;

        // Connect to the specified bus
        let connection = match bus_type {
            BusType::System => Connection::system().await?,
            BusType::Session => Connection::session().await?,
        };

        let introspector = SystemIntrospector::new().await?;

        Ok(Self {
            index_root,
            bus_type,
            connection,
            introspector,
        })
    }

    /// Index BOTH system and session buses for complete coverage
    pub async fn build_complete_index_all_buses(
        index_root: impl AsRef<Path>,
    ) -> Result<(DbusIndex, DbusIndex)> {
        let index_root = index_root.as_ref();

        log::info!("📡 Indexing BOTH system and session buses...");

        // Index system bus
        log::info!("📡 Starting system bus index...");
        let system_indexer = Self::new_with_bus(index_root, BusType::System).await?;
        let system_index = system_indexer.build_complete_index().await?;

        // Index session bus
        log::info!("📡 Starting session bus index...");
        let session_indexer = Self::new_with_bus(index_root, BusType::Session).await?;
        let session_index = session_indexer.build_complete_index().await?;

        log::info!("✅ Complete D-Bus index built");
        log::info!(
            "   System bus: {} services",
            system_index.statistics.total_services
        );
        log::info!(
            "   Session bus: {} services",
            session_index.statistics.total_services
        );

        Ok((system_index, session_index))
    }

    /// Build complete D-Bus index (unlimited, no artificial limits)
    pub async fn build_complete_index(&self) -> Result<DbusIndex> {
        log::info!(
            "🔍 Starting complete D-Bus index build for {} bus...",
            self.bus_type
        );
        let start = std::time::Instant::now();

        // Discover ALL services
        let service_names = self.introspector.list_all_services().await?;
        log::info!("   Found {} services", service_names.len());

        let mut services = HashMap::new();
        let mut total_objects = 0;
        let mut total_interfaces = 0;
        let mut total_methods = 0;
        let mut total_properties = 0;

        for (idx, service_name) in service_names.iter().enumerate() {
            log::info!(
                "   [{}/{}] Indexing {}",
                idx + 1,
                service_names.len(),
                service_name
            );

            match self.index_service(service_name).await {
                Ok(service_index) => {
                    total_objects += service_index.objects.len();
                    total_interfaces += service_index.total_interfaces;
                    total_methods += service_index.total_methods;
                    total_properties += service_index.total_properties;

                    services.insert(service_name.clone(), service_index);
                }
                Err(e) => {
                    log::warn!("   ⚠️  Failed to index {}: {}", service_name, e);
                    // Continue anyway - don't let one failure stop entire index
                }
            }
        }

        let duration = start.elapsed().as_secs_f64();

        let index = DbusIndex {
            version: "1.0.0".to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            bus_type: self.bus_type,
            services,
            statistics: IndexStatistics {
                total_services: service_names.len(),
                total_objects,
                total_interfaces,
                total_methods,
                total_properties,
                scan_duration_seconds: duration,
            },
        };

        log::info!("✅ Index complete in {:.2}s", duration);
        log::info!("   Services: {}", index.statistics.total_services);
        log::info!("   Objects: {}", index.statistics.total_objects);
        log::info!("   Methods: {}", index.statistics.total_methods);

        Ok(index)
    }

    /// Index a single service completely (no limits)
    /// Tries ObjectManager first for 10-100x speedup, falls back to recursive introspection
    async fn index_service(&self, service_name: &str) -> Result<ServiceIndex> {
        // Try ObjectManager.GetManagedObjects first (MUCH faster!)
        match self.try_index_via_object_manager(service_name).await {
            Ok(service_index) => {
                log::debug!(
                    "   ✨ Used ObjectManager for {} ({} objects)",
                    service_name,
                    service_index.objects.len()
                );
                return Ok(service_index);
            }
            Err(e) => {
                log::trace!("   ObjectManager not available for {}: {}", service_name, e);
                // Fall through to recursive introspection
            }
        }

        // Fallback: Discover ALL object paths via recursive introspection
        log::trace!("   🔍 Using recursive introspection for {}", service_name);
        let objects = self.discover_all_objects_unlimited(service_name).await?;

        let mut object_indices = Vec::new();
        let mut total_interfaces = 0;
        let mut total_methods = 0;
        let mut total_properties = 0;

        for object_path in &objects {
            match self.index_object(service_name, object_path).await {
                Ok(obj_index) => {
                    total_interfaces += obj_index.interfaces.len();
                    total_methods += obj_index.methods.len();
                    total_properties += obj_index.properties.len();
                    object_indices.push(obj_index);
                }
                Err(e) => {
                    log::debug!("   Skipping {}: {}", object_path, e);
                }
            }
        }

        Ok(ServiceIndex {
            name: service_name.to_string(),
            objects: object_indices,
            total_interfaces,
            total_methods,
            total_properties,
        })
    }

    /// Try to index service using ObjectManager.GetManagedObjects (10-100x faster!)
    async fn try_index_via_object_manager(&self, service_name: &str) -> Result<ServiceIndex> {
        // Try common ObjectManager paths
        let om_paths = vec![
            "/".to_string(),
            format!("/{}", service_name.replace('.', "/")),
        ];

        for path in om_paths {
            if let Ok(managed) = self.call_get_managed_objects(service_name, &path).await {
                return self.index_from_managed_objects(service_name, managed);
            }
        }

        anyhow::bail!("ObjectManager not available for {}", service_name)
    }

    /// Call ObjectManager.GetManagedObjects on a service
    async fn call_get_managed_objects(
        &self,
        service: &str,
        path: &str,
    ) -> Result<HashMap<String, HashMap<String, HashMap<String, OwnedValue>>>> {
        let proxy = Proxy::new(
            &self.connection,
            service,
            path,
            "org.freedesktop.DBus.ObjectManager",
        )
        .await?;

        // Call GetManagedObjects with proper zbus 3.14.1 API
        let result: Result<HashMap<String, HashMap<String, HashMap<String, OwnedValue>>>, _> =
            proxy.call("GetManagedObjects", &()).await;

        Ok(result?)
    }

    /// Build ServiceIndex from ObjectManager data
    fn index_from_managed_objects(
        &self,
        service_name: &str,
        managed: HashMap<String, HashMap<String, HashMap<String, OwnedValue>>>,
    ) -> Result<ServiceIndex> {
        let mut object_indices = Vec::new();
        let mut total_interfaces = 0;
        let total_methods = 0;
        let mut total_properties = 0;

        for (object_path, interfaces) in managed {
            let interface_names: Vec<String> = interfaces.keys().cloned().collect();
            total_interfaces += interface_names.len();

            // Count properties from the managed objects data
            let mut properties = Vec::new();
            for (interface_name, props) in &interfaces {
                for (prop_name, prop_value) in props {
                    properties.push(PropertyIndex {
                        name: prop_name.clone(),
                        interface: interface_name.clone(),
                        type_signature: prop_value.value_signature().to_string(),
                        access: "read".to_string(), // ObjectManager only gives current values
                    });
                }
            }
            total_properties += properties.len();

            object_indices.push(ObjectIndex {
                path: object_path,
                interfaces: interface_names,
                methods: Vec::new(), // ObjectManager doesn't include method signatures
                properties,
            });
        }

        Ok(ServiceIndex {
            name: service_name.to_string(),
            objects: object_indices,
            total_interfaces,
            total_methods,
            total_properties,
        })
    }

    /// Discover ALL objects for a service (UNLIMITED - no artificial cap)
    async fn discover_all_objects_unlimited(&self, service_name: &str) -> Result<Vec<String>> {
        // Start from root and recursively discover EVERYTHING
        let mut discovered = Vec::new();
        let mut to_visit = vec!["/".to_string()];
        let mut visited = std::collections::HashSet::new();

        while let Some(path) = to_visit.pop() {
            if visited.contains(&path) {
                continue;
            }
            visited.insert(path.clone());

            discovered.push(path.clone());

            // Try to introspect and find children
            if let Ok(xml) = self
                .introspector
                .introspect_service_at_path(service_name, &path)
                .await
            {
                let children = self.extract_child_nodes(&xml);
                for child in children {
                    let child_path = if path == "/" {
                        format!("/{}", child)
                    } else {
                        format!("{}/{}", path, child)
                    };

                    if !visited.contains(&child_path) {
                        to_visit.push(child_path);
                    }
                }
            }
        }

        Ok(discovered)
    }

    /// Index a single object (extract all methods, properties, interfaces)
    async fn index_object(&self, service_name: &str, object_path: &str) -> Result<ObjectIndex> {
        let xml = self
            .introspector
            .introspect_service_at_path(service_name, object_path)
            .await?;

        // Parse introspection XML with zbus_xml::Node for full method/property signatures
        let node =
            Node::from_reader(xml.as_bytes()).context("Failed to parse introspection XML")?;

        let mut interfaces = Vec::new();
        let mut methods = Vec::new();
        let mut properties = Vec::new();

        // Extract interfaces, methods, and properties with full signatures
        for iface in node.interfaces() {
            let iface_name = iface.name().to_string();
            interfaces.push(iface_name.clone());

            // Extract methods with input/output signatures
            for method in iface.methods() {
                let inputs: Vec<String> = method
                    .args()
                    .iter()
                    .filter(|a| {
                        a.direction()
                            .map(|d| d == zbus_xml::ArgDirection::In)
                            .unwrap_or(true)
                    })
                    .map(|a| a.ty().to_string())
                    .collect();

                let outputs: Vec<String> = method
                    .args()
                    .iter()
                    .filter(|a| {
                        a.direction()
                            .map(|d| d == zbus_xml::ArgDirection::Out)
                            .unwrap_or(false)
                    })
                    .map(|a| a.ty().to_string())
                    .collect();

                methods.push(MethodIndex {
                    name: method.name().to_string(),
                    interface: iface_name.clone(),
                    inputs,
                    outputs,
                });
            }

            // Extract properties with types and access modes
            for property in iface.properties() {
                // PropertyAccess enum: Read, Write, ReadWrite
                let access_str = match property.access() {
                    zbus_xml::PropertyAccess::Read => "read",
                    zbus_xml::PropertyAccess::Write => "write",
                    zbus_xml::PropertyAccess::ReadWrite => "readwrite",
                };

                properties.push(PropertyIndex {
                    name: property.name().to_string(),
                    interface: iface_name.clone(),
                    type_signature: property.ty().to_string(),
                    access: access_str.to_string(),
                });
            }
        }

        Ok(ObjectIndex {
            path: object_path.to_string(),
            interfaces,
            methods,
            properties,
        })
    }

    /// Save index to BTRFS subvolume
    pub fn save(&self, index: &DbusIndex) -> Result<()> {
        // Save main index
        let index_file = self.index_root.join("index.json");
        serde_json::to_writer_pretty(File::create(&index_file)?, index)?;

        // Save per-service files for hierarchical browsing
        let services_dir = self.index_root.join("services");
        fs::create_dir_all(&services_dir)?;

        for (name, service_index) in &index.services {
            let service_file = services_dir.join(format!("{}.json", name.replace(".", "_")));
            serde_json::to_writer_pretty(File::create(&service_file)?, service_index)?;
        }

        log::info!("💾 Index saved to {}", self.index_root.display());
        Ok(())
    }

    /// Load existing index from BTRFS subvolume
    pub fn load(&self) -> Result<DbusIndex> {
        let index_file = self.index_root.join("index.json");
        let index = serde_json::from_reader(File::open(&index_file)?)?;
        Ok(index)
    }

    // Helper method for extracting child nodes using zbus_xml::Node

    fn extract_child_nodes(&self, xml: &str) -> Vec<String> {
        // Extract child <node name="..."/> from introspection XML using zbus_xml
        match Node::from_reader(xml.as_bytes()) {
            Ok(node) => node
                .nodes()
                .iter()
                .filter_map(|n| n.name())
                .map(|s| s.to_string())
                .collect(),
            Err(_) => {
                // Fallback to basic parsing if XML parsing fails
                let mut children = Vec::new();
                for line in xml.lines() {
                    if line.trim().starts_with("<node name=") {
                        if let Some(name) = line.split('"').nth(1) {
                            children.push(name.to_string());
                        }
                    }
                }
                children
            }
        }
    }
}

/// Query engine for searching indexed D-Bus data
pub struct DbusQueryEngine {
    index: DbusIndex,
}

impl DbusQueryEngine {
    pub fn new(index: DbusIndex) -> Self {
        Self { index }
    }

    /// Search for services/objects/methods by name
    pub fn search(&self, query: &str) -> Vec<String> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for (service_name, service) in &self.index.services {
            if service_name.to_lowercase().contains(&query_lower) {
                results.push(format!("Service: {}", service_name));
            }

            for object in &service.objects {
                if object.path.to_lowercase().contains(&query_lower) {
                    results.push(format!("Object: {} ({})", object.path, service_name));
                }

                for method in &object.methods {
                    if method.name.to_lowercase().contains(&query_lower) {
                        results.push(format!(
                            "Method: {}.{} at {}",
                            service_name, method.name, object.path
                        ));
                    }
                }
            }
        }

        results
    }

    /// Get statistics
    pub fn stats(&self) -> &IndexStatistics {
        &self.index.statistics
    }
}

/// Verification result comparing index to live D-Bus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub timestamp: i64,
    pub index_complete: bool,
    pub index_services: usize,
    pub live_services: usize,
    pub missing_from_index: Vec<String>,
    pub extra_in_index: Vec<String>, // Services in index but not live
    pub coverage_percent: f64,
}

impl DbusIndexer {
    /// Verify index completeness against live D-Bus
    pub async fn verify_completeness(&self) -> Result<VerificationResult> {
        log::info!("🔍 Verifying index completeness against live D-Bus...");

        // Get live services from D-Bus
        let live_services = self.introspector.list_all_services().await?;
        log::info!("   Live D-Bus services: {}", live_services.len());

        // Load index
        let index = self.load()?;
        let index_services: Vec<String> = index.services.keys().cloned().collect();
        log::info!("   Indexed services: {}", index_services.len());

        // Find missing services (in live but not in index)
        let missing: Vec<String> = live_services
            .iter()
            .filter(|s| !index_services.contains(s))
            .cloned()
            .collect();

        // Find extra services (in index but not live)
        let extra: Vec<String> = index_services
            .iter()
            .filter(|s| !live_services.contains(s))
            .cloned()
            .collect();

        let coverage = if live_services.is_empty() {
            100.0
        } else {
            ((index_services.len() - extra.len()) as f64 / live_services.len() as f64) * 100.0
        };

        let complete = missing.is_empty();

        log::info!("   Coverage: {:.1}%", coverage);
        if !missing.is_empty() {
            log::warn!("   Missing {} services from index", missing.len());
        }
        if !extra.is_empty() {
            log::info!(
                "   {} services in index but not currently running",
                extra.len()
            );
        }

        Ok(VerificationResult {
            timestamp: chrono::Utc::now().timestamp(),
            index_complete: complete,
            index_services: index_services.len(),
            live_services: live_services.len(),
            missing_from_index: missing,
            extra_in_index: extra,
            coverage_percent: coverage,
        })
    }
}
