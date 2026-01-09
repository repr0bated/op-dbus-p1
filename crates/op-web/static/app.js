// MCP Control Center Application
class MCPControlCenter {
    constructor() {
        this.ws = null;
        this.tools = [];
        this.agents = [];
        this.services = [];
        this.logs = [];
        this.stats = {
            uptime: 0,
            requestCount: 0,
            activeAgents: 0,
            availableTools: 0
        };
        this.currentTool = null;
        this.activityFeed = [];
        this.reconnectAttempts = 0;
        this.maxReconnectAttempts = 5;
        
        this.init();
    }

    init() {
        this.setupWebSocket();
        this.setupEventListeners();
        this.setupNavigation();
        this.loadInitialData();
        this.startUpdateTimers();
        this.checkTheme();
    }

    // WebSocket Connection
    setupWebSocket() {
        // For now, show as connected since the chat server is working
        // WebSocket will be implemented for real-time updates in the future
        console.log('MCP Control Center initialized');
        this.updateConnectionStatus('connected');
        this.addActivity('MCP Control Center loaded - AI AI available');

        // Optional: Try to connect to WebSocket for real-time updates
        try {
            const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            const wsUrl = `${protocol}//${window.location.host}/ws`;

            this.ws = new WebSocket(wsUrl);

            this.ws.onopen = () => {
                console.log('WebSocket connected - real-time updates enabled');
                this.addActivity('Real-time updates connected');
            };

            this.ws.onmessage = (event) => {
                this.handleWebSocketMessage(JSON.parse(event.data));
            };

            this.ws.onerror = (error) => {
                console.log('WebSocket not available - using basic mode');
                // Don't change status to error since basic functionality works
            };

            this.ws.onclose = () => {
                console.log('WebSocket disconnected - continuing in basic mode');
                // Don't change status since basic functionality still works
            };
        } catch (error) {
            console.log('WebSocket initialization failed - using basic mode');
        }
    }

    attemptReconnect() {
        if (this.reconnectAttempts < this.maxReconnectAttempts) {
            this.reconnectAttempts++;
            const delay = Math.min(1000 * Math.pow(2, this.reconnectAttempts), 30000);
            console.log(`Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts}/${this.maxReconnectAttempts})`);
            setTimeout(() => this.setupWebSocket(), delay);
        } else {
            this.showToast('Connection lost. Please refresh the page.', 'error');
        }
    }

    handleWebSocketMessage(data) {
        switch (data.type) {
            case 'status':
                this.updateStats(data.data);
                break;
            case 'agent_update':
                this.updateAgents(data.data);
                break;
            case 'log':
                this.addLog(data.data);
                break;
            case 'activity':
                this.addActivity(data.message, data.level);
                break;
            case 'tool_result':
                this.handleToolResult(data.data);
                break;
            default:
                console.log('Unknown message type:', data.type);
        }
    }

    // Event Listeners
    setupEventListeners() {
        // Theme toggle
        document.getElementById('theme-toggle').addEventListener('click', () => {
            this.toggleTheme();
        });

        // Dashboard refresh
        document.getElementById('refresh-dashboard').addEventListener('click', () => {
            this.refreshDashboard();
        });

        // Tool search
        document.getElementById('tool-search').addEventListener('input', (e) => {
            this.filterTools(e.target.value);
        });

        // Log level filter
        document.getElementById('log-level').addEventListener('change', (e) => {
            this.filterLogs(e.target.value);
        });

        // Chat functionality
        const chatInput = document.getElementById('chat-input');
        if (chatInput) {
            chatInput.addEventListener('keypress', (e) => {
                if (e.key === 'Enter' && !e.shiftKey) {
                    e.preventDefault();
                    this.sendChatMessage();
                }
            });
        }

        const chatSendBtn = document.getElementById('chat-send-btn');
        if (chatSendBtn) {
            chatSendBtn.addEventListener('click', () => {
                this.sendChatMessage();
            });
        }
    }

    // Navigation
    setupNavigation() {
        const navLinks = document.querySelectorAll('.nav-link');
        navLinks.forEach(link => {
            link.addEventListener('click', (e) => {
                e.preventDefault();
                const target = link.getAttribute('href').substring(1);
                this.navigateTo(target);
            });
        });
    }

    navigateTo(section) {
        // Update nav
        document.querySelectorAll('.nav-link').forEach(link => {
            link.classList.remove('active');
        });
        document.querySelector(`.nav-link[href="#${section}"]`).classList.add('active');

        // Update sections
        document.querySelectorAll('.section').forEach(sec => {
            sec.classList.remove('active');
        });
        document.getElementById(section).classList.add('active');

        // Load section-specific data
        this.loadSectionData(section);
    }

    loadSectionData(section) {
        switch (section) {
            case 'tools':
                this.loadTools();
                break;
            case 'agents':
                this.loadAgents();
                break;
            case 'discovery':
                this.loadServices();
                break;
            case 'logs':
                this.loadLogs();
                break;
        }
    }

    // Initial Data Load
    async loadInitialData() {
        await this.loadStatus();
        await this.loadTools();
        await this.loadAgents();
        await this.loadServices(); // Load any previously discovered services
    }

    async loadStatus() {
        try {
            const response = await fetch('/api/stats');
            const data = await response.json();
            
            // Update stats display
            this.stats.uptime_secs = data.uptime || 0;
            this.stats.tools_count = data.tools_count || 0;
            this.stats.agents_count = data.agents_count || 0;
            this.stats.active_agents = data.active_agents || 0;
            
            this.updateStatDisplay('uptime', this.formatUptime(this.stats.uptime_secs));
            this.updateStatDisplay('available-tools', this.stats.tools_count);
            this.updateStatDisplay('active-agents', `${this.stats.active_agents} / ${this.stats.agents_count}`);
            
            console.log('Dashboard stats updated:', data);
        } catch (error) {
            console.error('Failed to load status:', error);
            this.showToast('Failed to load dashboard stats', 'error');
        }
    }

    async loadTools() {
        try {
            const response = await fetch('/api/tools');
            const data = await response.json();
            if (data.success) {
                this.tools = data.data.tools || [];
                this.renderTools();
                this.stats.availableTools = this.tools.length;
                this.updateStatDisplay('available-tools', this.tools.length);
            }
        } catch (error) {
            console.error('Failed to load tools:', error);
        }
    }

    async loadAgents() {
        try {
            const response = await fetch('/api/agents');
            const data = await response.json();
            if (data.success) {
                this.agents = data.data || [];
                this.renderAgents();
                this.stats.activeAgents = this.agents.length;
                this.updateStatDisplay('active-agents', this.agents.length);
            }
        } catch (error) {
            console.error('Failed to load agents:', error);
        }
    }

    async loadServices() {
        try {
            console.log('📡 Loading services from /api/discovery/services');
            const response = await fetch('/api/discovery/services');
            console.log('📥 Services response received:', response.status);
            const data = await response.json();
            console.log('📄 Services data:', data);

            if (data.success) {
                this.services = data.data || [];
                console.log(`📋 Loaded ${this.services.length} services`);
                console.log('🎨 Calling renderDiscoveryResults()...');
                this.renderDiscoveryResults();
                console.log('✅ Discovery results rendered');
            } else {
                console.error('❌ Services API returned error:', data.error);
            }
        } catch (error) {
            console.error('❌ Failed to load services:', error);
        }
    }

    async loadLogs() {
        try {
            const response = await fetch('/api/logs');
            const data = await response.json();
            if (data.success) {
                this.logs = data.data || [];
                this.renderLogs();
            }
        } catch (error) {
            console.error('Failed to load logs:', error);
            // Fall back to empty logs
            this.renderLogs();
        }
    }

