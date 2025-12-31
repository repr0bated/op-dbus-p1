//! MCP Tool Picker - Web UI for selecting which tools to serve
//!
//! Provides a web interface where users can:
//! 1. See all available tools grouped by category
//! 2. Select/deselect individual tools
//! 3. Save a custom profile (max 35 tools)
//! 4. Get the MCP endpoint URL for their custom profile

use axum::{
    extract::State,
    response::{Html, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::state::AppState;

/// Maximum tools that can be selected (Cursor limit)
pub const MAX_SELECTED_TOOLS: usize = 35;

/// Custom profile storage (persisted to disk)
#[derive(Debug)]
pub struct CustomProfiles {
    profiles: RwLock<HashMap<String, HashSet<String>>>,
}

const PROFILES_PATH: &str = "/var/lib/op-dbus/mcp-profiles.json";

impl CustomProfiles {
    pub fn new() -> Self {
        let mut profiles = HashMap::new();
        
        // Try to load from disk
        if let Ok(content) = std::fs::read_to_string(PROFILES_PATH) {
            match serde_json::from_str::<HashMap<String, HashSet<String>>>(&content) {
                Ok(saved) => {
                    info!("Loaded {} custom MCP profiles from {}", saved.len(), PROFILES_PATH);
                    profiles = saved;
                }
                Err(e) => {
                    tracing::error!("Failed to parse MCP profiles from {}: {}", PROFILES_PATH, e);
                }
            }
        } else {
            info!("No existing MCP profiles found at {}", PROFILES_PATH);
        }

        Self {
            profiles: RwLock::new(profiles),
        }
    }

    pub fn default() -> Self {
        Self::new()
    }

    pub async fn get_profile(&self, name: &str) -> Option<HashSet<String>> {
        self.profiles.read().await.get(name).cloned()
    }

    pub async fn set_profile(&self, name: String, tools: HashSet<String>) {
        {
            let mut lock = self.profiles.write().await;
            lock.insert(name, tools);
        } // Drop write lock
        
        // Save to disk
        self.save_to_disk().await;
    }

    pub async fn list_profiles(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.profiles.read().await.keys().cloned().collect();
        keys.sort();
        keys
    }

    async fn save_to_disk(&self) {
        let lock = self.profiles.read().await;
        match serde_json::to_string_pretty(&*lock) {
            Ok(json) => {
                if let Err(e) = tokio::fs::write(PROFILES_PATH, json).await {
                    tracing::error!("Failed to save MCP profiles to {}: {}", PROFILES_PATH, e);
                } else {
                    info!("Saved custom MCP profiles to {}", PROFILES_PATH);
                }
            }
            Err(e) => {
                tracing::error!("Failed to serialize MCP profiles: {}", e);
            }
        }
    }
}

/// Global custom profiles storage
lazy_static::lazy_static! {
    pub static ref CUSTOM_PROFILES: CustomProfiles = CustomProfiles::new();
}

/// Create the tool picker router
pub fn create_picker_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(picker_page))
        .route("/api/tools", get(list_all_tools))
        .route("/api/profiles", get(list_custom_profiles))
        .route("/api/profiles/:name", post(save_profile))
        .route("/api/profiles/:name", get(get_profile))
        .with_state(state)
}

/// Serve the tool picker HTML page
async fn picker_page() -> Html<String> {
    Html(PICKER_HTML.to_string())
}

