# Gemini Code Assistant Context: op-dbus-v2

This document provides context for the `op-dbus-v2` project to an AI code assistant.

## Project Overview

`op-dbus-v2` is a comprehensive, modular infrastructure management and automation platform written in Rust. It is designed to provide fine-grained control over a system's state, networking, and services, with a powerful AI/LLM integration layer for automation and chat-based interaction.

The system is architected as a monorepo containing multiple Rust crates, each responsible for a specific domain:

*   **Core Infrastructure (`op-state`, `op-plugins`, `op-blockchain`, `op-cache`):** A declarative state management engine forms the core. It uses a plugin architecture to manage different subsystems. All state changes are recorded onto a "streaming blockchain" (leveraging BTRFS snapshots) for a complete, verifiable audit trail and rollback capabilities. A BTRFS-based caching layer with NUMA-awareness provides high-performance data access.

*   **Networking (`op-network`, `op-jsonrpc`):** The platform has deep networking capabilities, including a native Rust implementation for managing Open vSwitch (OVS) and OpenFlow rules, bypassing the need for shell command wrappers. It also implements a "Privacy Router" for complex, privacy-focused network chaining.

*   **D-Bus Integration (`op-introspection`, `op-tools`):** D-Bus is a cornerstone of the architecture. The system can introspect existing D-Bus services on the host and automatically generate corresponding management plugins. An extensive library of built-in tools provides direct, programmatic access to D-Bus services like `systemd`, `NetworkManager`, and `PackageKit`.

*   **AI & Chat Integration (`op-mcp`, `op-chat`, `op-llm`, `chat-ui`):** The project integrates with Large Language Models (LLMs) via the "Model Context Protocol" (MCP). This exposes the system's vast toolset to an AI, enabling it to perform complex management tasks through conversation. The `chat-ui` (a SvelteKit application) provides a web interface for this interaction.

*   **Web Server (`op-web`, `op-http`):** An `axum`-based web server exposes the system's functionality via a REST API and serves the `chat-ui` frontend.

## Building and Running

The project is designed to be deployed as a set of systemd services on a Linux host.

### Key Components

*   **`op-dbus-service`:** The main D-Bus service process.
*   **`op-web`:** The web server that hosts the API and the `chat-ui`.
*   **`chat-ui`:** The SvelteKit frontend application.
*   **`nginx`:** Used as a reverse proxy in production deployments.

### Development

**1. Rust Backend:**

*   **Build:** The entire workspace can be built from the root of the project.
    ```bash
    cargo build
    ```
*   **Run individual services:**
    ```bash
    # Run the main web server
    cargo run --package op-web --bin op-web-server

    # Run the MCP server
    cargo run --package op-mcp --bin op-mcp-server
    ```

**2. SvelteKit Frontend (`chat-ui`):**

*   **Setup:** Navigate to the `chat-ui` directory and install dependencies.
    ```bash
    cd chat-ui
    npm install
    ```
*   **Run:** Start the development server.
    ```bash
    npm run dev -- --open
    ```
    The UI will be available at `http://localhost:5173`. It needs to be configured to connect to the backend `op-web` service.

### Production Deployment

The `deploy/install.sh` script provides a comprehensive method for deploying the application on a target machine. It handles:
*   Building release binaries.
*   Setting up system directories (`/etc/op-dbus`, `/var/lib/op-dbus`).
*   Creating an environment file (`/etc/op-dbus/op-web.env`).
*   Installing and enabling `systemd` services (`op-web.service`, `op-dbus-service.service`).
*   Configuring `nginx` as a reverse proxy with TLS.

**Example Installation:**
```bash
# Run the installer with default settings
sudo ./deploy/install.sh
```

## Development Conventions

*   **Monorepo Structure:** The project is a Rust workspace. New functionality should be added in new or existing crates to maintain modularity.
*   **State Management:** All system modifications should be funneled through the `op-state` engine to ensure they are captured by the audit trail. Prefer creating or using a `StatePlugin` over direct manipulation.
*   **Tooling:** When adding functionality that should be exposed to the AI, create a tool in the `op-tools` crate.
*   **D-Bus:** Interact with system services via D-Bus using the `zbus` crate whenever possible, rather than shelling out to command-line tools.
*   **Configuration:** Configuration is primarily handled through JSON files which are parsed by `op-state`. While some comments suggest YAML support, the current implementation heavily relies on JSON. Environment variables are used for secrets and runtime-specific settings.
*   **Asynchronous:** The entire codebase is built on `tokio` and is heavily asynchronous.
