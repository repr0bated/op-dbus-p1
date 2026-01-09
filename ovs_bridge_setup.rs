use op_network::{OvsdbClient, rtnetlink};
use op_tools::builtin::rtnetlink_tools::RtnetlinkAddAddressTool;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔧 OVS Bridge Atomic Configuration");
    println!("==================================");

    // Step 1: Check OVS availability
    println!("Step 1: Checking OVS availability...");
    let ovs_client = OvsdbClient::new();
    match ovs_client.list_dbs().await {
        Ok(dbs) => {
            println!("✅ OVS available, databases: {:?}", dbs);
        }
        Err(e) => {
            println!("❌ OVS not available: {}", e);
            return Err(e.into());
        }
    }

    // Step 2: Introspect network interfaces to find uplink
    println!("\nStep 2: Introspecting network interfaces...");
    let interfaces = rtnetlink::list_interfaces().await?;
    println!("Found {} interfaces:", interfaces.len());

    let mut uplink_interface = None;
    for interface in &interfaces {
        println!("  {} (index: {}, state: {}, addresses: {})",
                 interface.name,
                 interface.index,
                 interface.state,
                 interface.addresses.len());

        // Find uplink: up interface with IP addresses, not loopback/docker
        if interface.state == "up" &&
           !interface.addresses.is_empty() &&
           !interface.name.starts_with("lo") &&
           !interface.name.contains("docker") &&
           !interface.name.contains("veth") {
            uplink_interface = Some(interface.clone());
            println!("    ⭐ SELECTED AS UPLINK");
        }
    }

    let uplink = uplink_interface.ok_or("No suitable uplink interface found")?;
    println!("Using uplink interface: {}", uplink.name);

    // Step 3: Create OVS bridge
    println!("\nStep 3: Creating OVS bridge...");
    let bridge_name = "ovs-br0";

    // Check if bridge already exists
    let existing_bridges = ovs_client.list_bridges().await?;
    if existing_bridges.contains(&bridge_name.to_string()) {
        println!("✅ Bridge {} already exists", bridge_name);
    } else {
        ovs_client.create_bridge(bridge_name).await?;
        println!("✅ Created bridge {}", bridge_name);
    }

    // Step 4: Add uplink port to bridge
    println!("\nStep 4: Adding uplink port to bridge...");
    ovs_client.add_port(bridge_name, &uplink.name).await?;
    println!("✅ Added uplink port {} to bridge {}", uplink.name, bridge_name);

    // Step 5: Create internal port
    println!("\nStep 5: Creating internal port...");
    let internal_port = "ovs-int0";
    ovs_client.add_port_with_type(bridge_name, internal_port, Some("internal")).await?;
    println!("✅ Created internal port {} on bridge {}", internal_port, bridge_name);

    // Step 6: Assign IP to internal port
    println!("\nStep 6: Assigning IP to internal port...");

    // Find an available IP in the same subnet as uplink
    let uplink_ip = uplink.addresses.first()
        .and_then(|addr| addr.address.parse::<std::net::IpAddr>().ok())
        .ok_or("No IP address found on uplink interface")?;

    println!("Uplink IP: {}", uplink_ip);

    // For now, assign a static IP (this would need to be made dynamic)
    let internal_ip = match uplink_ip {
        std::net::IpAddr::V4(_) => "192.168.100.1/24",
        std::net::IpAddr::V6(_) => "2001:db8::1/64",
    };

    println!("Assigning {} to {}", internal_ip, internal_port);

    // Parse IP and prefix
    let parts: Vec<&str> = internal_ip.split('/').collect();
    let ip_addr = parts[0].parse::<std::net::Ipv4Addr>()?;
    let prefix_len = parts[1].parse::<u8>()?;

    // Add IP address using rtnetlink
    rtnetlink::add_ipv4_address(internal_port, ip_addr, prefix_len).await?;
    println!("✅ Assigned IP {} to internal port {}", internal_ip, internal_port);

    // Step 7: Bring interface up
    println!("\nStep 7: Bringing internal port up...");
    rtnetlink::link_up(internal_port).await?;
    println!("✅ Internal port {} is up", internal_port);

    // Step 8: Verify configuration
    println!("\nStep 8: Verifying configuration...");
    let bridges_after = ovs_client.list_bridges().await?;
    println!("OVS bridges: {:?}", bridges_after);

    let ports = ovs_client.list_bridge_ports(bridge_name).await?;
    println!("Bridge {} ports: {:?}", bridge_name, ports);

    let interfaces_after = rtnetlink::list_interfaces().await?;
    for interface in interfaces_after {
        if interface.name == internal_port {
            println!("Internal port {} addresses: {:?}",
                     internal_port,
                     interface.addresses.iter().map(|a| &a.address).collect::<Vec<_>>());
        }
    }

    println!("\n🎉 OVS Bridge configuration completed successfully!");
    println!("Bridge: {}", bridge_name);
    println!("Uplink: {}", uplink.name);
    println!("Internal: {} ({})", internal_port, internal_ip);

    Ok(())
}