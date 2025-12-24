// MCP Chat Interface JavaScript

class MCPChat {
    constructor() {
        this.ws = null;
        this.sessionId = null;
        this.messageCount = 0;
        this.toolsUsed = new Set();
        this.suggestions = [];
        this.selectedSuggestion = -1;
        this.commandHistory = [];
        this.historyIndex = -1;
        
        this.initElements();
        this.initWebSocket();
        this.initEventListeners();
        this.loadTheme();
        this.loadModels();
    }
    
    initElements() {
        this.elements = {
            connectionStatus: document.getElementById('connectionStatus'),
            statusDot: document.querySelector('.status-dot'),
            statusText: document.querySelector('.status-text'),
            themeToggle: document.getElementById('themeToggle'),
            messagesContainer: document.getElementById('messagesContainer'),
            chatInput: document.getElementById('chatInput'),
            sendButton: document.getElementById('sendButton'),
            suggestionsContainer: document.getElementById('suggestionsContainer'),
            quickCommands: document.getElementById('quickCommands'),
            toolTemplates: document.getElementById('toolTemplates'),
            sessionId: document.getElementById('sessionId'),
            messageCount: document.getElementById('messageCount'),
            toolsUsed: document.getElementById('toolsUsed'),
            templateModal: document.getElementById('templateModal'),
            modalTitle: document.getElementById('modalTitle'),
            modalClose: document.getElementById('modalClose'),
            modalCancel: document.getElementById('modalCancel'),
            modalExecute: document.getElementById('modalExecute'),
            templateForm: document.getElementById('templateForm'),
            providerSelect: document.getElementById('providerSelect'),
            modelSelect: document.getElementById('modelSelect')
        };
    }
    
    initWebSocket() {
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const wsUrl = `${protocol}//${window.location.host}/ws`;
        
        this.ws = new WebSocket(wsUrl);
        
        this.ws.onopen = () => {
            this.setConnectionStatus('connected');
            console.log('Connected to MCP Chat');
        };
        
        this.ws.onmessage = (event) => {
            const message = JSON.parse(event.data);
            this.handleMessage(message);
        };
        
        this.ws.onerror = (error) => {
            console.error('WebSocket error:', error);
            this.setConnectionStatus('error');
        };
        
        this.ws.onclose = () => {
            this.setConnectionStatus('disconnected');
            console.log('Disconnected from MCP Chat');
            // Attempt reconnection after 3 seconds
            setTimeout(() => this.initWebSocket(), 3000);
        };
    }
    
    initEventListeners() {
        // Send button
        this.elements.sendButton.addEventListener('click', () => this.sendMessage());
        
        // Chat input
        this.elements.chatInput.addEventListener('keydown', (e) => this.handleInputKeydown(e));
        this.elements.chatInput.addEventListener('input', () => this.handleInputChange());
        
        // Theme toggle
        this.elements.themeToggle.addEventListener('click', () => this.toggleTheme());
        
        // Quick commands
        this.elements.quickCommands.addEventListener('click', (e) => {
            if (e.target.classList.contains('quick-cmd')) {
                const cmd = e.target.dataset.cmd;
                this.elements.chatInput.value = cmd;
                this.sendMessage();
            }
        });
        
        // Tool templates
        this.elements.toolTemplates.addEventListener('click', (e) => {
            const card = e.target.closest('.template-card');
            if (card) {
                this.showTemplateModal(card.dataset.template);
            }
        });
        
        // Modal controls
        this.elements.modalClose.addEventListener('click', () => this.closeModal());
        this.elements.modalCancel.addEventListener('click', () => this.closeModal());
        this.elements.modalExecute.addEventListener('click', () => this.executeTemplate());
        
        // Click outside modal to close
        this.elements.templateModal.addEventListener('click', (e) => {
            if (e.target === this.elements.templateModal) {
                this.closeModal();
            }
        });
    }
    