    // Rendering Functions
    renderTools() {
        const grid = document.getElementById('tools-grid');
        grid.innerHTML = '';

        this.tools.forEach(tool => {
            const card = document.createElement('div');
            card.className = 'tool-card';
            card.innerHTML = `
                <div class="tool-name">${this.escapeHtml(tool.name)}</div>
                <div class="tool-description">${this.escapeHtml(tool.description || 'No description')}</div>
                <div class="tool-meta">
                    ${tool.inputSchema?.required?.map(req => 
                        `<span class="tool-tag">Required: ${req}</span>`
                    ).join('') || ''}
                </div>
            `;
            card.addEventListener('click', () => this.openToolTest(tool));
            grid.appendChild(card);
        });
    }

    renderAgents() {
        const tbody = document.getElementById('agents-tbody');
        tbody.innerHTML = '';

        if (this.agents.length === 0) {
            tbody.innerHTML = '<tr><td colspan="6" style="text-align:center;">No active agents</td></tr>';
            return;
        }

        this.agents.forEach(agent => {
            const row = document.createElement('tr');
            row.innerHTML = `
                <td><code>${this.escapeHtml(agent.id)}</code></td>
                <td>${this.escapeHtml(agent.agent_type)}</td>
                <td><span class="badge badge-${agent.status === 'running' ? 'success' : 'warning'}">${agent.status}</span></td>
                <td>${agent.task || '-'}</td>
                <td>${this.formatUptime(agent.uptime || 0)}</td>
                <td>
                    <button class="btn btn-sm" onclick="mcp.sendTaskToAgent('${agent.id}')">Task</button>
                    <button class="btn btn-sm btn-danger" onclick="mcp.killAgent('${agent.id}')">Kill</button>
                </td>
            `;
            tbody.appendChild(row);
        });
    }

    renderDiscoveryResults() {
        const container = document.getElementById('discovery-results');
        if (!container) {
            console.warn('Discovery results container not found');
            return;
        }

        const categories = this.groupServicesByCategory();

        container.innerHTML = `
            <div class="discovery-categories">
                ${Object.entries(categories).map(([category, services]) => `
                    <div class="category-section">
                        <h3>${category} (${Array.isArray(services) ? services.length : 0})</h3>
                        <div class="service-list">
                            ${(Array.isArray(services) ? services : []).map(service => `
                                <div class="service-item expandable" onclick="toggleServiceDetails('${service.name.replace(/[^a-zA-Z0-9]/g, '_')}')">
                                    <div class="service-header">
                                        <span class="service-name">${this.escapeHtml(service.name)}</span>
                                        <span class="service-status ${service.status || 'unknown'}">${service.status || 'unknown'}</span>
                                        <span class="service-type">${service.type || 'service'}</span>
                                    </div>
                                    <div class="service-path">${this.escapeHtml(service.path || '')}</div>
                                    <div class="service-description">${this.escapeHtml(service.description || 'No description available')}</div>

                                    <div class="service-details" id="details_${service.name.replace(/[^a-zA-Z0-9]/g, '_')}" style="display: none;">
                                        ${this.renderServiceDetails(service)}
                                    </div>
                                </div>
                            `).join('')}
                        </div>
                    </div>
                `).join('')}
            </div>
        `;
    }

    renderServiceDetails(service) {
        let details = '';

        // Status and basic info
        if (service.sub_status) {
            details += `<div class="detail-row"><strong>Sub-status:</strong> ${service.sub_status}</div>`;
        }

        // Resource usage
        if (service.resources) {
            details += `<div class="detail-row"><strong>Resources:</strong> CPU: ${service.resources.cpu}, Memory: ${service.resources.memory}</div>`;
        }

        if (service.memory_current) {
            details += `<div class="detail-row"><strong>Memory Usage:</strong> Current: ${service.memory_current}, Peak: ${service.memory_peak}</div>`;
        }

        if (service.cpu_usage_nsec) {
            details += `<div class="detail-row"><strong>CPU Time:</strong> ${Math.round(parseInt(service.cpu_usage_nsec) / 1000000)}ms</div>`;
        }

        // Systemd service specific
        if (service.type === 'systemd-service') {
            details += `<div class="detail-row"><strong>Loaded:</strong> ${service.loaded ? 'Yes' : 'No'}</div>`;
            details += `<div class="detail-row"><strong>Enabled:</strong> ${service.enabled ? 'Yes' : 'No'}</div>`;
            if (service.exec_main_pid) {
                details += `<div class="detail-row"><strong>Main PID:</strong> ${service.exec_main_pid}</div>`;
            }
            if (service.tasks_current) {
                details += `<div class="detail-row"><strong>Tasks:</strong> ${service.tasks_current}</div>`;
            }
            if (service.control_group) {
                details += `<div class="detail-row"><strong>Control Group:</strong> ${service.control_group}</div>`;
            }
        }

        // D-Bus service specific
        if (service.type === 'dbus-service') {
            if (service.interfaces && service.interfaces.length > 0) {
                details += `<div class="detail-row"><strong>Interfaces:</strong> ${service.interfaces.join(', ')}</div>`;
            }
            if (service.methods && service.methods.length > 0) {
                details += `<div class="detail-row"><strong>Methods:</strong> ${service.methods.slice(0, 3).join(', ')}${service.methods.length > 3 ? '...' : ''}</div>`;
            }
            if (service.properties) {
                details += `<div class="detail-section"><strong>Properties:</strong></div>`;
                Object.entries(service.properties).slice(0, 5).forEach(([key, value]) => {
                    const displayValue = Array.isArray(value) ? value.join(', ') : String(value);
                    details += `<div class="detail-row">• ${key}: ${displayValue}</div>`;
                });
            }
            if (service.dependencies) {
                details += `<div class="detail-row"><strong>Dependencies:</strong> ${service.dependencies.join(', ')}</div>`;
            }
        }

        // Network service specific
        if (service.type === 'network-service' || service.type === 'container-runtime') {
            if (service.version) {
                details += `<div class="detail-row"><strong>Version:</strong> ${service.version}</div>`;
            }
            if (service.socket_path) {
                details += `<div class="detail-row"><strong>Socket:</strong> ${service.socket_path}</div>`;
            }
            if (service.config_file) {
                details += `<div class="detail-row"><strong>Config:</strong> ${service.config_file}</div>`;
            }
        }

        // Docker specific
        if (service.name === 'docker') {
            details += `<div class="detail-row"><strong>Containers:</strong> ${service.containers_running} running, ${service.containers_stopped} stopped</div>`;
            details += `<div class="detail-row"><strong>Images:</strong> ${service.images_count}</div>`;
            details += `<div class="detail-row"><strong>Storage Driver:</strong> ${service.storage_driver}</div>`;
            details += `<div class="detail-row"><strong>Data Usage:</strong> ${service.data_space_used} used, ${service.data_space_available} available</div>`;
        }

        // Netmaker specific
        if (service.name === 'netmaker') {
            details += `<div class="detail-row"><strong>Peers:</strong> ${service.peers}</div>`;
            details += `<div class="detail-row"><strong>Networks:</strong> ${service.networks.join(', ')}</div>`;
            details += `<div class="detail-row"><strong>Listen Port:</strong> ${service.listen_port}</div>`;
            details += `<div class="detail-row"><strong>MTU:</strong> ${service.mtu}</div>`;
        }

        // OVS specific
        if (service.name === 'openvswitch.service') {
            details += `<div class="detail-row"><strong>OVS Version:</strong> ${service.ovs_version}</div>`;
            details += `<div class="detail-row"><strong>Bridges:</strong> ${service.bridge_count}</div>`;
            details += `<div class="detail-row"><strong>Ports:</strong> ${service.port_count}</div>`;
            details += `<div class="detail-row"><strong>Flows:</strong> ${service.flow_count}</div>`;
        }

        // Chrony specific
        if (service.name === 'chrony.service') {
            details += `<div class="detail-row"><strong>NTP Sources:</strong> ${service.ntp_sources.join(', ')}</div>`;
            details += `<div class="detail-row"><strong>Stratum:</strong> ${service.stratum}</div>`;
            details += `<div class="detail-row"><strong>Offset:</strong> ${service.offset}</div>`;
        }

        // Filesystem specific
        if (service.type === 'kernel-filesystem') {
            details += `<div class="detail-row"><strong>Mount Point:</strong> ${service.mount_point}</div>`;
            details += `<div class="detail-row"><strong>Filesystem:</strong> ${service.filesystem_type}</div>`;
            details += `<div class="detail-row"><strong>Mount Options:</strong> ${service.mount_options}</div>`;

            if (service.name === '/proc') {
                details += `<div class="detail-row"><strong>Processes:</strong> ${service.process_count}</div>`;
                details += `<div class="detail-row"><strong>Threads:</strong> ${service.thread_count}</div>`;
                details += `<div class="detail-row"><strong>Kernel:</strong> ${service.kernel_version}</div>`;
                details += `<div class="detail-row"><strong>Uptime:</strong> ${Math.round(service.uptime_seconds / 3600)} hours</div>`;
                details += `<div class="detail-row"><strong>Load Average:</strong> ${service.load_average.join(', ')}</div>`;
            }

            if (service.name === '/sys') {
                details += `<div class="detail-row"><strong>Subsystems:</strong> ${service.subsystem_count}</div>`;
                details += `<div class="detail-row"><strong>Devices:</strong> ${service.device_count}</div>`;
                details += `<div class="detail-row"><strong>Drivers:</strong> ${service.driver_count}</div>`;
                details += `<div class="detail-row"><strong>Bus Types:</strong> ${service.bus_types.join(', ')}</div>`;
            }

            if (service.name === '/dev') {
                details += `<div class="detail-row"><strong>Device Nodes:</strong> ${service.device_nodes}</div>`;
                details += `<div class="detail-row"><strong>Block Devices:</strong> ${service.block_devices}</div>`;
                details += `<div class="detail-row"><strong>Character Devices:</strong> ${service.character_devices}</div>`;
                details += `<div class="detail-row"><strong>Mounted Devices:</strong> ${service.mounted_devices.join(', ')}</div>`;
            }
        }

        return `<div class="service-detail-content">${details}</div>`;
    }

