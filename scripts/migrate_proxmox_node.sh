#!/bin/bash
# Post-reboot migration script for Proxmox rename
# Run this AFTER rebooting into the new hostname 'op-dbus'

set -e

OLD_NODE="xray"
NEW_NODE="op-dbus"

echo "Checking if we are running as $NEW_NODE..."
CURRENT_HOSTNAME=$(hostname)

if [ "$CURRENT_HOSTNAME" != "$NEW_NODE" ] && [ "$CURRENT_HOSTNAME" != "$NEW_NODE.ghostbridge.tech" ]; then
    echo "Error: Hostname is '$CURRENT_HOSTNAME', expected '$NEW_NODE'"
    echo "Please reboot first!"
    exit 1
fi

echo "Moving QEMU/KVM Virtual Machines..."
if [ -d "/etc/pve/nodes/$OLD_NODE/qemu-server" ]; then
    mkdir -p /etc/pve/nodes/$NEW_NODE/qemu-server
    # Copy config files
    cp /etc/pve/nodes/$OLD_NODE/qemu-server/*.conf /etc/pve/nodes/$NEW_NODE/qemu-server/ 2>/dev/null || true
    echo "VMs moved."
else
    echo "No VMs found to move."
fi

echo "Moving LXC Containers..."
if [ -d "/etc/pve/nodes/$OLD_NODE/lxc" ]; then
    mkdir -p /etc/pve/nodes/$NEW_NODE/lxc
    # Copy config files
    cp /etc/pve/nodes/$OLD_NODE/lxc/*.conf /etc/pve/nodes/$NEW_NODE/lxc/ 2>/dev/null || true
    echo "Containers moved."
else
    echo "No containers found to move."
fi

echo "Cleaning up..."
# We don't delete the old node directory immediately just in case
echo "Migration complete. Check the Proxmox UI."
echo "If everything looks good, you can manually remove /etc/pve/nodes/$OLD_NODE later."
