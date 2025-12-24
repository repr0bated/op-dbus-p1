//! Page Components
//!
//! Main page views for the application.

use leptos::*;
use crate::api::*;
use crate::components::*;
use crate::state::*;

/// Chat page - main interface for natural language admin
#[component]
pub fn ChatPage() -> impl IntoView {
    let app_state = expect_context::<RwSignal<AppState>>();
    let (loading, set_loading) = create_signal(false);
    let (error, set_error) = create_signal::<Option<String>>(None);

    // Initialize connection on mount
    create_effect(move |_| {
        spawn_local(async move {
            let client = ApiClient::default();
            match client.health().await {
                Ok(_) => {
                    app_state.update(|s| {
                        s.connected = true;
                    });
                }
                Err(e) => {
                    app_state.update(|s| s.connected = false);
                    set_error.set(Some(format!("Connection failed: {}", e)));
                }
            }
        });
    });

    // Handle sending messages
    let on_send = move |message: String| {
        set_loading.set(true);
        set_error.set(None);

        // Add user message immediately
        let user_msg = ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::User,
            content: message.clone(),
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            tools_executed: vec![],
            tool_results: vec![],
        };
        app_state.update(|s| s.messages.push_back(user_msg));

        spawn_local(async move {
            let client = ApiClient::default();
            match client.chat(&message, None).await {
                Ok(response) => {
                    let assistant_msg = ChatMessage {
                        id: uuid::Uuid::new_v4().to_string(),
                        role: MessageRole::Assistant,
                        content: response.message,
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        tools_executed: vec![],
                        tool_results: vec![],
                    };
                    app_state.update(|s| s.messages.push_back(assistant_msg));
                }
                Err(e) => {
                    set_error.set(Some(e));
                }
            }
            set_loading.set(false);
        });
    };

    let messages = move || app_state.get().messages.clone();

    view! {
        <div class="chat-page">
            <div class="messages-container">
                {move || error.get().map(|e| view! { <ErrorDisplay message=e/> })}
                
                <div class="messages-list">
                    {move || messages().iter().map(|msg| view! {
                        <MessageBubble message=msg.clone()/>
                    }).collect_view()}
                    
                    {move || loading.get().then(|| view! {
                        <div class="typing-indicator">
                            <span>"Assistant is thinking..."</span>
                            <LoadingSpinner/>
                        </div>
                    })}
                </div>
            </div>
            
            <ChatInput on_send=on_send disabled=loading.get()/>
            
            <div class="chat-hints">
                <p>"Try: 'List OVS bridges', 'Restart nginx', 'Show network interfaces'"</p>
            </div>
        </div>
    }
}

/// Tools page - browse and execute tools
#[component]
pub fn ToolsPage() -> impl IntoView {
    let app_state = expect_context::<RwSignal<AppState>>();
    let (loading, set_loading) = create_signal(true);
    let (_error, _set_error) = create_signal::<Option<String>>(None);
    let (selected_tool, set_selected_tool) = create_signal::<Option<String>>(None);
    let (tool_args, set_tool_args) = create_signal(String::new());
    let (execution_result, set_execution_result) = create_signal::<Option<ToolExecutionResponse>>(None);

    // Load tools on mount
    create_effect(move |_| {
        spawn_local(async move {
            let client = ApiClient::default();
            match client.list_tools().await {
                Ok(tools) => {
                    // Convert ToolDefinition to ToolInfo
                    let tool_infos: Vec<ToolInfo> = tools.into_iter().map(|tool| ToolInfo {
                        name: tool.name,
                        description: tool.description,
                        category: tool.category,
                        input_schema: serde_json::json!({}),
                    }).collect();
                    app_state.update(|s| s.tools = tool_infos);
                    set_loading.set(false);
                }
                Err(_) => {
                    set_loading.set(false);
                }
            }
        });
    });

    let on_execute = move |tool_name: String| {
        set_selected_tool.set(Some(tool_name));
        set_tool_args.set("{}".to_string());
        set_execution_result.set(None);
    };

    let execute_selected = move |_| {
        if let Some(tool_name) = selected_tool.get() {
            let args_str = tool_args.get();
            let args: serde_json::Value = serde_json::from_str(&args_str)
                .unwrap_or(serde_json::json!({}));

            spawn_local(async move {
                let client = ApiClient::default();
                match client.execute_tool(&tool_name, args).await {
                    Ok(result) => {
                        set_execution_result.set(Some(result));
                    }
                    Err(_) => {}
                }
            });
        }
    };

    let tools = move || app_state.get().tools.clone();

    view! {
        <div class="tools-page">
            <h2>"Available Tools"</h2>
            
            {move || _error.get().map(|e| view! { <ErrorDisplay message=e/> })}
            
            {move || loading.get().then(|| view! { <LoadingSpinner/> })}
            
            <div class="tools-grid">
                {move || tools().into_iter().map(|tool| view! {
                    <ToolCard tool=tool on_execute=on_execute.clone()/>
                }).collect_view()}
            </div>
            
            // Tool execution modal
            {move || selected_tool.get().map(|tool_name| view! {
                <div class="tool-modal">
                    <div class="modal-content">
                        <h3>"Execute: " {&tool_name}</h3>
                        <label>"Arguments (JSON):"</label>
                        <textarea
                            class="args-input"
                            prop:value=tool_args
                            on:input=move |ev| set_tool_args.set(event_target_value(&ev))
                            rows=5
                        />
                        <div class="modal-actions">
                            <button on:click=execute_selected>"Execute"</button>
                            <button on:click=move |_| set_selected_tool.set(None)>"Cancel"</button>
                        </div>
                        
                        {move || execution_result.get().map(|result| view! {
                            <div class="execution-result" class:success=result.success class:error=!result.success>
                                <h4>{if result.success { "Success" } else { "Failed" }}</h4>
                                {result.result.map(|r| view! {
                                    <pre>{serde_json::to_string_pretty(&r).unwrap_or_default()}</pre>
                                })}
                                {result.error.map(|e| view! {
                                    <p class="error">{e}</p>
                                })}
                            </div>
                        })}
                    </div>
                </div>
            })}
        </div>
    }
}