    handleInputKeydown(e) {
        if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            this.sendMessage();
        } else if (e.key === 'Tab') {
            e.preventDefault();
            this.handleTabCompletion();
        } else if (e.key === 'ArrowUp') {
            if (this.suggestionsContainer.style.display === 'block') {
                e.preventDefault();
                this.navigateSuggestions(-1);
            } else if (this.elements.chatInput.selectionStart === 0) {
                e.preventDefault();
                this.navigateHistory(-1);
            }
        } else if (e.key === 'ArrowDown') {
            if (this.suggestionsContainer.style.display === 'block') {
                e.preventDefault();
                this.navigateSuggestions(1);
            } else {
                e.preventDefault();
                this.navigateHistory(1);
            }
        } else if (e.key === 'Escape') {
            this.hideSuggestions();
        }
    }
    
    handleInputChange() {
        // Auto-resize textarea
        this.elements.chatInput.style.height = 'auto';
        this.elements.chatInput.style.height = Math.min(this.elements.chatInput.scrollHeight, 120) + 'px';
        
        // Update suggestions
        const value = this.elements.chatInput.value;
        if (value.length > 0) {
            this.fetchSuggestions(value);
        } else {
            this.hideSuggestions();
        }
    }
    
    async fetchSuggestions(partial) {
        try {
            const response = await fetch('/chat/api/suggestions', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ partial })
            });
            
            if (response.ok) {
                this.suggestions = await response.json();
                this.showSuggestions();
            }
        } catch (error) {
            console.error('Failed to fetch suggestions:', error);
        }
    }
    
    showSuggestions() {
        if (this.suggestions.length === 0) {
            this.hideSuggestions();
            return;
        }
        
        this.elements.suggestionsContainer.innerHTML = '';
        this.suggestions.forEach((suggestion, index) => {
            const item = document.createElement('div');
            item.className = 'suggestion-item';
            if (index === this.selectedSuggestion) {
                item.classList.add('selected');
            }
            item.textContent = suggestion;
            item.addEventListener('click', () => {
                this.elements.chatInput.value = suggestion;
                this.hideSuggestions();
                this.elements.chatInput.focus();
            });
            this.elements.suggestionsContainer.appendChild(item);
        });
        
        this.elements.suggestionsContainer.style.display = 'block';
    }
    
    hideSuggestions() {
        this.elements.suggestionsContainer.style.display = 'none';
        this.selectedSuggestion = -1;
    }
    
    navigateSuggestions(direction) {
        const items = this.elements.suggestionsContainer.querySelectorAll('.suggestion-item');
        
        if (items.length === 0) return;
        
        // Remove previous selection
        if (this.selectedSuggestion >= 0 && this.selectedSuggestion < items.length) {
            items[this.selectedSuggestion].classList.remove('selected');
        }
        
        // Update selection
        this.selectedSuggestion += direction;
        if (this.selectedSuggestion < 0) {
            this.selectedSuggestion = items.length - 1;
        } else if (this.selectedSuggestion >= items.length) {
            this.selectedSuggestion = 0;
        }
        
        // Add new selection
        items[this.selectedSuggestion].classList.add('selected');
        items[this.selectedSuggestion].scrollIntoView({ block: 'nearest' });
    }
    
    handleTabCompletion() {
        const items = this.elements.suggestionsContainer.querySelectorAll('.suggestion-item');
        
        if (this.elements.suggestionsContainer.style.display === 'block' && items.length > 0) {
            // Use selected suggestion or first one
            const index = this.selectedSuggestion >= 0 ? this.selectedSuggestion : 0;
            this.elements.chatInput.value = this.suggestions[index];
            this.hideSuggestions();
        } else {
            // Trigger suggestions
            this.fetchSuggestions(this.elements.chatInput.value);
        }
    }
    
    navigateHistory(direction) {
        if (this.commandHistory.length === 0) return;
        
        if (direction === -1) {
            // Going back in history
            if (this.historyIndex < this.commandHistory.length - 1) {
                this.historyIndex++;
                this.elements.chatInput.value = this.commandHistory[this.commandHistory.length - 1 - this.historyIndex];
            }
        } else {
            // Going forward in history
            if (this.historyIndex > 0) {
                this.historyIndex--;
                this.elements.chatInput.value = this.commandHistory[this.commandHistory.length - 1 - this.historyIndex];
            } else if (this.historyIndex === 0) {
                this.historyIndex = -1;
                this.elements.chatInput.value = '';
            }
        }
    }
    
    sendMessage() {
        const message = this.elements.chatInput.value.trim();

        if (!message) return;

        if (this.ws && this.ws.readyState === WebSocket.OPEN) {
            // Send as JSON
            this.ws.send(JSON.stringify({
                type: 'chat',
                message: message,
                timestamp: Date.now()
            }));

            this.commandHistory.push(message);
            this.historyIndex = -1;

            // Keep history limited to 100 items
            if (this.commandHistory.length > 100) {
                this.commandHistory.shift();
            }

            // Add user message to UI
            this.addMessage({
                type: 'user',
                content: message,
                timestamp: Date.now() / 1000
            });

            this.elements.chatInput.value = '';
            this.elements.chatInput.style.height = 'auto';
            this.hideSuggestions();
        } else {
            this.addMessage({
                type: 'error',
                content: 'Not connected to server. Please wait...',
                timestamp: Date.now() / 1000
            });
        }
    }
    
    handleMessage(message) {
        // Extract session ID from system messages
        if (message.type === 'system' && message.content.includes('Session:')) {
            const match = message.content.match(/Session: ([a-f0-9]{8})/);
            if (match) {
                this.sessionId = match[1];
                this.elements.sessionId.textContent = this.sessionId;
            }
        }
        
        // Track tools used
        if (message.tools_used) {
            message.tools_used.forEach(tool => this.toolsUsed.add(tool));
            this.elements.toolsUsed.textContent = this.toolsUsed.size;
        }
        
        this.addMessage(message);
    }
    
    addMessage(message) {
        const messageDiv = document.createElement('div');
        messageDiv.className = `message ${message.type}`;
        
        // Message header
        const header = document.createElement('div');
        header.className = 'message-header';
        
        const avatar = document.createElement('div');
        avatar.className = 'message-avatar';
        avatar.textContent = this.getAvatarEmoji(message.type);
        
        const meta = document.createElement('div');
        meta.className = 'message-meta';
        
        const sender = document.createElement('div');
        sender.className = 'message-sender';
        sender.textContent = this.getSenderName(message.type);
        
        const time = document.createElement('div');
        time.className = 'message-time';
        time.textContent = this.formatTime(message.timestamp);
        
        meta.appendChild(sender);
        meta.appendChild(time);
        header.appendChild(avatar);
        header.appendChild(meta);
        
        // Message content
        const content = document.createElement('div');
        content.className = 'message-content';
        content.textContent = message.content;
        
        // Tool badges
        if (message.tools_used && message.tools_used.length > 0) {
            const tools = document.createElement('div');
            tools.className = 'message-tools';
            message.tools_used.forEach(tool => {
                const badge = document.createElement('span');
                badge.className = 'tool-badge';
                badge.textContent = `🔧 ${tool}`;
                tools.appendChild(badge);
            });
            content.appendChild(tools);
        }
        
        messageDiv.appendChild(header);
        messageDiv.appendChild(content);
        
        this.elements.messagesContainer.appendChild(messageDiv);
        this.elements.messagesContainer.scrollTop = this.elements.messagesContainer.scrollHeight;
        
        // Update message count
        this.messageCount++;
        this.elements.messageCount.textContent = this.messageCount;
    }
    
    getAvatarEmoji(type) {
        const emojis = {
            user: '👤',
            assistant: '🤖',
            system: '⚙️',
            error: '❌'
        };
        return emojis[type] || '❓';
    }
    
    getSenderName(type) {
        const names = {
            user: 'You',
            assistant: 'MCP Assistant',
            system: 'System',
            error: 'Error'
        };
        return names[type] || 'Unknown';
    }
    
    formatTime(timestamp) {
        const date = new Date(timestamp * 1000);
        return date.toLocaleTimeString();
    }
    
    setConnectionStatus(status) {
        const statusMap = {
            connected: { text: 'Connected', class: 'connected' },
            disconnected: { text: 'Disconnected', class: 'error' },
            error: { text: 'Error', class: 'error' },
            connecting: { text: 'Connecting...', class: '' }
        };
        
        const statusInfo = statusMap[status] || statusMap.connecting;
        this.elements.statusText.textContent = statusInfo.text;
        this.elements.statusDot.className = `status-dot ${statusInfo.class}`;
    }
    
    showTemplateModal(template) {
        const templates = {
            systemd: {
                title: 'Systemd Service Control',
                fields: [
                    { name: 'action', label: 'Action', type: 'select', options: ['status', 'start', 'stop', 'restart', 'enable', 'disable'], required: true },
                    { name: 'service', label: 'Service Name', type: 'text', placeholder: 'nginx.service', required: true }
                ]
            },
            file: {
                title: 'File Operations',
                fields: [
                    { name: 'action', label: 'Action', type: 'select', options: ['read', 'write', 'delete'], required: true },
                    { name: 'path', label: 'File Path', type: 'text', placeholder: '/path/to/file', required: true },
                    { name: 'content', label: 'Content (for write)', type: 'textarea', placeholder: 'File content...', rows: 5 }
                ]
            },
            network: {
                title: 'Network Management',
                fields: [
                    { name: 'action', label: 'Action', type: 'select', options: ['list', 'up', 'down', 'configure'], required: true },
                    { name: 'interface', label: 'Interface', type: 'text', placeholder: 'eth0' }
                ]
            },
            process: {
                title: 'Process Management',
                fields: [
                    { name: 'action', label: 'Action', type: 'select', options: ['list', 'kill', 'info'], required: true },
                    { name: 'pid', label: 'Process ID', type: 'number', placeholder: '1234' }
                ]
            }
        };
        
        const config = templates[template];
        if (!config) return;
        
        this.currentTemplate = template;
        this.elements.modalTitle.textContent = config.title;
        
        // Build form
        this.elements.templateForm.innerHTML = '';
        config.fields.forEach(field => {
            const group = document.createElement('div');
            group.className = 'form-group';
            
            const label = document.createElement('label');
            label.className = 'form-label';
            label.textContent = field.label;
            if (field.required) {
                label.textContent += ' *';
            }
            
            let input;
            if (field.type === 'select') {
                input = document.createElement('select');
                input.className = 'form-select';
                
                const defaultOption = document.createElement('option');
                defaultOption.value = '';
                defaultOption.textContent = 'Select...';
                input.appendChild(defaultOption);
                
                field.options.forEach(option => {
                    const opt = document.createElement('option');
                    opt.value = option;
                    opt.textContent = option;
                    input.appendChild(opt);
                });
            } else if (field.type === 'textarea') {
                input = document.createElement('textarea');
                input.className = 'form-textarea';
                input.rows = field.rows || 3;
            } else {
                input = document.createElement('input');
                input.className = 'form-input';
                input.type = field.type || 'text';
            }
            
            input.name = field.name;
            input.placeholder = field.placeholder || '';
            input.required = field.required || false;
            
            group.appendChild(label);
            group.appendChild(input);
            
            if (field.help) {
                const help = document.createElement('div');
                help.className = 'form-help';
                help.textContent = field.help;
                group.appendChild(help);
            }
            
            this.elements.templateForm.appendChild(group);
        });
        
        this.elements.templateModal.style.display = 'flex';
    }
    
    closeModal() {
        this.elements.templateModal.style.display = 'none';
        this.currentTemplate = null;
    }
    
    executeTemplate() {
        if (!this.currentTemplate) return;
        
        const formData = new FormData(this.elements.templateForm);
        const params = {};
        
        for (const [key, value] of formData.entries()) {
            if (value) {
                params[key] = value;
            }
        }
        
        // Build command
        let command = `run ${this.currentTemplate}`;
        
        // Format parameters
        const paramStr = Object.entries(params)
            .map(([key, value]) => `${key}="${value}"`)
            .join(' ');
        
        if (paramStr) {
            command += ` ${paramStr}`;
        }
        
        // Send command
        this.elements.chatInput.value = command;
        this.sendMessage();
        this.closeModal();
    }
    
    async loadModels() {
        try {
            const response = await fetch('/api/models');
            if (response.ok) {
                const data = await response.json();
                this.populateProviderDropdown(data.provider);
                this.populateModelDropdown(data);
            } else {
                console.error('Failed to load models:', response.statusText);
            }
        } catch (error) {
            console.error('Error loading models:', error);
        }
    }

    populateProviderDropdown(currentProvider) {
        const providers = [
            { id: 'ollama', name: 'Ollama Cloud' },
            { id: 'huggingface', name: 'HuggingFace' },
            { id: 'gemini', name: 'Google Gemini' },
            { id: 'grok', name: 'Grok (xAI)' }
        ];

        const select = this.elements.providerSelect;
        select.innerHTML = '';

        providers.forEach(provider => {
            const option = document.createElement('option');
            option.value = provider.id;
            option.textContent = provider.name;
            option.selected = provider.id === currentProvider;
            select.appendChild(option);
        });

        // Add change listener
        select.addEventListener('change', (e) => this.handleProviderChange(e.target.value));
    }

    populateModelDropdown(data) {
        const { provider, currentModel, availableModels } = data;
        const select = this.elements.modelSelect;

        // Clear existing options
        select.innerHTML = '';

        availableModels.forEach(model => {
            const option = document.createElement('option');
            option.value = model.id;
            option.textContent = model.name;
            option.selected = model.id === currentModel;
            select.appendChild(option);
        });

        // Add change listener (remove old listeners first)
        const newSelect = select.cloneNode(true);
        select.parentNode.replaceChild(newSelect, select);
        this.elements.modelSelect = newSelect;
        newSelect.addEventListener('change', (e) => this.handleModelChange(e.target.value));
    }

    async handleProviderChange(provider) {
        try {
            const response = await fetch('/api/provider/select', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ provider })
            });

            if (response.ok) {
                const data = await response.json();
                console.log(`Switched to provider: ${provider}`);

                // Reload models for new provider
                await this.loadModels();

                // Show notification
                this.addMessage({
                    type: 'system',
                    content: `Provider switched to: ${provider.toUpperCase()}`
                });
            } else {
                console.error('Failed to switch provider:', response.statusText);
                this.addMessage({
                    type: 'error',
                    content: 'Failed to switch provider'
                });
            }
        } catch (error) {
            console.error('Error switching provider:', error);
            this.addMessage({
                type: 'error',
                content: 'Error switching provider'
            });
        }
    }

    async handleModelChange(modelId) {
        try {
            const response = await fetch('/api/models/select', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ modelId })
            });

            if (response.ok) {
                const data = await response.json();
                console.log(`Switched to model: ${data.modelName}`);

                // Show notification
                this.addMessage({
                    type: 'system',
                    content: `Model switched to: ${data.modelName}`
                });
            } else {
                console.error('Failed to switch model:', response.statusText);
                this.addMessage({
                    type: 'error',
                    content: 'Failed to switch model'
                });
            }
        } catch (error) {
            console.error('Error switching model:', error);
            this.addMessage({
                type: 'error',
                content: 'Error switching model'
            });
        }
    }

    toggleTheme() {
        const isLight = document.body.classList.toggle('light-theme');
        this.elements.themeToggle.textContent = isLight ? '☀️' : '🌙';
        localStorage.setItem('theme', isLight ? 'light' : 'dark');
    }

    loadTheme() {
        const theme = localStorage.getItem('theme') || 'dark';
        if (theme === 'light') {
            document.body.classList.add('light-theme');
            this.elements.themeToggle.textContent = '☀️';
        }
    }
}

// Initialize chat when DOM is ready
document.addEventListener('DOMContentLoaded', () => {
    window.mcpChat = new MCPChat();
});