    renderLogs() {
        const container = document.getElementById('logs-container');
        container.innerHTML = '';

        this.logs.forEach(log => {
            const entry = document.createElement('div');
            entry.className = `log-entry log-${log.level}`;
            entry.innerHTML = `
                <span class="log-timestamp">${this.formatTime(log.timestamp)}</span>
                <span class="log-level ${log.level}">${log.level.toUpperCase()}</span>
                <span class="log-message">${this.escapeHtml(log.message)}</span>
            `;
            container.appendChild(entry);
        });

        // Auto-scroll to bottom
        container.scrollTop = container.scrollHeight;
    }

    // Tool Testing
    openToolTest(tool) {
        this.currentTool = tool;
        document.getElementById('test-tool-name').textContent = tool.name;
        
        // Set default parameters based on schema
        const params = {};
        if (tool.inputSchema?.properties) {
            Object.entries(tool.inputSchema.properties).forEach(([key, schema]) => {
                if (schema.default !== undefined) {
                    params[key] = schema.default;
                } else if (schema.type === 'string') {
                    params[key] = '';
                } else if (schema.type === 'number') {
                    params[key] = 0;
                } else if (schema.type === 'boolean') {
                    params[key] = false;
                } else if (schema.type === 'array') {
                    params[key] = [];
                } else if (schema.type === 'object') {
                    params[key] = {};
                }
            });
        }
        
        document.getElementById('tool-params').value = JSON.stringify(params, null, 2);
        document.getElementById('tool-result').textContent = '';
        document.getElementById('tool-test-panel').style.display = 'block';
    }

    closeToolTest() {
        document.getElementById('tool-test-panel').style.display = 'none';
        this.currentTool = null;
    }

    clearToolTest() {
        document.getElementById('tool-params').value = '{}';
        document.getElementById('tool-result').textContent = '';
    }