/// Status page - system overview
#[component]
pub fn StatusPage() -> impl IntoView {
    let app_state = expect_context::<RwSignal<AppState>>();
    let (loading, set_loading) = create_signal(true);
    let (_error, _set_error) = create_signal::<Option<String>>(None);

    // Load status on mount and refresh periodically
    create_effect(move |_| {
        spawn_local(async move {
            // For now, just set loading to false since get_status doesn't exist
            set_loading.set(false);
        });
    });

    let refresh = move |_| {
        set_loading.set(true);
        spawn_local(async move {
            // For now, just set loading to false since get_status doesn't exist
            set_loading.set(false);
        });
    };

    let status = move || app_state.get().system_status.clone();

    view! {
        <div class="status-page">
            <div class="status-header">
                <h2>"System Status"</h2>
                <button on:click=refresh disabled=loading>
                    {move || if loading.get() { "Refreshing..." } else { "🔄 Refresh" }}
                </button>
            </div>
            
            {move || _error.get().map(|e| view! { <ErrorDisplay message=e/> })}
            
            {move || loading.get().then(|| view! { <LoadingSpinner/> })}
            
            {move || status().map(|s| view! {
                <div class="status-sections">
                    // System Info
                    <section class="status-section system-info">
                        <h3>"🖥️ System"</h3>
                        <div class="info-grid">
                            <div class="info-item">
                                <label>"Hostname"</label>
                                <span>{&s.system_info.hostname}</span>
                            </div>
                            <div class="info-item">
                                <label>"Kernel"</label>
                                <span>{&s.system_info.kernel}</span>
                            </div>
                            <div class="info-item">
                                <label>"Uptime"</label>
                                <span>{&s.system_info.uptime}</span>
                            </div>
                            <div class="info-item">
                                <label>"Load Average"</label>
                                <span>
                                    {format!("{:.2} {:.2} {:.2}", 
                                        s.system_info.load_average[0],
                                        s.system_info.load_average[1],
                                        s.system_info.load_average[2])}
                                </span>
                            </div>
                            <div class="info-item">
                                <label>"Memory"</label>
                                <span>{format!("{:.1}% used", s.system_info.memory_used_percent)}</span>
                            </div>
                            <div class="info-item">
                                <label>"CPUs"</label>
                                <span>{s.system_info.cpu_count}</span>
                            </div>
                        </div>
                    </section>
                    
                    // Services
                    <section class="status-section services">
                        <h3>"⚙️ Services"</h3>
                        <table class="services-table">
                            <thead>
                                <tr>
                                    <th>"Name"</th>
                                    <th>"State"</th>
                                    <th>"Sub-state"</th>
                                    <th>"Description"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {s.services.iter().map(|svc| view! {
                                    <ServiceRow service=svc.clone()/>
                                }).collect_view()}
                            </tbody>
                        </table>
                    </section>
                    
                    // Network
                    <section class="status-section network">
                        <h3>"🌐 Network Interfaces"</h3>
                        <div class="interfaces-grid">
                            {s.network.interfaces.iter().map(|iface| view! {
                                <InterfaceCard interface=iface.clone()/>
                            }).collect_view()}
                        </div>
                    </section>
                    
                    // OVS (if available)
                    {s.ovs.as_ref().filter(|o| o.available).map(|ovs| view! {
                        <section class="status-section ovs">
                            <h3>"🔀 Open vSwitch"</h3>
                            <div class="bridges-grid">
                                {ovs.bridges.iter().map(|br| view! {
                                    <OvsBridgeCard bridge=br.clone()/>
                                }).collect_view()}
                                {ovs.bridges.is_empty().then(|| view! {
                                    <p class="no-bridges">"No bridges configured"</p>
                                })}
                            </div>
                        </section>
                    })}
                </div>
            })}
        </div>
    }
}

/// Settings page
#[component]
pub fn SettingsPage() -> impl IntoView {
    let app_state = expect_context::<RwSignal<AppState>>();
    
    let provider = move || app_state.get().current_provider.clone();
    let model = move || app_state.get().current_model.clone();

    view! {
        <div class="settings-page">
            <h2>"⚙️ Settings"</h2>
            
            <section class="settings-section">
                <h3>"LLM Configuration"</h3>
                <div class="setting-item">
                    <label>"Provider"</label>
                    <span class="setting-value">{provider}</span>
                </div>
                <div class="setting-item">
                    <label>"Model"</label>
                    <span class="setting-value">{model}</span>
                </div>
            </section>
            
            <section class="settings-section">
                <h3>"About"</h3>
                <p>"op-dbus Admin is a natural language server administration interface."</p>
                <p>"Built with Rust, Leptos (WebAssembly), and native Linux protocols."</p>
                <p>"Uses D-Bus, OVSDB JSON-RPC, and rtnetlink - never CLI tools."</p>
            </section>
        </div>
    }
}