/// List all available tools grouped by category
async fn list_all_tools(State(state): State<Arc<AppState>>) -> Json<Value> {
    let tools = state.tool_registry.list().await;
    
    let mut by_category: HashMap<String, Vec<Value>> = HashMap::new();
    
    for tool in &tools {
        let entry = by_category.entry(tool.category.clone()).or_default();
        entry.push(json!({
            "name": tool.name,
            "description": tool.description,
        }));
    }
    
    // Sort categories and tools
    let mut categories: Vec<Value> = by_category
        .into_iter()
        .map(|(cat, mut tools)| {
            tools.sort_by(|a, b| {
                a.get("name").and_then(|v| v.as_str()).unwrap_or("")
                    .cmp(b.get("name").and_then(|v| v.as_str()).unwrap_or(""))
            });
            json!({
                "category": cat,
                "tools": tools,
                "count": tools.len()
            })
        })
        .collect();
    
    categories.sort_by(|a, b| {
        a.get("category").and_then(|v| v.as_str()).unwrap_or("")
            .cmp(b.get("category").and_then(|v| v.as_str()).unwrap_or(""))
    });
    
    Json(json!({
        "total_tools": tools.len(),
        "max_selectable": MAX_SELECTED_TOOLS,
        "categories": categories
    }))
}

/// List saved custom profiles
async fn list_custom_profiles() -> Json<Value> {
    let profiles = CUSTOM_PROFILES.list_profiles().await;
    Json(json!({ "profiles": profiles }))
}

#[derive(Debug, Deserialize)]
struct SaveProfileRequest {
    tools: Vec<String>,
}

/// Save a custom profile
async fn save_profile(
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(request): Json<SaveProfileRequest>,
) -> Json<Value> {
    let tools: HashSet<String> = request.tools.into_iter().take(MAX_SELECTED_TOOLS).collect();
    let count = tools.len();
    
    CUSTOM_PROFILES.set_profile(name.clone(), tools).await;
    
    info!("Saved custom MCP profile '{}' with {} tools", name, count);
    
    Json(json!({
        "success": true,
        "profile": name,
        "tool_count": count,
        "mcp_endpoint": format!("/mcp/custom/{}", name)
    }))
}