    async executeToolTest() {
        if (!this.currentTool) return;

        const paramsText = document.getElementById('tool-params').value;
        let params;
        try {
            params = JSON.parse(paramsText);
        } catch (e) {
            this.showToast('Invalid JSON parameters', 'error');
            return;
        }

        const resultEl = document.getElementById('tool-result');
        resultEl.textContent = 'Executing...';

        try {
            const response = await fetch(`/api/tools/${this.currentTool.name}`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(params)
            });
            
            const data = await response.json();
            
            if (data.success) {
                resultEl.textContent = JSON.stringify(data.data, null, 2);
                this.showToast('Tool executed successfully', 'success');
            } else {
                resultEl.textContent = `Error: ${data.error}`;
                this.showToast(`Execution failed: ${data.error}`, 'error');
            }
        } catch (error) {
            resultEl.textContent = `Error: ${error.message}`;
            this.showToast('Failed to execute tool', 'error');
        }
    }

    // Agent Management
    showSpawnAgent() {
        document.getElementById('spawn-agent-modal').style.display = 'flex';
    }

    hideSpawnAgent() {
        document.getElementById('spawn-agent-modal').style.display = 'none';
    }

    async spawnAgent() {
        const agentType = document.getElementById('agent-type').value;
        const configText = document.getElementById('agent-config').value;
        
        let config;
        try {
            config = JSON.parse(configText || '{}');
        } catch (e) {
            this.showToast('Invalid JSON configuration', 'error');
            return;
        }

        try {
            const response = await fetch('/api/agents', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ type: agentType, config })
            });
            
            const data = await response.json();
            
            if (data.success) {
                this.showToast(`Agent spawned: ${data.data.agent_id}`, 'success');
                this.hideSpawnAgent();
                this.loadAgents();
            } else {
                this.showToast(`Failed to spawn agent: ${data.error}`, 'error');
            }
        } catch (error) {
            this.showToast('Failed to spawn agent', 'error');
        }
    }

    async killAgent(agentId) {
        if (!confirm('Are you sure you want to kill this agent?')) return;

        try {
            const response = await fetch(`/api/agents/${agentId}`, {
                method: 'DELETE'
            });
            
            const data = await response.json();
            
            if (data.success) {
                this.showToast('Agent terminated', 'success');
                this.loadAgents();
            } else {
                this.showToast(`Failed to kill agent: ${data.error}`, 'error');
            }
        } catch (error) {
            this.showToast('Failed to kill agent', 'error');
        }
    }

    // Discovery
    async runDiscovery() {
        console.log('🔍 Starting service discovery...');

        // Show loading state in discovery results
        const container = document.getElementById('discovery-results');
        if (container) {
            container.innerHTML = `
                <div class="discovery-categories">
                    <div class="category-section">
                        <h3>🔍 Discovering Services...</h3>
                        <div class="service-list">
                            <div class="service-item">
                                <span class="service-name">Scanning D-Bus services...</span>
                                <span class="service-path">Please wait</span>
                            </div>
                        </div>
                    </div>
                </div>
            `;
        }

        this.showToast('Running service discovery...', 'info');

        try {
            console.log('📡 Making API call to /api/discovery/run');
            const response = await fetch('/api/discovery/run', {
                method: 'POST'
            });

            console.log('📥 Response received:', response.status);
            const data = await response.json();
            console.log('📄 Response data:', data);

            if (data.success) {
                this.showToast(`Discovered ${data.data.count} services`, 'success');
                console.log(`✅ Discovery successful: ${data.data.count} services found`);

                // Use the discovery results directly instead of making another API call
                this.services = data.data.services || [];
                console.log(`📋 Using discovery results: ${this.services.length} services`);

                // Render immediately
                this.renderDiscoveryResults();
                console.log('✅ Discovery results rendered');
            } else {
                console.error('❌ Discovery API returned error:', data.error);
                this.showToast(`Discovery failed: ${data.error}`, 'error');
            }
        } catch (error) {
            console.error('❌ Discovery network error:', error);
            this.showToast('Failed to run discovery', 'error');
        }
    }

    groupServicesByCategory() {
        const categories = {};

        // Safety check: ensure services is an array
        if (!this.services || !Array.isArray(this.services)) {
            console.warn('Services not initialized or not an array:', this.services);
            return categories;
        }

        this.services.forEach(service => {
            const category = service.category || 'Other';
            if (!categories[category]) {
                categories[category] = [];
            }
            categories[category].push(service);
        });

        return categories;
    }

    // Update Functions
    updateStats(stats) {
        this.stats = { ...this.stats, ...stats };
        
        this.updateStatDisplay('uptime', this.formatUptime(stats.uptime_secs));
        this.updateStatDisplay('request-count', stats.request_count);
        this.updateStatDisplay('active-agents', stats.active_agents?.length || 0);
    }

    updateStatDisplay(id, value) {
        const element = document.getElementById(id);
        if (element) {
            element.textContent = value;
        }
    }

    updateConnectionStatus(status) {
        const statusEl = document.getElementById('connection-status');
        statusEl.className = `connection-status ${status}`;
        
        const statusText = statusEl.querySelector('.status-text');
        switch (status) {
            case 'connected':
                statusText.textContent = 'Connected';
                break;
            case 'disconnected':
                statusText.textContent = 'Disconnected';
                break;
            case 'error':
                statusText.textContent = 'Error';
                break;
            default:
                statusText.textContent = 'Connecting...';
        }
    }

    updateAgents(agents) {
        this.agents = agents;
        if (document.querySelector('.section#agents.active')) {
            this.renderAgents();
        }
        this.updateStatDisplay('active-agents', agents.length);
    }

    // Activity Feed
    addActivity(message, level = 'info') {
        const activity = {
            timestamp: new Date(),
            message,
            level
        };
        
        this.activityFeed.unshift(activity);
        if (this.activityFeed.length > 100) {
            this.activityFeed.pop();
        }
        
        this.renderActivityFeed();
    }

    renderActivityFeed() {
        const container = document.getElementById('activity-feed');
        
        const html = this.activityFeed.slice(0, 20).map(item => `
            <div class="feed-item">
                <span class="feed-time">${this.formatTime(item.timestamp)}</span>
                <span class="feed-message">${this.escapeHtml(item.message)}</span>
            </div>
        `).join('');
        
        container.innerHTML = html;
    }

    // Logs
    addLog(log) {
        this.logs.push({
            timestamp: new Date(),
            ...log
        });
        
        if (this.logs.length > 1000) {
            this.logs.shift();
        }
        
        if (document.querySelector('.section#logs.active')) {
            this.renderLogs();
        }
    }

    filterLogs(level) {
        // Implementation for log filtering
        const filtered = level === 'all' 
            ? this.logs 
            : this.logs.filter(log => log.level === level);
        
        // Re-render with filtered logs
        const tempLogs = this.logs;
        this.logs = filtered;
        this.renderLogs();
        this.logs = tempLogs;
    }

    clearLogs() {
        this.logs = [];
        this.renderLogs();
    }

    downloadLogs() {
        const content = this.logs.map(log => 
            `[${this.formatTime(log.timestamp)}] [${log.level}] ${log.message}`
        ).join('\n');
        
        const blob = new Blob([content], { type: 'text/plain' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `mcp-logs-${Date.now()}.txt`;
        a.click();
        URL.revokeObjectURL(url);
    }

    // Theme Management
    checkTheme() {
        const savedTheme = localStorage.getItem('theme') || 'dark';
        document.documentElement.setAttribute('data-theme', savedTheme);
        this.updateThemeToggle(savedTheme);
    }

    toggleTheme() {
        const currentTheme = document.documentElement.getAttribute('data-theme');
        const newTheme = currentTheme === 'dark' ? 'light' : 'dark';
        
        document.documentElement.setAttribute('data-theme', newTheme);
        localStorage.setItem('theme', newTheme);
        this.updateThemeToggle(newTheme);
    }

    updateThemeToggle(theme) {
        const sunIcon = document.querySelector('.sun-icon');
        const moonIcon = document.querySelector('.moon-icon');
        
        if (theme === 'dark') {
            sunIcon.style.display = 'block';
            moonIcon.style.display = 'none';
        } else {
            sunIcon.style.display = 'none';
            moonIcon.style.display = 'block';
        }
    }

    // Utilities
    formatUptime(seconds) {
        const hours = Math.floor(seconds / 3600);
        const minutes = Math.floor((seconds % 3600) / 60);
        const secs = seconds % 60;
        return `${hours.toString().padStart(2, '0')}:${minutes.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
    }

    formatTime(date) {
        if (!(date instanceof Date)) {
            date = new Date(date);
        }
        return date.toLocaleTimeString('en-US', { hour12: false });
    }

    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }

    showToast(message, type = 'info') {
        const container = document.getElementById('toast-container');
        
        const toast = document.createElement('div');
        toast.className = `toast ${type}`;
        toast.innerHTML = `
            <svg width="20" height="20" viewBox="0 0 20 20" fill="currentColor">
                ${this.getToastIcon(type)}
            </svg>
            <span>${this.escapeHtml(message)}</span>
        `;
        
        container.appendChild(toast);
        
        setTimeout(() => {
            toast.style.animation = 'slideOut 0.3s ease';
            setTimeout(() => toast.remove(), 300);
        }, 5000);
    }

    getToastIcon(type) {
        switch (type) {
            case 'success':
                return '<path d="M10 0C4.5 0 0 4.5 0 10s4.5 10 10 10 10-4.5 10-10S15.5 0 10 0zm4 8l-5 5-3-3 1.5-1.5L9 10l3.5-3.5L14 8z"/>';
            case 'error':
                return '<path d="M10 0C4.5 0 0 4.5 0 10s4.5 10 10 10 10-4.5 10-10S15.5 0 10 0zm5 13.5L13.5 15 10 11.5 6.5 15 5 13.5 8.5 10 5 6.5 6.5 5 10 8.5 13.5 5 15 6.5 11.5 10 15 13.5z"/>';
            case 'warning':
                return '<path d="M10 0C4.5 0 0 4.5 0 10s4.5 10 10 10 10-4.5 10-10S15.5 0 10 0zm0 15c-.6 0-1-.4-1-1s.4-1 1-1 1 .4 1 1-.4 1-1 1zm1-4H9V5h2v6z"/>';
            default:
                return '<path d="M10 0C4.5 0 0 4.5 0 10s4.5 10 10 10 10-4.5 10-10S15.5 0 10 0zm1 15H9v-2h2v2zm0-4H9V5h2v6z"/>';
        }
    }

    refreshDashboard() {
        this.showToast('Refreshing dashboard...', 'info');
        this.loadStatus();
        this.loadAgents();
    }

    startUpdateTimers() {
        // Update uptime every second
        setInterval(() => {
            this.stats.uptime_secs++;
            this.updateStatDisplay('uptime', this.formatUptime(this.stats.uptime_secs));
        }, 1000);

        // Refresh stats every 10 seconds
        setInterval(() => {
            this.loadStatus();
        }, 10000);
    }

    // Chat functionality
    async sendChatMessage() {
        const input = document.getElementById('chat-input');
        const sendBtn = document.getElementById('chat-send-btn');
        const typingIndicator = document.getElementById('typing-indicator');

        if (!input || !input.value.trim()) return;

        const message = input.value.trim();
        input.value = '';

        // Disable input while sending
        input.disabled = true;
        sendBtn.disabled = true;

        // Add user message to chat
        this.addChatMessage('user', message);

        // Show typing indicator
        if (typingIndicator) {
            typingIndicator.style.display = 'flex';
        }

        try {
            // Send to AI chat server
            const response = await fetch('/api/chat', {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                },
                body: JSON.stringify({ message }),
            });

            const data = await response.json();

            if (data.success) {
                this.addChatMessage('assistant', data.message);
            } else {
                this.addChatMessage('error', `Error: ${data.error || 'Unknown error'}`);
            }
        } catch (error) {
            console.error('Chat error:', error);
            this.addChatMessage('error', `Network error: ${error.message}`);
        } finally {
            // Re-enable input
            input.disabled = false;
            sendBtn.disabled = false;

            // Hide typing indicator
            if (typingIndicator) {
                typingIndicator.style.display = 'none';
            }
        }
    }

    addChatMessage(type, content) {
        const messagesContainer = document.getElementById('chat-messages');
        if (!messagesContainer) return;

        const messageDiv = document.createElement('div');
        messageDiv.className = `chat-message ${type}`;

        const avatar = type === 'user' ? '👤' : type === 'error' ? '❌' : '🤖';
        const timestamp = new Date().toLocaleTimeString();

        messageDiv.innerHTML = `
            <div class="message-avatar">${avatar}</div>
            <div class="message-content">
                <div class="message-text">${this.escapeHtml(content)}</div>
                <div class="message-time">${timestamp}</div>
            </div>
        `;

        messagesContainer.appendChild(messageDiv);
        messagesContainer.scrollTop = messagesContainer.scrollHeight;
    }

    clearChat() {
        const messagesContainer = document.getElementById('chat-messages');
        if (!messagesContainer) return;

        // Keep only the initial welcome message
        const welcomeMessage = messagesContainer.querySelector('.assistant');
        messagesContainer.innerHTML = '';
        if (welcomeMessage) {
            messagesContainer.appendChild(welcomeMessage);
        }
    }

    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }
}

// MCP Client - Thin JavaScript wrapper for Rust backend MCP server
// ALL MCP operations go through Rust backend - no direct HTTP/HTTPS connections from JavaScript
// Rust backend handles: remote server connections, authentication, protocol handling
class MCPClient {
    constructor() {
        this.requestId = 1;
        this.initialized = false;
        this.currentServer = null; // Currently configured remote server name (stored in Rust backend)
    }

    // Configure remote server via Rust backend endpoint
    // Rust backend handles: HTTPS connection, Bearer token, API key authentication
    // JavaScript only sends configuration to Rust, Rust does all the work
    async configure(serverUrl, options = {}) {
        if (!serverUrl || !serverUrl.trim()) {
            // Use local server
            this.currentServer = null;
            this.initialized = false;
            return;
        }

        const serverName = options.name || `remote-${Date.now()}`;
        const config = {
            name: serverName,
            url: serverUrl.trim(),
            bearer_token: options.bearerToken && options.bearerToken.trim() ? options.bearerToken.trim() : null,
            api_key: options.apiKey && options.apiKey.trim() ? options.apiKey.trim() : null,
            disabled: false
        };

        try {
            const response = await fetch('/api/mcp/remote/configure', {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json'
                },
                body: JSON.stringify(config)
            });

            const data = await response.json();
            
            if (data.success) {
                this.currentServer = serverName;
                this.initialized = false; // Will re-initialize on next request
                return data;
            } else {
                throw new Error(data.error || 'Failed to configure remote server');
            }
        } catch (error) {
            throw new Error(`Failed to configure remote MCP server: ${error.message}`);
        }
    }

    // All MCP requests go through Rust backend
    // Rust handles: local MCP server, remote HTTPS servers, authentication, protocol
    // JavaScript is just a thin API client - no direct MCP protocol handling
    async request(method, params = {}) {
        // Tell Rust backend which server to use (if remote server configured)
        const headers = {
            'Content-Type': 'application/json'
        };
        
        if (this.currentServer) {
            // Rust backend will route to the configured remote server
            headers['X-MCP-Server'] = this.currentServer;
        }

        // All requests go to Rust backend - Rust handles everything
        const response = await fetch('/api/mcp', {
            method: 'POST',
            headers: headers,
            body: JSON.stringify({
                jsonrpc: '2.0',
                id: this.requestId++,
                method: method,
                params: params
            })
        });

        if (!response.ok) {
            throw new Error(`HTTP ${response.status}: ${response.statusText}`);
        }

        const data = await response.json();
        
        if (data.error) {
            throw new Error(data.error.message || 'MCP request failed');
        }
        
        return data.result;
    }

    // All methods below are thin wrappers - they just call Rust backend
    // Rust backend handles: MCP protocol, remote connections, tool execution
    
    async initialize() {
        if (this.initialized) return;
        
        // Rust backend handles MCP initialization
        const result = await this.request('initialize', {
            protocolVersion: '2024-11-05',
            capabilities: {},
            clientInfo: {
                name: 'browser-mcp-client',
                version: '1.0.0'
            }
        });
        
        this.initialized = true;
        return result;
    }

    // List tools - Rust backend queries local or remote MCP server
    async listTools() {
        const result = await this.request('tools/list');
        return result.tools || [];
    }

    // Call tool - Rust backend executes on local or remote MCP server
    async callTool(name, arguments_ = {}) {
        const result = await this.request('tools/call', {
            name: name,
            arguments: arguments_
        });
        return result;
    }

    // Ping - Rust backend handles the ping
    async ping() {
        return await this.request('ping');
    }
}

// Global instance
window.mcp = new MCPControlCenter();

// Add MCP client to global scope
window.mcpClient = new MCPClient();

// Load saved MCP configuration from Rust backend
// All configuration is stored and managed by Rust backend
async function loadMCPConfig() {
    const savedUrl = localStorage.getItem('mcp_server_url');
    const savedBearerToken = localStorage.getItem('mcp_bearer_token');
    const savedApiKey = localStorage.getItem('mcp_api_key');
    
    // Load configurations from Rust backend (Rust is the source of truth)
    try {
        const response = await fetch('/api/mcp/remote/configs');
        const data = await response.json();
        
        if (data.success && data.data && data.data.length > 0) {
            // Use the first configured remote server from Rust backend
            const config = data.data[0];
            if (savedUrl === config.url) {
                // Configure via Rust backend
                await window.mcpClient.configure(config.url, {
                    name: config.name,
                    bearerToken: config.bearer_token || null,
                    apiKey: config.api_key || null
                });
            }
        } else if (savedUrl) {
            // Configure new server via Rust backend
            await window.mcpClient.configure(savedUrl, {
                name: `saved-${Date.now()}`,
                bearerToken: savedBearerToken || null,
                apiKey: savedApiKey || null
            });
        }
    } catch (error) {
        console.warn('Failed to load MCP configs from Rust backend:', error);
    }
    
    // Populate modal fields (UI only - Rust backend is source of truth)
    if (document.getElementById('mcp-server-url')) {
        document.getElementById('mcp-server-url').value = savedUrl || '';
        document.getElementById('mcp-bearer-token').value = savedBearerToken || '';
        document.getElementById('mcp-api-key').value = savedApiKey || '';
    }
}

// Show MCP configuration modal
window.showMCPConfig = function() {
    loadMCPConfig();
    document.getElementById('mcp-config-modal').style.display = 'flex';
};

// Hide MCP configuration modal
window.hideMCPConfig = function() {
    document.getElementById('mcp-config-modal').style.display = 'none';
};

// Save MCP configuration - ALL configuration goes to Rust backend
// Rust backend handles: HTTPS connections, authentication, remote server management
window.saveMCPConfig = async function() {
    const serverUrl = document.getElementById('mcp-server-url').value.trim();
    const bearerToken = document.getElementById('mcp-bearer-token').value.trim();
    const apiKey = document.getElementById('mcp-api-key').value.trim();
    const serverName = document.getElementById('mcp-server-name')?.value.trim() || `server-${Date.now()}`;
    
    // Save to localStorage for UI persistence only
    if (serverUrl) {
        localStorage.setItem('mcp_server_url', serverUrl);
    } else {
        localStorage.removeItem('mcp_server_url');
    }
    
    // Only save tokens if provided (they're optional)
    if (bearerToken) {
        localStorage.setItem('mcp_bearer_token', bearerToken);
    } else {
        localStorage.removeItem('mcp_bearer_token');
    }
    
    if (apiKey) {
        localStorage.setItem('mcp_api_key', apiKey);
    } else {
        localStorage.removeItem('mcp_api_key');
    }
    
    try {
        // Configure via Rust backend - Rust handles all HTTP/HTTPS connections
        if (serverUrl) {
            await window.mcpClient.configure(serverUrl, {
                name: serverName,
                bearerToken: bearerToken || null,  // null if not provided
                apiKey: apiKey || null              // null if not provided
            });
        } else {
            // Use local server (handled by Rust)
            await window.mcpClient.configure('', {});
        }
        
        // Reset initialization
        window.mcpClient.initialized = false;
        
        // Show success message
        window.mcp.showToast('MCP configuration saved!', 'success');
        hideMCPConfig();
        
        // Try to initialize (via Rust backend)
        await window.mcpClient.initialize();
        window.mcp.showToast('MCP client connected!', 'success');
    } catch (err) {
        window.mcp.showToast(`MCP configuration failed: ${err.message}`, 'error');
        console.error('MCP configuration error:', err);
    }
};

// Test MCP connection - ALL testing goes through Rust backend
// Rust backend handles: HTTPS connection, authentication, tool listing
window.testMCPConnection = async function() {
    const serverUrl = document.getElementById('mcp-server-url').value.trim();
    const bearerToken = document.getElementById('mcp-bearer-token').value.trim();
    const apiKey = document.getElementById('mcp-api-key').value.trim();
    const serverName = `test-${Date.now()}`;
    
    try {
        window.mcp.showToast('Testing MCP connection via Rust backend...', 'info');
        
        // Configure remote server via Rust backend (Rust handles HTTPS connection)
        if (serverUrl) {
            await window.mcpClient.configure(serverUrl, {
                name: serverName,
                bearerToken: bearerToken || null,
                apiKey: apiKey || null
            });
            
            // Test by listing tools (Rust backend queries remote server)
            const tools = await window.mcpClient.listTools();
            window.mcp.showToast(`✅ Connection successful! Found ${tools.length} tools.`, 'success');
        } else {
            // Test local server (handled by Rust)
            await window.mcpClient.configure('', {});
            const tools = await window.mcpClient.listTools();
            window.mcp.showToast(`✅ Local server connected! Found ${tools.length} tools.`, 'success');
        }
    } catch (error) {
        window.mcp.showToast(`❌ Connection failed: ${error.message}`, 'error');
        console.error('MCP connection test failed:', error);
    }
};

// Initialize MCP client on page load - ALL initialization via Rust backend
document.addEventListener('DOMContentLoaded', async () => {
    // Load saved configuration from Rust backend
    loadMCPConfig();
    
    try {
        // Initialize via Rust backend (Rust handles MCP protocol)
        await window.mcpClient.initialize();
        console.log('✅ MCP Client initialized via Rust backend');
    } catch (error) {
        console.warn('⚠️ MCP Client initialization failed:', error);
    }
});

// Global function to test MCP connection
window.testMCPClient = async () => {
    try {
        console.log('Testing MCP client...');
        
        // Initialize if not already
        if (!window.mcpClient.initialized) {
            await window.mcpClient.initialize();
        }
        
        // List tools
        const tools = await window.mcpClient.listTools();
        console.log('Available tools:', tools);
        
        // Show in chat
        const chatMessages = document.getElementById('chat-messages');
        if (chatMessages) {
            const messageDiv = document.createElement('div');
            messageDiv.className = 'chat-message system';
            messageDiv.innerHTML = `
                <div class="message-content">
                    <div class="message-text" style="color: var(--accent-blue);">
                        <strong>MCP Client Connected!</strong><br>
                        Found ${tools.length} available tools:<br>
                        ${tools.slice(0, 5).map(t => `• ${t.name || t.id}`).join('<br>')}
                        ${tools.length > 5 ? `<br>... and ${tools.length - 5} more` : ''}
                    </div>
                </div>
            `;
            chatMessages.appendChild(messageDiv);
            chatMessages.scrollTop = chatMessages.scrollHeight;
        }
        
        return tools;
    } catch (error) {
        console.error('MCP Client test failed:', error);
        alert(`MCP Client Error: ${error.message}`);
        throw error;
    }
};

// Global functions for inline handlers
window.closeToolTest = () => window.mcp.closeToolTest();
window.clearToolTest = () => window.mcp.clearToolTest();
window.executeToolTest = () => window.mcp.executeToolTest();
window.showSpawnAgent = () => window.mcp.showSpawnAgent();
window.hideSpawnAgent = () => window.mcp.hideSpawnAgent();
window.spawnAgent = () => window.mcp.spawnAgent();
window.runDiscovery = () => window.mcp.runDiscovery();
window.clearLogs = () => window.mcp.clearLogs();
window.downloadLogs = () => window.mcp.downloadLogs();
window.clearChat = () => window.mcp.clearChat();
window.toggleServiceDetails = (serviceId) => {
    const detailsElement = document.getElementById(`details_${serviceId}`);
    if (detailsElement) {
        const isVisible = detailsElement.style.display !== 'none';
        detailsElement.style.display = isVisible ? 'none' : 'block';

        // Toggle expand/collapse visual indicator
        const headerElement = detailsElement.parentElement;
        headerElement.classList.toggle('expanded', !isVisible);
    }
};

// Workflow visualization
let workflowNodes = [];
let workflowEdges = [];

window.generateWorkflow = async () => {
    console.log('🔗 Generating workflow visualization...');

    // Show loading
    const canvas = document.getElementById('workflow-canvas');
    canvas.innerHTML = '<div style="display: flex; align-items: center; justify-content: center; height: 100%;"><div>Loading workflow...</div></div>';

    try {
        // Get discovered services
        await this.loadServices();

        if (!this.services || this.services.length === 0) {
            canvas.innerHTML = '<div style="display: flex; align-items: center; justify-content: center; height: 100%; color: var(--text-secondary);">No services discovered. Run discovery first.</div>';
            return;
        }

        // Generate nodes and edges
        generateWorkflowData(this.services);

        // Render the workflow
        renderWorkflow();

        console.log(`✅ Generated workflow with ${workflowNodes.length} nodes and ${workflowEdges.length} edges`);

    } catch (error) {
        console.error('❌ Failed to generate workflow:', error);
        canvas.innerHTML = '<div style="display: flex; align-items: center; justify-content: center; height: 100%; color: #ef4444;">Failed to generate workflow</div>';
    }
};

window.resetWorkflowView = () => {
    const canvas = document.getElementById('workflow-canvas');
    canvas.innerHTML = `
        <div class="workflow-placeholder">
            <div class="placeholder-icon">🔗</div>
            <div class="placeholder-text">
                <h3>System Workflow Visualization</h3>
                <p>Click "Generate Workflow" to create a visual representation of your system's services, dependencies, and relationships.</p>
                <p>This will show how systemd services, D-Bus components, network services, and kernel subsystems interact.</p>
            </div>
        </div>
    `;
    workflowNodes = [];
    workflowEdges = [];
};

window.changeLayout = () => {
    if (workflowNodes.length > 0) {
        renderWorkflow();
    }
};

function generateWorkflowData(services) {
    workflowNodes = [];
    workflowEdges = [];

    const canvas = document.getElementById('workflow-canvas');
    const canvasWidth = canvas.offsetWidth || 800;
    const canvasHeight = canvas.offsetHeight || 600;

    // Create nodes
    services.forEach((service, index) => {
        const node = {
            id: service.name,
            label: service.name.split('.')[0].split('/').pop(), // Short name
            fullName: service.name,
            type: service.type || 'service',
            category: service.category,
            x: Math.random() * (canvasWidth - 120) + 60,
            y: Math.random() * (canvasHeight - 80) + 40,
            data: service
        };
        workflowNodes.push(node);
    });

    // Create edges based on relationships
    services.forEach(service => {
        // D-Bus services connect to systemd services
        if (service.type === 'dbus-service' && service.name.includes('systemd')) {
            services.forEach(otherService => {
                if (otherService.type === 'systemd-service') {
                    workflowEdges.push({
                        from: service.name,
                        to: otherService.name,
                        type: 'manages'
                    });
                }
            });
        }

        // Systemd services connect to their dependencies
        if (service.dependencies) {
            service.dependencies.forEach(dep => {
                const depService = services.find(s => s.name.includes(dep.replace('.service', '')));
                if (depService) {
                    workflowEdges.push({
                        from: service.name,
                        to: depService.name,
                        type: 'depends_on'
                    });
                }
            });
        }

        // Network services connect to network interfaces
        if (service.type === 'network-service') {
            services.forEach(otherService => {
                if (otherService.name === '/sys' || otherService.name.includes('net')) {
                    workflowEdges.push({
                        from: service.name,
                        to: otherService.name,
                        type: 'uses'
                    });
                }
            });
        }

        // Container services connect to systemd and kernel
        if (service.type === 'container-runtime') {
            services.forEach(otherService => {
                if (otherService.type === 'systemd-service' || otherService.name === '/proc') {
                    workflowEdges.push({
                        from: service.name,
                        to: otherService.name,
                        type: 'depends_on'
                    });
                }
            });
        }

        // Kernel filesystems connect to everything that uses them
        if (service.type === 'kernel-filesystem') {
            services.forEach(otherService => {
                if (otherService.type !== 'kernel-filesystem') {
                    workflowEdges.push({
                        from: otherService.name,
                        to: service.name,
                        type: 'accesses'
                    });
                }
            });
        }
    });

    // Apply selected layout
    applyLayout();
}

function applyLayout() {
    const layout = document.getElementById('workflow-layout').value;
    const canvas = document.getElementById('workflow-canvas');
    const canvasWidth = canvas.offsetWidth || 800;
    const canvasHeight = canvas.offsetHeight || 600;

    switch (layout) {
        case 'hierarchical':
            applyHierarchicalLayout(canvasWidth, canvasHeight);
            break;
        case 'circular':
            applyCircularLayout(canvasWidth, canvasHeight);
            break;
        case 'force':
        default:
            applyForceLayout(canvasWidth, canvasHeight);
            break;
    }
}

function applyHierarchicalLayout(width, height) {
    const categories = {};
    const categoryOrder = ['System', 'Kernel', 'Network', 'Application', 'Container'];

    // Group by category
    workflowNodes.forEach(node => {
        const category = node.category || 'Other';
        if (!categories[category]) categories[category] = [];
        categories[category].push(node);
    });

    let y = 60;
    categoryOrder.forEach(category => {
        if (categories[category]) {
            const nodes = categories[category];
            const xSpacing = width / (nodes.length + 1);

            nodes.forEach((node, index) => {
                node.x = xSpacing * (index + 1);
                node.y = y;
            });

            y += 120;
        }
    });
}

function applyCircularLayout(width, height) {
    const centerX = width / 2;
    const centerY = height / 2;
    const radius = Math.min(width, height) * 0.35;

    workflowNodes.forEach((node, index) => {
        const angle = (index / workflowNodes.length) * 2 * Math.PI;
        node.x = centerX + radius * Math.cos(angle);
        node.y = centerY + radius * Math.sin(angle);
    });
}

function applyForceLayout(width, height) {
    // Simple force-directed layout simulation
    const iterations = 50;
    const repulsion = 1000;
    const attraction = 0.01;

    for (let iter = 0; iter < iterations; iter++) {
        // Calculate repulsive forces between nodes
        workflowNodes.forEach(node => {
            node.fx = 0;
            node.fy = 0;
        });

        for (let i = 0; i < workflowNodes.length; i++) {
            for (let j = i + 1; j < workflowNodes.length; j++) {
                const node1 = workflowNodes[i];
                const node2 = workflowNodes[j];

                const dx = node2.x - node1.x;
                const dy = node2.y - node1.y;
                const distance = Math.sqrt(dx * dx + dy * dy) || 1;

                const force = repulsion / (distance * distance);
                const fx = (dx / distance) * force;
                const fy = (dy / distance) * force;

                node1.fx -= fx;
                node1.fy -= fy;
                node2.fx += fx;
                node2.fy += fy;
            }
        }

        // Calculate attractive forces along edges
        workflowEdges.forEach(edge => {
            const sourceNode = workflowNodes.find(n => n.id === edge.from);
            const targetNode = workflowNodes.find(n => n.id === edge.to);

            if (sourceNode && targetNode) {
                const dx = targetNode.x - sourceNode.x;
                const dy = targetNode.y - sourceNode.y;
                const distance = Math.sqrt(dx * dx + dy * dy) || 1;

                const force = attraction * distance;
                const fx = (dx / distance) * force;
                const fy = (dy / distance) * force;

                sourceNode.fx += fx;
                sourceNode.fy += fy;
                targetNode.fx -= fx;
                targetNode.fy -= fy;
            }
        });

        // Apply forces and constraints
        workflowNodes.forEach(node => {
            node.x += node.fx * 0.1;
            node.y += node.fy * 0.1;

            // Keep nodes within bounds
            node.x = Math.max(60, Math.min(width - 60, node.x));
            node.y = Math.max(40, Math.min(height - 40, node.y));
        });
    }
}

function renderWorkflow() {
    const canvas = document.getElementById('workflow-canvas');
    canvas.innerHTML = '';

    // Create tooltip
    const tooltip = document.createElement('div');
    tooltip.className = 'node-tooltip';
    tooltip.id = 'workflow-tooltip';
    canvas.appendChild(tooltip);

    // Render edges
    workflowEdges.forEach(edge => {
        const sourceNode = workflowNodes.find(n => n.id === edge.from);
        const targetNode = workflowNodes.find(n => n.id === edge.to);

        if (sourceNode && targetNode) {
            const edgeElement = document.createElement('div');
            edgeElement.className = 'workflow-edge';

            // Calculate line position and angle
            const x1 = sourceNode.x + 40; // Half node width
            const y1 = sourceNode.y + 20; // Half node height
            const x2 = targetNode.x + 40;
            const y2 = targetNode.y + 20;

            const length = Math.sqrt((x2 - x1) ** 2 + (y2 - y1) ** 2);
            const angle = Math.atan2(y2 - y1, x2 - x1) * 180 / Math.PI;

            edgeElement.style.width = length + 'px';
            edgeElement.style.height = '2px';
            edgeElement.style.background = 'var(--border)';
            edgeElement.style.position = 'absolute';
            edgeElement.style.left = x1 + 'px';
            edgeElement.style.top = y1 + 'px';
            edgeElement.style.transform = `rotate(${angle}deg)`;
            edgeElement.style.transformOrigin = '0 0';

            canvas.appendChild(edgeElement);
        }
    });

    // Render nodes
    workflowNodes.forEach(node => {
        const nodeElement = document.createElement('div');
        nodeElement.className = `workflow-node ${node.type}`;
        nodeElement.textContent = node.label;
        nodeElement.style.left = node.x + 'px';
        nodeElement.style.top = node.y + 'px';

        // Add hover tooltip
        nodeElement.addEventListener('mouseenter', (e) => {
            const tooltip = document.getElementById('workflow-tooltip');
            tooltip.innerHTML = `
                <strong>${node.fullName}</strong><br>
                Type: ${node.type}<br>
                Category: ${node.category}<br>
                Status: ${node.data.status || 'unknown'}
            `;
            tooltip.style.left = (e.pageX + 10) + 'px';
            tooltip.style.top = (e.pageY - 10) + 'px';
            tooltip.classList.add('show');
        });

        nodeElement.addEventListener('mouseleave', () => {
            document.getElementById('workflow-tooltip').classList.remove('show');
        });

        nodeElement.addEventListener('mousemove', (e) => {
            const tooltip = document.getElementById('workflow-tooltip');
            tooltip.style.left = (e.pageX + 10) + 'px';
            tooltip.style.top = (e.pageY - 10) + 'px';
        });

        canvas.appendChild(nodeElement);
    });
}

// Provider and Model Selection Functions
async function loadProvidersAndModels() {
    try {
        // First, get current provider and models
        const response = await fetch('/api/models');
        if (response.ok) {
            const data = await response.json();
            let currentProvider = data.provider;
            
            // Default to HuggingFace if no provider is set
            const validProviders = ['huggingface', 'gemini'];
            if (!currentProvider || !validProviders.includes(currentProvider)) {
                currentProvider = 'huggingface';
                // Switch to default provider if needed
                try {
                    await fetch('/api/provider/select', {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ provider: currentProvider })
                    });
                    // Reload models after switching provider
                    const modelsResponse = await fetch('/api/models');
                    if (modelsResponse.ok) {
                        const modelsData = await modelsResponse.json();
                        populateModelDropdown(modelsData);
                    }
                } catch (error) {
                    console.error('Error setting default provider:', error);
                }
            }
            
            // Populate provider dropdown with default order: HuggingFace, Gemini
            populateProviderDropdown(currentProvider);
            
            // Then fetch and populate models for current provider dynamically
            if (currentProvider) {
                const modelsResponse = await fetch(`/api/models/${currentProvider}`);
                if (modelsResponse.ok) {
                    const modelsData = await modelsResponse.json();
                    populateModelDropdownFromProvider(modelsData);
                }
            }
        } else {
            console.error('Failed to load models:', response.statusText);
        }
    } catch (error) {
        console.error('Error loading providers and models:', error);
    }
}

function populateProviderDropdown(currentProvider) {
    // Only show HuggingFace and Gemini (in that order)
    const providers = [
        { id: 'huggingface', name: 'HuggingFace' },
        { id: 'gemini', name: 'Gemini' }
    ];

    const select = document.getElementById('providerSelect');
    if (!select) return;

    // Clear existing options
    select.innerHTML = '';

    // Add providers in the specified order
    providers.forEach(provider => {
        const option = document.createElement('option');
        option.value = provider.id;
        option.textContent = provider.name;
        // Set selected based on current provider
        if (provider.id === currentProvider) {
            option.selected = true;
        }
        select.appendChild(option);
    });

    // Remove old listeners and add new one
    const newSelect = select.cloneNode(true);
    select.parentNode.replaceChild(newSelect, select);
    document.getElementById('providerSelect').addEventListener('change', (e) => handleProviderChange(e.target.value));
}

async function handleProviderChange(provider) {
    if (!provider) return;

    try {
        // Switch provider
        const response = await fetch('/api/provider/select', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ provider })
        });

        if (response.ok) {
            console.log(`Switched to provider: ${provider}`);

            // Fetch models dynamically for the selected provider
            const modelsResponse = await fetch(`/api/models/${provider}`);
            if (modelsResponse.ok) {
                const modelsData = await modelsResponse.json();
                populateModelDropdownFromProvider(modelsData);
            } else {
                console.error('Failed to fetch models for provider:', provider);
                const modelSelect = document.getElementById('modelSelect');
                if (modelSelect) {
                    modelSelect.innerHTML = '<option value="">No models available</option>';
                }
            }

            // Show notification in chat
            const chatMessages = document.getElementById('chat-messages');
            if (chatMessages) {
                const messageDiv = document.createElement('div');
                messageDiv.className = 'chat-message system';
                messageDiv.innerHTML = `
                    <div class="message-content">
                        <div class="message-text" style="color: var(--accent-blue); font-style: italic;">
                            Provider switched to: ${provider.toUpperCase()}
                        </div>
                    </div>
                `;
                chatMessages.appendChild(messageDiv);
                chatMessages.scrollTop = chatMessages.scrollHeight;
            }
        } else {
            console.error('Failed to switch provider:', response.statusText);
        }
    } catch (error) {
        console.error('Error switching provider:', error);
    }
}

async function loadModels() {
    try {
        const response = await fetch('/api/models');
        if (response.ok) {
            const data = await response.json();
            populateModelDropdown(data);
        } else {
            console.error('Failed to load models:', response.statusText);
        }
    } catch (error) {
        console.error('Error loading models:', error);
    }
}

function populateModelDropdown(data) {
    const { provider, currentModel, availableModels } = data;
    const select = document.getElementById('modelSelect');

    if (!select) return;

    // Clear existing options
    select.innerHTML = '';

    if (!availableModels || availableModels.length === 0) {
        const option = document.createElement('option');
        option.value = '';
        option.textContent = 'No models available';
        select.appendChild(option);
        return;
    }

    // Add models
    availableModels.forEach(model => {
        const option = document.createElement('option');
        option.value = model.id;
        option.textContent = model.name;
        option.selected = model.id === currentModel;
        select.appendChild(option);
    });

    // Remove old listeners and add new one
    const newSelect = select.cloneNode(true);
    select.parentNode.replaceChild(newSelect, select);
    document.getElementById('modelSelect').addEventListener('change', (e) => handleModelChange(e.target.value));
}

// Populate model dropdown from provider-specific model list
function populateModelDropdownFromProvider(data) {
    const select = document.getElementById('modelSelect');
    if (!select) return;
    
    // Clear existing options
    select.innerHTML = '';
    
    if (!data.models || data.models.length === 0) {
        select.innerHTML = '<option value="">No models available</option>';
        return;
    }
    
    // Add models
    data.models.forEach(model => {
        const option = document.createElement('option');
        option.value = model;
        option.textContent = model;
        select.appendChild(option);
    });
    
    // Select first model by default if available
    if (data.models.length > 0) {
        select.value = data.models[0];
        // Trigger model selection
        handleModelChange(data.models[0]);
    }
}

async function handleModelChange(modelId) {
    try {
        const response = await fetch('/api/models/select', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ model: modelId })
        });

        if (response.ok) {
            const data = await response.json();
            console.log(`Switched to model: ${data.modelName}`);

            // Show notification in chat
            const chatMessages = document.getElementById('chat-messages');
            if (chatMessages) {
                const messageDiv = document.createElement('div');
                messageDiv.className = 'chat-message system';
                messageDiv.innerHTML = `
                    <div class="message-content">
                        <div class="message-text" style="color: var(--accent-blue); font-style: italic;">
                            Model switched to: ${data.modelName}
                        </div>
                    </div>
                `;
                chatMessages.appendChild(messageDiv);
                chatMessages.scrollTop = chatMessages.scrollHeight;
            }
        } else {
            console.error('Failed to switch model:', response.statusText);
        }
    } catch (error) {
        console.error('Error switching model:', error);
    }
}

// Load providers and models when page loads
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', loadProvidersAndModels);
} else {
    loadProvidersAndModels();
}


async function handleModelChange(modelId) {
    try {
        const response = await fetch('/api/models/select', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ model: modelId })
        });

        if (response.ok) {
            const data = await response.json();
            console.log(`Switched to model: ${data.modelName}`);

            // Show notification in chat
            const chatMessages = document.getElementById('chat-messages');
            if (chatMessages) {
                const messageDiv = document.createElement('div');
                messageDiv.className = 'chat-message system';
                messageDiv.innerHTML = `
                    <div class="message-content">
                        <div class="message-text" style="color: var(--accent-blue); font-style: italic;">
                            Model switched to: ${data.modelName}
                        </div>
                    </div>
                `;
                chatMessages.appendChild(messageDiv);
                chatMessages.scrollTop = chatMessages.scrollHeight;
            }
        } else {
            console.error('Failed to switch model:', response.statusText);
        }
    } catch (error) {
        console.error('Error switching model:', error);
    }
}

// Load providers and models when page loads
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', loadProvidersAndModels);
} else {
    loadProvidersAndModels();
}
