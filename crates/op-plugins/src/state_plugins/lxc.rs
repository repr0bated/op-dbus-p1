//! LXC plugin - read-only introspection of LXC containers on the host.
//!
//! Design
//! - Discovers LXC containers by correlating OVS ports (vi{VMID}) with OVS bridges
//!   and known cgroup paths on Proxmox (pve-container@{VMID}.service) for running state.
//! - No CLI; native OVSDB JSON-RPC and filesystem reads only.
//! - Read-only for now (apply is a no-op). StateManager will still produce footprints on apply.

use anyhow::Result;
use async_trait::async_trait;
use op_state::plugtree::PlugTree;
use op_state::{
    ApplyResult, Checkpoint, DiffMetadata, PluginCapabilities, StateAction, StateDiff, StatePlugin,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LxcState {
    pub containers: Vec<ContainerInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContainerInfo {
    pub id: String,
    pub veth: String,
    pub bridge: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub running: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, Value>>, // extensible (includes network_type, template, etc.)
}

pub struct LxcPlugin;

impl LxcPlugin {
    pub fn new() -> Self {
        Self
    }

    /// Apply state for a single container
    pub async fn apply_container_state(&self, container: &ContainerInfo) -> Result<ApplyResult> {
        let mut changes_applied = Vec::new();
        let mut errors = Vec::new();

        // Check if container exists
        let current_containers = self.discover_from_ovs().await?;
        let exists = current_containers.iter().any(|c| c.id == container.id);

        if !exists {
            // Create container
            match Self::create_container(container).await {
                Ok(_) => {
                    changes_applied.push(format!("Created container {}", container.id));

                    // Start it
                    if let Err(e) = Self::start_container(&container.id).await {
                        errors.push(format!("Failed to start container {}: {}", container.id, e));
                    } else {
                        changes_applied.push(format!("Started container {}", container.id));
                    }
                }
                Err(e) => {
                    errors.push(format!(
                        "Failed to create container {}: {}",
                        container.id, e
                    ));
                }
            }
        } else {
            changes_applied.push(format!("Container {} already exists", container.id));
        }

        Ok(ApplyResult {
            success: errors.is_empty(),
            changes_applied,
            errors,
            checkpoint: None,
        })
    }

    fn is_running(ct_id: &str) -> Option<bool> {
        // Proxmox systemd service path: pve-container@{vmid}.service (cgroup v2)
        let path = format!(
            "/sys/fs/cgroup/system.slice/pve-container@{}.service",
            ct_id
        );
        Some(fs::metadata(path).is_ok())
    }

    async fn discover_from_ovs(&self) -> Result<Vec<ContainerInfo>> {
        let client = op_network::ovsdb::OvsdbClient::new();
        // If OVSDB is not reachable, return empty list
        if client.list_dbs().await.is_err() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        let bridges = client.list_bridges().await.unwrap_or_default();
        for br in bridges {
            let ports = client.list_bridge_ports(&br).await.unwrap_or_default();
            for p in ports {
                if let Some(ct_id) = p.strip_prefix("vi") {
                    // ensure ID is numeric-like
                    if ct_id.chars().all(|c| c.is_ascii_digit()) {
                        let running = Self::is_running(ct_id);
                        results.push(ContainerInfo {
                            id: ct_id.to_string(),
                            veth: p.clone(),
                            bridge: br.clone(),
                            running,
                            properties: None,
                        });
                    }
                }
            }
        }
        Ok(results)
    }
}

impl Default for LxcPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlugTree for LxcPlugin {
    fn pluglet_type(&self) -> &str {
        "container"
    }

    fn pluglet_id_field(&self) -> &str {
        "id"
    }

    fn extract_pluglet_id(&self, resource: &Value) -> Result<String> {
        resource
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Container missing 'id' field"))
    }

    async fn apply_pluglet(&self, _pluglet_id: &str, desired: &Value) -> Result<ApplyResult> {
        let container: ContainerInfo = serde_json::from_value(desired.clone())?;
        self.apply_container_state(&container).await
    }

    async fn query_pluglet(&self, pluglet_id: &str) -> Result<Option<Value>> {
        let containers = self.discover_from_ovs().await?;

        for container in containers {
            if container.id == pluglet_id {
                return Ok(Some(serde_json::to_value(container)?));
            }
        }

        Ok(None)
    }

    async fn list_pluglet_ids(&self) -> Result<Vec<String>> {
        let containers = self.discover_from_ovs().await?;
        Ok(containers.into_iter().map(|c| c.id).collect())
    }
}

impl LxcPlugin {
    /// Find container's veth interface name
    async fn find_container_veth(ct_id: &str) -> Result<String> {
        // Try to get the actual interface name from the container's eth0 peer
        let output = tokio::process::Command::new("ip")
            .args([
                "netns",
                "exec",
                &format!("ct{}", ct_id),
                "ip",
                "link",
                "show",
                "eth0",
            ])
            .output()
            .await?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Look for the peer link index
            for line in stdout.lines() {
                if line.contains("link-netnsid") || line.contains("@if") {
                    // Parse the peer interface name from the host side
                    // Format: "veth<random>@if<index>"
                    let veth_interfaces = op_network::rtnetlink::list_veth_interfaces().await?;

                    // Find veth that matches this container's namespace
                    for veth_name in veth_interfaces {
                        if !veth_name.is_empty() {
                            return Ok(veth_name);
                        }
                    }
                }
            }
        }

        // Fallback: try to find veth by checking all veth pairs
        let veth_interfaces = op_network::rtnetlink::list_veth_interfaces().await?;
        // Look for any veth interface (first one found)
        for veth_name in veth_interfaces {
            if !veth_name.is_empty() && veth_name.starts_with("veth") {
                log::info!("Found veth interface: {}", veth_name);
                return Ok(veth_name);
            }
        }

        Err(anyhow::anyhow!(
            "Could not find veth interface for container {}",
            ct_id
        ))
    }

    /// Determine bridge based on network type
    fn get_bridge_for_network_type(container: &ContainerInfo) -> String {
        let network_type = container
            .properties
            .as_ref()
            .and_then(|p| p.get("network_type"))
            .and_then(|v| v.as_str())
            .unwrap_or("bridge");

        match network_type {
            "netmaker" => "mesh".to_string(),     // Netmaker mesh bridge
            "bridge" => container.bridge.clone(), // Traditional bridge (ovsbr0)
            _ => container.bridge.clone(),
        }
    }

    /// Create LXC container via pct (Proxmox)
    async fn create_container(container: &ContainerInfo) -> Result<()> {
        log::info!("Creating LXC container {}", container.id);

        // Select bridge based on network type
        let bridge = Self::get_bridge_for_network_type(container);
        log::info!("Container {} will use bridge {}", container.id, bridge);

        // Extract properties with sensible defaults
        let props = container.properties.as_ref();

        // Check if using BTRFS golden image (fast path) or tar.zst template (slow path)
        let golden_image = props
            .and_then(|p| p.get("golden_image"))
            .and_then(|v| v.as_str());

        if let Some(golden_image_name) = golden_image {
            // BTRFS snapshot path - instant container creation
            return Self::create_container_from_btrfs_snapshot(
                container,
                golden_image_name,
                &bridge,
            )
            .await;
        }

        // Traditional tar.zst template path (fallback)
        let template = props
            .and_then(|p| p.get("template"))
            .and_then(|v| v.as_str())
            .unwrap_or("local-btrfs:vztmpl/debian-13-standard_13.1-2_amd64.tar.zst");

        // Hostname
        let hostname = props
            .and_then(|p| p.get("hostname"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("ct{}", container.id));

        // Memory (MB)
        let memory = props
            .and_then(|p| p.get("memory"))
            .and_then(|v| v.as_u64())
            .unwrap_or(512);

        // Swap (MB)
        let swap = props
            .and_then(|p| p.get("swap"))
            .and_then(|v| v.as_u64())
            .unwrap_or(512);

        // Storage location and size
        let storage = props
            .and_then(|p| p.get("storage"))
            .and_then(|v| v.as_str())
            .unwrap_or("local-btrfs");

        let rootfs_size = props
            .and_then(|p| p.get("rootfs_size"))
            .and_then(|v| v.as_u64())
            .unwrap_or(8);

        let rootfs = format!("{}:{}", storage, rootfs_size);

        // CPU cores
        let cores = props
            .and_then(|p| p.get("cores"))
            .and_then(|v| v.as_u64())
            .unwrap_or(2);

        // Unprivileged mode
        let unprivileged = props
            .and_then(|p| p.get("unprivileged"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Features (comma-separated)
        let features = props
            .and_then(|p| p.get("features"))
            .and_then(|v| v.as_str())
            .unwrap_or("nesting=1");

        // Network configuration
        let firewall = props
            .and_then(|p| p.get("firewall"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let net0 = format!(
            "name=eth0,bridge={},firewall={}",
            bridge,
            if firewall { "1" } else { "0" }
        );

        // Optional: IP address configuration
        if let Some(ip) = props.and_then(|p| p.get("ip")).and_then(|v| v.as_str()) {
            // ip can be "dhcp" or "192.168.1.100/24"
            // pct expects: --net0 "name=eth0,bridge=vmbr0,ip=192.168.1.100/24,gw=192.168.1.1"
            // For now, we'll handle this in a future enhancement
            log::info!("IP configuration: {} (note: not yet implemented)", ip);
        }

        log::info!(
            "Creating container {}: template={}, memory={}MB, cores={}, rootfs={}",
            container.id,
            template,
            memory,
            cores,
            rootfs
        );

        // Build pct create command
        let mut cmd = tokio::process::Command::new("pct");
        cmd.args([
            "create",
            &container.id,
            template,
            "--hostname",
            hostname.as_str(),
            "--memory",
            &memory.to_string(),
            "--swap",
            &swap.to_string(),
            "--cores",
            &cores.to_string(),
            "--rootfs",
            &rootfs,
            "--net0",
            &net0,
            "--unprivileged",
            if unprivileged { "1" } else { "0" },
            "--features",
            features,
        ]);

        // Optional: Start on boot
        if let Some(onboot) = props
            .and_then(|p| p.get("onboot"))
            .and_then(|v| v.as_bool())
        {
            cmd.args(["--onboot", if onboot { "1" } else { "0" }]);
        }

        // Optional: Protection (prevent accidental deletion)
        if let Some(protection) = props
            .and_then(|p| p.get("protection"))
            .and_then(|v| v.as_bool())
        {
            cmd.args(["--protection", if protection { "1" } else { "0" }]);
        }

        // Optional: Nameserver
        if let Some(nameserver) = props
            .and_then(|p| p.get("nameserver"))
            .and_then(|v| v.as_str())
        {
            cmd.args(["--nameserver", nameserver]);
        }

        // Optional: Searchdomain
        if let Some(searchdomain) = props
            .and_then(|p| p.get("searchdomain"))
            .and_then(|v| v.as_str())
        {
            cmd.args(["--searchdomain", searchdomain]);
        }

        // Execute pct create
        let output = cmd.output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("pct create failed: {}", stderr));
        }

        log::info!(
            "Container {} created successfully on bridge {}",
            container.id,
            bridge
        );

        // Inject netmaker token for first-boot join (if netmaker network type)
        let network_type = props
            .and_then(|p| p.get("network_type"))
            .and_then(|v| v.as_str())
            .unwrap_or("bridge");

        if network_type == "netmaker" {
            // Read token from host
            if let Ok(token) = tokio::fs::read_to_string("/etc/op-dbus/netmaker.env").await {
                // Parse NETMAKER_TOKEN=xxx from env file
                for line in token.lines() {
                    if let Some(token_value) = line.strip_prefix("NETMAKER_TOKEN=") {
                        let token_clean = token_value.trim_matches('"').trim();

                        // Write token to container's rootfs /root/.bashrc
                        let bashrc_path =
                            format!("/var/lib/lxc/{}/rootfs/root/.bashrc", container.id);

                        // Append export statement to bashrc
                        let export_line = format!("\nexport NETMAKER_TOKEN={}\n", token_clean);

                        // Read existing bashrc if it exists
                        let existing_content = tokio::fs::read_to_string(&bashrc_path)
                            .await
                            .unwrap_or_default();

                        // Append export if not already present
                        if !existing_content.contains("NETMAKER_TOKEN") {
                            match tokio::fs::write(
                                &bashrc_path,
                                format!("{}{}", existing_content, export_line),
                            )
                            .await
                            {
                                Ok(_) => {
                                    log::info!(
                                        "Injected netmaker token into {} .bashrc",
                                        container.id
                                    );
                                }
                                Err(e) => {
                                    log::warn!(
                                        "Failed to inject netmaker token into {}: {}",
                                        container.id,
                                        e
                                    );
                                }
                            }
                        }
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Create LXC container from BTRFS golden image snapshot (instant provisioning)
    async fn create_container_from_btrfs_snapshot(
        container: &ContainerInfo,
        golden_image_name: &str,
        bridge: &str,
    ) -> Result<()> {
        log::info!(
            "Creating container {} from BTRFS golden image: {}",
            container.id,
            golden_image_name
        );

        let props = container.properties.as_ref();

        // Storage backend (configurable per container)
        let storage = props
            .and_then(|p| p.get("storage"))
            .and_then(|v| v.as_str())
            .unwrap_or("local-btrfs");

        // Proxmox storage paths (adjust based on storage.cfg configuration)
        let storage_path = format!("/var/lib/pve/{}", storage);
        let golden_image_path = format!("{}/templates/subvol/{}", storage_path, golden_image_name);
        let container_rootfs = format!("{}/images/{}/rootfs", storage_path, container.id);
        let container_dir = format!("{}/images/{}", storage_path, container.id);

        // Verify golden image exists
        if tokio::fs::metadata(&golden_image_path).await.is_err() {
            return Err(anyhow::anyhow!(
                "Golden image not found: {}. Create it with: sudo ./create-btrfs-golden-image.sh {}",
                golden_image_path,
                golden_image_name
            ));
        }

        // Check if it's a BTRFS subvolume
        let check_output = tokio::process::Command::new("btrfs")
            .args(["subvolume", "show", &golden_image_path])
            .output()
            .await?;

        if !check_output.status.success() {
            return Err(anyhow::anyhow!(
                "Golden image is not a BTRFS subvolume: {}",
                golden_image_path
            ));
        }

        log::info!("✓ Golden image verified: {}", golden_image_path);

        // Create container directory
        tokio::fs::create_dir_all(&container_dir).await?;

        // Create BTRFS snapshot (instant copy-on-write)
        log::info!("Creating BTRFS snapshot...");
        let snapshot_output = tokio::process::Command::new("btrfs")
            .args([
                "subvolume",
                "snapshot",
                &golden_image_path,
                &container_rootfs,
            ])
            .output()
            .await?;

        if !snapshot_output.status.success() {
            let stderr = String::from_utf8_lossy(&snapshot_output.stderr);
            return Err(anyhow::anyhow!("BTRFS snapshot failed: {}", stderr));
        }

        log::info!("✓ BTRFS snapshot created in <1ms: {}", container_rootfs);

        // Extract properties
        let hostname = props
            .and_then(|p| p.get("hostname"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("ct{}", container.id));

        let memory = props
            .and_then(|p| p.get("memory"))
            .and_then(|v| v.as_u64())
            .unwrap_or(512);

        let swap = props
            .and_then(|p| p.get("swap"))
            .and_then(|v| v.as_u64())
            .unwrap_or(512);

        let cores = props
            .and_then(|p| p.get("cores"))
            .and_then(|v| v.as_u64())
            .unwrap_or(2);

        let unprivileged = props
            .and_then(|p| p.get("unprivileged"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let features = props
            .and_then(|p| p.get("features"))
            .and_then(|v| v.as_str())
            .unwrap_or("nesting=1");

        let firewall = props
            .and_then(|p| p.get("firewall"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Create Proxmox container configuration
        let config_path = format!("/etc/pve/lxc/{}.conf", container.id);
        let config_content = format!(
            r#"arch: amd64
cores: {}
hostname: {}
memory: {}
swap: {}
net0: name=eth0,bridge={},firewall={}
ostype: debian
rootfs: local-btrfs:images/{}/rootfs
unprivileged: {}
features: {}
"#,
            cores,
            hostname,
            memory,
            swap,
            bridge,
            if firewall { "1" } else { "0" },
            container.id,
            if unprivileged { "1" } else { "0" },
            features
        );

        // Add optional properties
        let mut config = config_content;

        if let Some(onboot) = props
            .and_then(|p| p.get("onboot"))
            .and_then(|v| v.as_bool())
        {
            config.push_str(&format!("onboot: {}\n", if onboot { "1" } else { "0" }));
        }

        if let Some(protection) = props
            .and_then(|p| p.get("protection"))
            .and_then(|v| v.as_bool())
        {
            config.push_str(&format!(
                "protection: {}\n",
                if protection { "1" } else { "0" }
            ));
        }

        if let Some(nameserver) = props
            .and_then(|p| p.get("nameserver"))
            .and_then(|v| v.as_str())
        {
            config.push_str(&format!("nameserver: {}\n", nameserver));
        }

        if let Some(searchdomain) = props
            .and_then(|p| p.get("searchdomain"))
            .and_then(|v| v.as_str())
        {
            config.push_str(&format!("searchdomain: {}\n", searchdomain));
        }

        // Write Proxmox config
        tokio::fs::write(&config_path, config).await?;

        log::info!("✓ Proxmox configuration written: {}", config_path);

        // Inject firstboot script if specified
        if let Some(firstboot_script) = props
            .and_then(|p| p.get("firstboot_script"))
            .and_then(|v| v.as_str())
        {
            Self::inject_firstboot_script(container, storage, firstboot_script).await?;
        }

        // Inject Netmaker token for netmaker network type
        let network_type = props
            .and_then(|p| p.get("network_type"))
            .and_then(|v| v.as_str())
            .unwrap_or("bridge");

        if network_type == "netmaker" {
            Self::inject_netmaker_token(container, storage).await?;
        }

        log::info!(
            "✓ Container {} created from golden image '{}' (BTRFS snapshot)",
            container.id,
            golden_image_name
        );

        Ok(())
    }

    /// Inject firstboot script into container rootfs
    async fn inject_firstboot_script(
        container: &ContainerInfo,
        storage: &str,
        script_content: &str,
    ) -> Result<()> {
        let rootfs = format!("/var/lib/pve/{}/images/{}/rootfs", storage, container.id);
        let script_path = format!("{}/usr/local/bin/lxc-firstboot.sh", rootfs);
        let service_path = format!("{}/etc/systemd/system/lxc-firstboot.service", rootfs);

        // Create script directory if needed
        tokio::fs::create_dir_all(format!("{}/usr/local/bin", rootfs)).await?;

        // Write firstboot script
        tokio::fs::write(&script_path, script_content).await?;

        // Make executable
        tokio::process::Command::new("chmod")
            .args(["+x", &script_path])
            .output()
            .await?;

        // Create systemd service
        let service_content = r#"[Unit]
Description=LXC First Boot Initialization
After=network-online.target
Wants=network-online.target
ConditionPathExists=!/var/lib/lxc-firstboot-complete

[Service]
Type=oneshot
ExecStart=/usr/local/bin/lxc-firstboot.sh
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
"#
        .to_string();

        tokio::fs::create_dir_all(format!("{}/etc/systemd/system", rootfs)).await?;
        tokio::fs::write(&service_path, service_content).await?;

        // Enable service (create symlink)
        let symlink_dir = format!("{}/etc/systemd/system/multi-user.target.wants", rootfs);
        tokio::fs::create_dir_all(&symlink_dir).await?;

        let symlink_path = format!("{}/lxc-firstboot.service", symlink_dir);
        tokio::fs::symlink("../lxc-firstboot.service", &symlink_path)
            .await
            .ok(); // Ignore if exists

        log::info!(
            "✓ Firstboot script injected into container {}",
            container.id
        );

        Ok(())
    }

    /// Inject Netmaker enrollment token into container
    async fn inject_netmaker_token(container: &ContainerInfo, storage: &str) -> Result<()> {
        // Read token from host
        if let Ok(token_content) = tokio::fs::read_to_string("/etc/op-dbus/netmaker.env").await {
            for line in token_content.lines() {
                if let Some(token_value) = line.strip_prefix("NETMAKER_TOKEN=") {
                    let token_clean = token_value.trim_matches('"').trim();

                    let rootfs = format!("/var/lib/pve/{}/images/{}/rootfs", storage, container.id);
                    let token_path = format!("{}/etc/netmaker/enrollment-token", rootfs);

                    // Create netmaker directory
                    tokio::fs::create_dir_all(format!("{}/etc/netmaker", rootfs)).await?;

                    // Write token
                    tokio::fs::write(&token_path, token_clean).await?;

                    // Set permissions
                    tokio::process::Command::new("chmod")
                        .args(["600", &token_path])
                        .output()
                        .await?;

                    log::info!("✓ Netmaker token injected into container {}", container.id);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Cleanup OVS port for deleted container
    async fn cleanup_ovs_port_for_container(ct_id: &str) -> Result<String> {
        let client = op_network::ovsdb::OvsdbClient::new();

        // Find port names matching this container (vi{VMID} or internal_{VMID})
        let potential_ports = vec![
            format!("vi{}", ct_id),        // Proxmox veth pattern
            format!("internal_{}", ct_id), // Socket networking pattern
            format!("veth{}pl", ct_id),    // Alternative veth pattern
        ];

        // Try each potential port name
        for port_name in &potential_ports {
            // Check all bridges for this port
            if let Ok(bridges) = client.list_bridges().await {
                for bridge in bridges {
                    if let Ok(ports) = client.list_bridge_ports(&bridge).await {
                        if ports.contains(port_name) {
                            log::info!("Found port {} on bridge {}, removing", port_name, bridge);

                            // Delete the port using OVSDB
                            let operations = serde_json::json!([{
                                "op": "select",
                                "table": "Port",
                                "where": [["name", "==", port_name]],
                                "columns": ["_uuid"]
                            }]);

                            if let Ok(result) = client.transact(operations).await {
                                if let Some(rows) = result[0]["rows"].as_array() {
                                    if let Some(first_row) = rows.first() {
                                        if let Some(uuid_array) = first_row["_uuid"].as_array() {
                                            if uuid_array.len() == 2 && uuid_array[0] == "uuid" {
                                                let port_uuid = uuid_array[1].as_str().unwrap();

                                                // Get bridge UUID
                                                let bridge_ops = serde_json::json!([{
                                                    "op": "select",
                                                    "table": "Bridge",
                                                    "where": [["name", "==", &bridge]],
                                                    "columns": ["_uuid"]
                                                }]);

                                                if let Ok(bridge_result) =
                                                    client.transact(bridge_ops).await
                                                {
                                                    if let Some(bridge_rows) =
                                                        bridge_result[0]["rows"].as_array()
                                                    {
                                                        if let Some(bridge_row) =
                                                            bridge_rows.first()
                                                        {
                                                            if let Some(bridge_uuid_array) =
                                                                bridge_row["_uuid"].as_array()
                                                            {
                                                                if bridge_uuid_array.len() == 2
                                                                    && bridge_uuid_array[0]
                                                                        == "uuid"
                                                                {
                                                                    let bridge_uuid =
                                                                        bridge_uuid_array[1]
                                                                            .as_str()
                                                                            .unwrap();

                                                                    // Remove port from bridge and delete it
                                                                    let delete_ops = serde_json::json!([
                                                                        {
                                                                            "op": "mutate",
                                                                            "table": "Bridge",
                                                                            "where": [["_uuid", "==", ["uuid", bridge_uuid]]],
                                                                            "mutations": [
                                                                                ["ports", "delete", ["uuid", port_uuid]]
                                                                            ]
                                                                        },
                                                                        {
                                                                            "op": "delete",
                                                                            "table": "Port",
                                                                            "where": [["_uuid", "==", ["uuid", port_uuid]]]
                                                                        }
                                                                    ]);

                                                                    client
                                                                        .transact(delete_ops)
                                                                        .await?;
                                                                    return Ok(port_name.clone());
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Err(anyhow::anyhow!("No OVS port found for container {}", ct_id))
    }

    /// Start LXC container
    async fn start_container(ct_id: &str) -> Result<()> {
        let output = tokio::process::Command::new("pct")
            .args(["start", ct_id])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("pct start failed: {}", stderr));
        }

        Ok(())
    }

    /// Stop LXC container
    async fn stop_container(ct_id: &str) -> Result<()> {
        let output = tokio::process::Command::new("pct")
            .args(["stop", ct_id])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("pct stop failed: {}", stderr));
        }

        Ok(())
    }
}

#[async_trait]
impl StatePlugin for LxcPlugin {
    fn name(&self) -> &str {
        "lxc"
    }
    fn version(&self) -> &str {
        "1.0.0"
    }

    fn is_available(&self) -> bool {
        // Check if pct command is available (Proxmox specific)
        std::process::Command::new("pct")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn unavailable_reason(&self) -> String {
        "Proxmox pct command not found - this plugin requires Proxmox VE".to_string()
    }

    async fn query_current_state(&self) -> Result<Value> {
        let containers = self.discover_from_ovs().await?;
        Ok(serde_json::to_value(LxcState { containers })?)
    }

    async fn calculate_diff(&self, current: &Value, desired: &Value) -> Result<StateDiff> {
        // For now, emit a single modify if different; once lifecycle is defined, compute granular actions.
        let actions = if current != desired {
            vec![StateAction::Modify {
                resource: "lxc".into(),
                changes: desired.clone(),
            }]
        } else {
            vec![]
        };
        Ok(StateDiff {
            plugin: self.name().to_string(),
            actions,
            metadata: DiffMetadata {
                timestamp: chrono::Utc::now().timestamp(),
                current_hash: format!("{:x}", md5::compute(serde_json::to_string(current)?)),
                desired_hash: format!("{:x}", md5::compute(serde_json::to_string(desired)?)),
            },
        })
    }

    async fn apply_state(&self, diff: &StateDiff) -> Result<ApplyResult> {
        let mut changes_applied = Vec::new();
        let mut errors = Vec::new();

        for action in &diff.actions {
            match action {
                StateAction::Create {
                    resource: _,
                    config,
                } => {
                    let container: ContainerInfo = serde_json::from_value(config.clone())?;

                    // 1. Create LXC container
                    match Self::create_container(&container).await {
                        Ok(_) => {
                            changes_applied.push(format!("Created container {}", container.id));

                            // 2. Start container to create veth interface
                            if let Err(e) = Self::start_container(&container.id).await {
                                errors.push(format!(
                                    "Failed to start container {}: {}",
                                    container.id, e
                                ));
                                continue;
                            }

                            // Wait for veth to appear
                            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

                            // 3. Find and rename veth
                            let veth_name = format!("vi{}", container.id);
                            match Self::find_container_veth(&container.id).await {
                                Ok(old_veth) => {
                                    log::info!(
                                        "Found veth {} for container {}",
                                        old_veth,
                                        container.id
                                    );

                                    match op_network::rtnetlink::link_set_name(
                                        &old_veth, &veth_name,
                                    )
                                    .await
                                    {
                                        Ok(_) => {
                                            changes_applied.push(format!(
                                                "Renamed {} to {}",
                                                old_veth, veth_name
                                            ));

                                            // 4. Network enrollment based on type
                                            let network_type = container
                                                .properties
                                                .as_ref()
                                                .and_then(|p| p.get("network_type"))
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("bridge");

                                            match network_type {
                                                "netmaker" => {
                                                    // Add to mesh (netmaker mesh bridge)
                                                    let client =
                                                        op_network::ovsdb::OvsdbClient::new();
                                                    match client.add_port("mesh", &veth_name).await
                                                    {
                                                        Ok(_) => {
                                                            log::info!(
                                                                "Container {} on mesh bridge",
                                                                container.id
                                                            );
                                                            changes_applied.push(format!(
                                                                "Added {} to mesh (netmaker bridge)",
                                                                veth_name
                                                            ));
                                                        }
                                                        Err(e) => {
                                                            errors.push(format!(
                                                                "Failed to add {} to mesh: {}",
                                                                veth_name, e
                                                            ));
                                                        }
                                                    }
                                                }
                                                "bridge" => {
                                                    // Add to traditional bridge (ovsbr0)
                                                    let client =
                                                        op_network::ovsdb::OvsdbClient::new();
                                                    match client
                                                        .add_port(&container.bridge, &veth_name)
                                                        .await
                                                    {
                                                        Ok(_) => {
                                                            changes_applied.push(format!(
                                                                "Added {} to bridge {}",
                                                                veth_name, container.bridge
                                                            ));
                                                        }
                                                        Err(e) => {
                                                            errors.push(format!(
                                                                "Failed to add port to bridge: {}",
                                                                e
                                                            ));
                                                        }
                                                    }
                                                }
                                                _ => {
                                                    // Default to traditional bridge
                                                    let client =
                                                        op_network::ovsdb::OvsdbClient::new();
                                                    match client
                                                        .add_port(&container.bridge, &veth_name)
                                                        .await
                                                    {
                                                        Ok(_) => {
                                                            changes_applied.push(format!(
                                                                "Added {} to bridge {}",
                                                                veth_name, container.bridge
                                                            ));
                                                        }
                                                        Err(e) => {
                                                            errors.push(format!(
                                                                "Failed to add port to bridge: {}",
                                                                e
                                                            ));
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            errors.push(format!("Failed to rename veth: {}", e));
                                        }
                                    }
                                }
                                Err(e) => {
                                    // If we can't find the veth, stop the container to prevent orphan
                                    log::warn!(
                                        "Failed to find veth for container {}, stopping container",
                                        container.id
                                    );
                                    let _ = Self::stop_container(&container.id).await;
                                    errors.push(format!(
                                        "Failed to find veth for container {}: {}",
                                        container.id, e
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            errors.push(format!(
                                "Failed to create container {}: {}",
                                container.id, e
                            ));
                        }
                    }
                }
                StateAction::Modify {
                    resource,
                    changes: _,
                } => {
                    // Handle container state changes (start/stop)
                    log::info!(
                        "Modify operation for container {} (not yet implemented)",
                        resource
                    );
                    changes_applied.push(format!("Skipped modify for {}", resource));
                }
                StateAction::Delete { resource } => {
                    // Delete container and cleanup OVS ports
                    log::info!("Deleting container {} and cleaning up OVS ports", resource);

                    // First, try to find and cleanup the OVS port for this container
                    let cleanup_result = Self::cleanup_ovs_port_for_container(resource).await;
                    match cleanup_result {
                        Ok(port_name) => {
                            log::info!(
                                "Cleaned up OVS port {} for container {}",
                                port_name,
                                resource
                            );
                            changes_applied.push(format!(
                                "Removed OVS port {} for container {}",
                                port_name, resource
                            ));
                        }
                        Err(e) => {
                            log::warn!(
                                "Could not cleanup OVS port for container {}: {}",
                                resource,
                                e
                            );
                        }
                    }

                    // Then delete the container
                    let output = tokio::process::Command::new("pct")
                        .args(["destroy", resource])
                        .output()
                        .await;

                    match output {
                        Ok(out) if out.status.success() => {
                            changes_applied.push(format!("Deleted container {}", resource));
                        }
                        _ => {
                            errors.push(format!("Failed to delete container {}", resource));
                        }
                    }
                }
                StateAction::NoOp { .. } => {}
            }
        }

        Ok(ApplyResult {
            success: errors.is_empty(),
            changes_applied,
            errors,
            checkpoint: None,
        })
    }

    async fn verify_state(&self, _desired: &Value) -> Result<bool> {
        Ok(true)
    }

    async fn create_checkpoint(&self) -> Result<Checkpoint> {
        Ok(Checkpoint {
            id: format!("lxc-{}", chrono::Utc::now().timestamp()),
            plugin: self.name().into(),
            timestamp: chrono::Utc::now().timestamp(),
            state_snapshot: json!({}),
            backend_checkpoint: None,
        })
    }

    async fn rollback(&self, _checkpoint: &Checkpoint) -> Result<()> {
        Ok(())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities {
            supports_rollback: false,
            supports_checkpoints: false,
            supports_verification: false,
            atomic_operations: false,
        }
    }
}