/// Get a custom profile
async fn get_profile(
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Json<Value> {
    match CUSTOM_PROFILES.get_profile(&name).await {
        Some(tools) => {
            let tools: std::collections::HashSet<String> = tools;
            let tools_vec: Vec<String> = tools.into_iter().collect();
            Json(json!({
                "profile": name,
                "tools": tools_vec,
                "mcp_endpoint": format!("/mcp/custom/{}", name)
            }))
        },
        None => Json(json!({
            "error": format!("Profile '{}' not found", name)
        })),
    }
}

/// The HTML page for the tool picker
const PICKER_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>MCP Tool Picker - op-dbus</title>
    <style>
        :root {
            --bg-primary: #0f0f1a;
            --bg-secondary: #1a1a2e;
            --bg-tertiary: #252540;
            --text-primary: #e0e0ff;
            --text-secondary: #a0a0c0;
            --accent: #6366f1;
            --accent-hover: #818cf8;
            --success: #10b981;
            --warning: #f59e0b;
            --danger: #ef4444;
            --border: #3f3f5a;
        }
        
        * {
            box-sizing: border-box;
            margin: 0;
            padding: 0;
        }
        
        body {
            font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
            background: var(--bg-primary);
            color: var(--text-primary);
            min-height: 100vh;
            line-height: 1.6;
        }
        
        .container {
            max-width: 1600px;
            margin: 0 auto;
            padding: 2rem;
        }
        
        header {
            text-align: center;
            margin-bottom: 2rem;
            padding: 2rem;
            background: linear-gradient(135deg, var(--bg-secondary), var(--bg-tertiary));
            border-radius: 16px;
            border: 1px solid var(--border);
        }
        
        h1 {
            font-size: 2.5rem;
            margin-bottom: 0.5rem;
            background: linear-gradient(135deg, var(--accent), #a855f7);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
        }
        
        .subtitle {
            color: var(--text-secondary);
            font-size: 1.1rem;
        }
        
        .stats-bar {
            display: flex;
            justify-content: center;
            gap: 2rem;
            margin-top: 1.5rem;
            padding: 1rem;
            background: var(--bg-primary);
            border-radius: 8px;
        }
        
        .stat {
            text-align: center;
        }
        
        .stat-value {
            font-size: 2rem;
            font-weight: bold;
            color: var(--accent);
        }
        
        .stat-value.warning { color: var(--warning); }
        .stat-value.danger { color: var(--danger); }
        .stat-value.success { color: var(--success); }
        
        .stat-label {
            font-size: 0.875rem;
            color: var(--text-secondary);
        }
        
        /* Layout for 2 columns */
        .main-layout {
            display: flex;
            gap: 2rem;
            align-items: flex-start;
        }
        
        .tools-column {
            flex: 1;
            min-width: 0;
        }
        
        .sidebar-column {
            width: 320px;
            flex-shrink: 0;
            position: sticky;
            top: 1rem;
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 12px;
            display: flex;
            flex-direction: column;
            max-height: calc(100vh - 4rem);
        }
        
        .sidebar-header {
            padding: 1rem;
            border-bottom: 1px solid var(--border);
            font-weight: 600;
            background: var(--bg-tertiary);
            border-radius: 12px 12px 0 0;
            display: flex;
            justify-content: space-between;
            align-items: center;
        }

        .sidebar-content {
            padding: 0.5rem;
            overflow-y: auto;
            flex: 1;
        }

        .selected-list {
            list-style: none;
        }

        .selected-item {
            display: flex;
            justify-content: space-between;
            align-items: center;
            padding: 0.5rem 0.75rem;
            border-bottom: 1px solid var(--border);
            font-size: 0.9rem;
            background: var(--bg-primary);
            margin-bottom: 0.25rem;
            border-radius: 4px;
        }

        .selected-item:last-child { margin-bottom: 0; }
        
        .selected-name {
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
            margin-right: 0.5rem;
        }

        .remove-btn {
            color: var(--text-secondary);
            cursor: pointer;
            font-size: 1.1rem;
            line-height: 1;
        }
        
        .remove-btn:hover { color: var(--danger); }
        
        .controls {
            display: flex;
            gap: 1rem;
            margin-bottom: 2rem;
            flex-wrap: wrap;
        }
        
        input[type="text"] {
            flex: 1;
            min-width: 200px;
            padding: 0.75rem 1rem;
            border: 1px solid var(--border);
            border-radius: 8px;
            background: var(--bg-secondary);
            color: var(--text-primary);
            font-size: 1rem;
        }
        
        input[type="text"]:focus {
            outline: none;
            border-color: var(--accent);
        }
        
        button {
            padding: 0.75rem 1.5rem;
            border: none;
            border-radius: 8px;
            font-size: 1rem;
            font-weight: 600;
            cursor: pointer;
            transition: all 0.2s;
        }
        
        .btn-primary {
            background: var(--accent);
            color: white;
        }
        
        .btn-primary:hover {
            background: var(--accent-hover);
            transform: translateY(-1px);
        }
        
        .btn-primary:disabled {
            background: var(--border);
            cursor: not-allowed;
            transform: none;
        }
        
        .btn-secondary {
            background: var(--bg-tertiary);
            color: var(--text-primary);
            border: 1px solid var(--border);
        }
        
        .btn-secondary:hover {
            background: var(--border);
        }
        
        .categories {
            display: grid;
            grid-template-columns: repeat(auto-fill, minmax(350px, 1fr));
            gap: 1.5rem;
        }
        
        .category {
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 12px;
            overflow: hidden;
        }
        
        .category-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            padding: 1rem 1.25rem;
            background: var(--bg-tertiary);
            border-bottom: 1px solid var(--border);
            cursor: pointer;
        }
        
        .category-header:hover {
            background: var(--border);
        }
        
        .category-name {
            font-weight: 600;
            font-size: 1rem;
        }
        
        .category-count {
            font-size: 0.875rem;
            color: var(--text-secondary);
        }
        
        .category-tools {
            max-height: 300px;
            overflow-y: auto;
        }
        
        .tool {
            display: flex;
            align-items: flex-start;
            gap: 0.75rem;
            padding: 0.75rem 1.25rem;
            border-bottom: 1px solid var(--border);
            cursor: pointer;
            transition: background 0.15s;
        }
        
        .tool:last-child {
            border-bottom: none;
        }
        
        .tool:hover {
            background: var(--bg-tertiary);
        }
        
        .tool.selected {
            background: rgba(99, 102, 241, 0.1);
        }
        
        .tool input[type="checkbox"] {
            margin-top: 0.25rem;
            width: 18px;
            height: 18px;
            accent-color: var(--accent);
        }
        
        .tool-info {
            flex: 1;
            min-width: 0;
        }
        
        .tool-name {
            font-weight: 500;
            font-size: 0.95rem;
            word-break: break-all;
        }
        
        .tool-description {
            font-size: 0.8rem;
            color: var(--text-secondary);
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
        }
        
        /* Client Config Tabs */
        .config-section {
            margin-top: 1.5rem;
            margin-bottom: 2rem;
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 12px;
            overflow: hidden;
            display: none;
        }
        
        .config-section.show {
            display: block;
        }
        
        .config-section h3 {
            padding: 1rem 1.25rem;
            background: var(--bg-tertiary);
            border-bottom: 1px solid var(--border);
            font-size: 1rem;
            display: flex;
            align-items: center;
            gap: 0.5rem;
        }
        
        .tabs {
            display: flex;
            border-bottom: 1px solid var(--border);
            background: var(--bg-tertiary);
            overflow-x: auto;
        }
        
        .tab {
            padding: 0.75rem 1.25rem;
            cursor: pointer;
            border-bottom: 2px solid transparent;
            color: var(--text-secondary);
            font-weight: 500;
            transition: all 0.2s;
            white-space: nowrap;
        }
        
        .tab:hover {
            color: var(--text-primary);
            background: rgba(99, 102, 241, 0.1);
        }
        
        .tab.active {
            color: var(--accent);
            border-bottom-color: var(--accent);
        }
        
        .tab-content {
            display: none;
            padding: 1rem;
        }
        
        .tab-content.active {
            display: block;
        }
        
        .tab-content h4 {
            font-size: 0.9rem;
            color: var(--text-secondary);
            margin-bottom: 0.5rem;
        }
        
        .tab-content p {
            font-size: 0.85rem;
            color: var(--text-secondary);
            margin-bottom: 0.75rem;
        }
        
        .json-block {
            background: var(--bg-primary);
            border: 1px solid var(--border);
            border-radius: 8px;
            padding: 1rem;
            font-family: 'Fira Code', 'Monaco', monospace;
            font-size: 0.85rem;
            line-height: 1.5;
            overflow-x: auto;
            white-space: pre;
            position: relative;
        }
        
        .json-block .copy-json {
            position: absolute;
            top: 0.5rem;
            right: 0.5rem;
            padding: 0.25rem 0.5rem;
            font-size: 0.75rem;
            background: var(--bg-tertiary);
            border: 1px solid var(--border);
            border-radius: 4px;
            cursor: pointer;
        }
        
        .json-block .copy-json:hover {
            background: var(--accent);
            color: white;
        }
        
        .json-key { color: #a78bfa; }
        .json-string { color: #34d399; }
        .json-number { color: #fbbf24; }
        
        .endpoint {
            background: var(--bg-primary);
            padding: 1rem;
            border-radius: 8px;
            font-family: monospace;
            font-size: 0.95rem;
            word-break: break-all;
            margin-bottom: 1rem;
        }
        
        .copy-btn {
            width: 100%;
        }
        
        .search-box {
            position: relative;
        }
        
        .search-box::before {
            content: "🔍";
            position: absolute;
            left: 1rem;
            top: 50%;
            transform: translateY(-50%);
        }
        
        .search-box input {
            padding-left: 2.5rem;
        }
        
        @media (max-width: 1024px) {
            .main-layout { flex-direction: column; }
            .sidebar-column { 
                width: 100%; 
                position: static; 
                max-height: 400px;
                order: -1; /* Show selected tools on top on mobile if desired, or remove to show at bottom */
                margin-bottom: 2rem;
            }
        }
        
        @media (max-width: 768px) {
            .container { padding: 1rem; }
            h1 { font-size: 1.75rem; }
            .stats-bar { flex-wrap: wrap; }
            .categories { grid-template-columns: 1fr; }
        }
    </style>
</head>
<body>
    <div class="container">
        <header>
            <h1>🔧 MCP Tool Picker</h1>
            <p class="subtitle">Select tools to serve via MCP (max 35 for Cursor compatibility)</p>
            
            <div class="stats-bar">
                <div class="stat">
                    <div class="stat-value" id="total-tools">0</div>
                    <div class="stat-label">Total Tools</div>
                </div>
                <div class="stat">
                    <div class="stat-value" id="selected-count">0</div>
                    <div class="stat-label">Selected</div>
                </div>
                <div class="stat">
                    <div class="stat-value" id="remaining">35</div>
                    <div class="stat-label">Remaining</div>
                </div>
            </div>
        </header>
        
        <div class="main-layout">
            <div class="tools-column">
                <div class="controls">
                    <div class="search-box" style="flex: 2;">
                        <input type="text" id="search" placeholder="Search tools...">
                    </div>
                    <select id="saved-profiles" style="padding: 0.75rem; border-radius: 8px; background: var(--bg-secondary); color: var(--text-primary); border: 1px solid var(--border);">
                        <option value="">-- Load Saved Profile --</option>
                    </select>
                    <button class="btn-secondary" onclick="loadSelectedProfile()">📂 Load</button>
                    <input type="text" id="profile-name" placeholder="Profile name" value="default" style="max-width: 150px;">
                    <button class="btn-primary" id="save-btn" onclick="saveProfile()">
                        💾 Save
                    </button>
                    <button class="btn-secondary" onclick="selectAll()">Select All</button>
                    <button class="btn-secondary" onclick="deselectAll()">Clear</button>
                </div>
                
                <!-- Client Config Section (shown after save) -->
                <div class="config-section" id="config-section">
                    <h3>📋 Client Configuration</h3>
                    
                    <div class="tabs">
                        <div class="tab active" onclick="showTab('gemini')">Gemini</div>
                        <div class="tab" onclick="showTab('claude')">Claude</div>
                        <div class="tab" onclick="showTab('codex')">Codex</div>
                        <div class="tab" onclick="showTab('antigravity')">Antigravity</div>
                        <div class="tab" onclick="showTab('cursor')">Cursor</div>
                        <div class="tab" onclick="showTab('generic')">Generic</div>
                    </div>
                    
                    <div class="tab-content active" id="tab-gemini">
                        <h4>Gemini MCP Configuration</h4>
                        <p>Configuration for Gemini clients:</p>
                        <div class="json-block" id="gemini-config">
                            <button class="copy-json" onclick="copyJson('gemini-config')">Copy</button>
                        </div>
                    </div>

                    <div class="tab-content" id="tab-claude">
                        <h4>Claude Desktop Configuration</h4>
                        <p>Add this to your <code>claude_desktop_config.json</code>:</p>
                        <div class="json-block" id="claude-config">
                            <button class="copy-json" onclick="copyJson('claude-config')">Copy</button>
                        </div>
                    </div>
                    
                    <div class="tab-content" id="tab-codex">
                        <h4>Codex Configuration</h4>
                        <p>Configuration for Codex:</p>
                        <div class="json-block" id="codex-config">
                            <button class="copy-json" onclick="copyJson('codex-config')">Copy</button>
                        </div>
                    </div>
                    
                    <div class="tab-content" id="tab-antigravity">
                        <h4>Antigravity Configuration</h4>
                        <p>Add this to your <code>~/.gemini/antigravity/mcp_config.json</code>:</p>
                        <div class="json-block" id="antigravity-config">
                            <button class="copy-json" onclick="copyJson('antigravity-config')">Copy</button>
                        </div>
                    </div>

                    <div class="tab-content" id="tab-cursor">
                        <h4>Cursor MCP Configuration</h4>
                        <p>Add this to your <code>~/.cursor/mcp.json</code> file:</p>
                        <div class="json-block" id="cursor-config">
                            <button class="copy-json" onclick="copyJson('cursor-config')">Copy</button>
                        </div>
                    </div>
                    
                    <div class="tab-content" id="tab-generic">
                        <h4>Generic MCP Endpoint</h4>
                        <p>Use this endpoint URL in any MCP-compatible client:</p>
                        <div class="endpoint" id="endpoint-url"></div>
                        <button class="btn-primary copy-btn" onclick="copyEndpoint()">📋 Copy Endpoint URL</button>
                    </div>
                </div>
                
                <div class="categories" id="categories">
                    <!-- Categories will be populated by JavaScript -->
                </div>
            </div>

            <div class="sidebar-column">
                <div class="sidebar-header">
                    <span>Selected Tools</span>
                    <span id="sidebar-count" style="color: var(--accent);">0</span>
                </div>
                <div class="sidebar-content">
                    <ul class="selected-list" id="selected-list">
                        <!-- List populated by JS -->
                    </ul>
                </div>
            </div>
        </div>
    </div>
    
    <script>
        let allTools = [];
        let selectedTools = new Set();
        let currentEndpoint = '';
        let currentProfileName = '';
        const MAX_TOOLS = 35;
        
        async function init() {
            const response = await fetch('/mcp-picker/api/tools');
            const data = await response.json();
            
            document.getElementById('total-tools').textContent = data.total_tools;
            renderCategories(data.categories);
            updateStats();
            
            // Load saved profiles into dropdown
            await loadSavedProfiles();
        }
        
        async function loadSavedProfiles() {
            const response = await fetch('/mcp-picker/api/profiles');
            const data = await response.json();
            
            const select = document.getElementById('saved-profiles');
            // Keep first option, remove rest
            select.innerHTML = '<option value="">-- Load Saved Profile --</option>';
            
            if (data.profiles && data.profiles.length > 0) {
                data.profiles.forEach(name => {
                    const option = document.createElement('option');
                    option.value = name;
                    option.textContent = name;
                    select.appendChild(option);
                });
            }
        }
        
        async function loadSelectedProfile() {
            const select = document.getElementById('saved-profiles');
            const profileName = select.value;
            
            if (!profileName) {
                alert('Please select a profile to load');
                return;
            }
            
            const response = await fetch(`/mcp-picker/api/profiles/${profileName}`);
            const data = await response.json();
            
            if (data.error) {
                alert(data.error);
                return;
            }
            
            // Clear current selections
            selectedTools.clear();
            document.querySelectorAll('.tool.selected').forEach(el => {
                el.classList.remove('selected');
                el.querySelector('input').checked = false;
            });
            
            // Apply loaded profile
            data.tools.forEach(toolName => {
                selectedTools.add(toolName);
                const toolEl = document.querySelector(`.tool[data-name="${toolName}"]`);
                if (toolEl) {
                    toolEl.classList.add('selected');
                    toolEl.querySelector('input').checked = true;
                }
            });
            
            // Update profile name input
            document.getElementById('profile-name').value = profileName;
            
            updateStats();
            alert(`Loaded profile "${profileName}" with ${data.tools.length} tools`);
        }
        
        function renderCategories(categories) {
            const container = document.getElementById('categories');
            container.innerHTML = '';
            
            categories.forEach(cat => {
                const div = document.createElement('div');
                div.className = 'category';
                div.innerHTML = `
                    <div class="category-header" onclick="toggleCategory(this)">
                        <span class="category-name">${cat.category}</span>
                        <span class="category-count">${cat.count} tools</span>
                    </div>
                    <div class="category-tools">
                        ${cat.tools.map(tool => `
                            <label class="tool ${selectedTools.has(tool.name) ? 'selected' : ''}" data-name="${tool.name}">
                                <input type="checkbox" 
                                    ${selectedTools.has(tool.name) ? 'checked' : ''} 
                                    onchange="toggleTool('${tool.name}', this.checked)">
                                <div class="tool-info">
                                    <div class="tool-name">${tool.name}</div>
                                    <div class="tool-description">${tool.description || ''}</div>
                                </div>
                            </label>
                        `).join('')}
                    </div>
                `;
                container.appendChild(div);
                
                cat.tools.forEach(t => allTools.push(t.name));
            });
        }
        
        function toggleTool(name, checked) {
            if (checked) {
                if (selectedTools.size >= MAX_TOOLS) {
                    alert(`Maximum ${MAX_TOOLS} tools can be selected!`);
                    // Find the checkbox and uncheck it
                    const checkbox = document.querySelector(`.tool[data-name="${name}"] input`);
                    if(checkbox) checkbox.checked = false;
                    return;
                }
                selectedTools.add(name);
            } else {
                selectedTools.delete(name);
            }
            
            // Update visual
            const toolEl = document.querySelector(`.tool[data-name="${name}"]`);
            if (toolEl) {
                toolEl.classList.toggle('selected', checked);
            }
            
            updateStats();
        }

        function renderSelectedList() {
            const list = document.getElementById('selected-list');
            list.innerHTML = '';
            
            const sorted = Array.from(selectedTools).sort();
            
            sorted.forEach(name => {
                const li = document.createElement('li');
                li.className = 'selected-item';
                li.innerHTML = `
                    <span class="selected-name" title="${name}">${name}</span>
                    <span class="remove-btn" onclick="removeTool('${name}')">&times;</span>
                `;
                list.appendChild(li);
            });
            
            document.getElementById('sidebar-count').textContent = selectedTools.size;
        }

        function removeTool(name) {
            // Uncheck the main list item
            const toolEl = document.querySelector(`.tool[data-name="${name}"]`);
            if (toolEl) {
                toolEl.classList.remove('selected');
                toolEl.querySelector('input').checked = false;
            }
            
            selectedTools.delete(name);
            updateStats();
        }
        
        function updateStats() {
            const count = selectedTools.size;
            const remaining = MAX_TOOLS - count;
            
            document.getElementById('selected-count').textContent = count;
            document.getElementById('remaining').textContent = remaining;
            
            const remainingEl = document.getElementById('remaining');
            remainingEl.classList.remove('warning', 'danger', 'success');
            if (remaining <= 0) {
                remainingEl.classList.add('danger');
            } else if (remaining <= 5) {
                remainingEl.classList.add('warning');
            } else {
                remainingEl.classList.add('success');
            }
            
            document.getElementById('save-btn').disabled = count === 0;
            renderSelectedList();
        }
        
        function toggleCategory(header) {
            const tools = header.nextElementSibling;
            tools.style.display = tools.style.display === 'none' ? 'block' : 'none';
        }
        
        function selectAll() {
            document.querySelectorAll('.tool:not(.selected)').forEach(el => {
                if (selectedTools.size < MAX_TOOLS) {
                    const name = el.dataset.name;
                    if (el.style.display !== 'none') {
                        selectedTools.add(name);
                        el.classList.add('selected');
                        el.querySelector('input').checked = true;
                    }
                }
            });
            updateStats();
        }
        
        function deselectAll() {
            selectedTools.clear();
            document.querySelectorAll('.tool.selected').forEach(el => {
                el.classList.remove('selected');
                el.querySelector('input').checked = false;
            });
            updateStats();
        }
        
        async function saveProfile() {
            const name = document.getElementById('profile-name').value.trim() || 'default';
            currentProfileName = name;
            
            const response = await fetch(`/mcp-picker/api/profiles/${name}`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ tools: Array.from(selectedTools) })
            });
            
            const data = await response.json();
            
            if (data.success) {
                // Use current origin for configs (works for both localhost and public domain)
                const baseUrl = window.location.origin;
                currentEndpoint = `${baseUrl}${data.mcp_endpoint}`;
                
                // Update all config displays
                updateConfigs(name, currentEndpoint, data.tool_count);
                
                // Show config section
                document.getElementById('config-section').classList.add('show');
                
                // Refresh profiles dropdown
                await loadSavedProfiles();
            }
        }
        
        function updateConfigs(profileName, endpoint, toolCount) {
            // Gemini
             const geminiConfig = {
                "mcpServers": {
                    [`op-dbus-${profileName}`]: {
                        "url": endpoint,
                        "transport": "sse"
                    }
                }
            };
            document.getElementById('gemini-config').innerHTML = 
                `<button class="copy-json" onclick="copyJson('gemini-config')">Copy</button>` +
                syntaxHighlight(JSON.stringify(geminiConfig, null, 2));

            // Claude Desktop config
            const claudeConfig = {
                "mcpServers": {
                    [`op-dbus-${profileName}`]: {
                        "command": "curl",
                        "args": ["-X", "POST", "-H", "Content-Type: application/json", "-d", "@-", endpoint]
                    }
                }
            };
            document.getElementById('claude-config').innerHTML = 
                `<button class="copy-json" onclick="copyJson('claude-config')">Copy</button>` +
                syntaxHighlight(JSON.stringify(claudeConfig, null, 2));

            // Codex
             const codexConfig = {
                "mcpServers": {
                    [`op-dbus-${profileName}`]: {
                        "url": endpoint,
                        "transport": "sse"
                    }
                }
            };
            document.getElementById('codex-config').innerHTML = 
                `<button class="copy-json" onclick="copyJson('codex-config')">Copy</button>` +
                syntaxHighlight(JSON.stringify(codexConfig, null, 2));
            
            // Antigravity config
            const antigravityConfig = {
                "mcpServers": {
                    [`op-dbus-${profileName}`]: {
                        "serverUrl": endpoint
                    }
                }
            };
            document.getElementById('antigravity-config').innerHTML = 
                `<button class="copy-json" onclick="copyJson('antigravity-config')">Copy</button>` +
                syntaxHighlight(JSON.stringify(antigravityConfig, null, 2));
            
             // Cursor Config
            const cursorConfig = {
                "mcpServers": {
                    [`op-dbus-${profileName}`]: {
                        "url": endpoint,
                        "transport": "sse"
                    }
                }
            };
            document.getElementById('cursor-config').innerHTML = 
                `<button class="copy-json" onclick="copyJson('cursor-config')">Copy</button>` +
                syntaxHighlight(JSON.stringify(cursorConfig, null, 2));
            
            // Generic endpoint
            document.getElementById('endpoint-url').textContent = endpoint;
        }
        
        function syntaxHighlight(json) {
            return json.replace(/("(\\u[a-zA-Z0-9]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(true|false|null)\b|-?\d+(?:\.\d*)?(?:[eE][+\-]?\d+)?)/g, function (match) {
                let cls = 'json-number';
                if (/^"/.test(match)) {
                    if (/:$/.test(match)) {
                        cls = 'json-key';
                    } else {
                        cls = 'json-string';
                    }
                }
                return '<span class="' + cls + '">' + match + '</span>';
            });
        }
        
        function showTab(tabName) {
            // Update tab buttons
            document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
            document.querySelector(`.tab[onclick="showTab('${tabName}')"]`).classList.add('active');
            
            // Update tab content
            document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
            document.getElementById(`tab-${tabName}`).classList.add('active');
        }
        
        function copyJson(elementId) {
            const el = document.getElementById(elementId);
            const text = el.textContent.replace('Copy', '').trim();
            navigator.clipboard.writeText(text);
            
            const btn = el.querySelector('.copy-json');
            btn.textContent = 'Copied!';
            setTimeout(() => btn.textContent = 'Copy', 1500);
        }
        
        function copyEndpoint() {
            navigator.clipboard.writeText(currentEndpoint);
            alert('Endpoint URL copied to clipboard!');
        }
        
        // Search functionality
        document.getElementById('search').addEventListener('input', (e) => {
            const query = e.target.value.toLowerCase();
            document.querySelectorAll('.tool').forEach(el => {
                const name = el.dataset.name.toLowerCase();
                el.style.display = name.includes(query) ? 'flex' : 'none';
            });
        });
        
        init();
    </script>
</body>
</html>
"##;
