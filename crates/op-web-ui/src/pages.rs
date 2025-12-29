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

        spawn_local(async move {
            let client = ApiClient::default();
            if let Ok(status) = client.llm_status().await {
                app_state.update(|s| {
                    s.current_provider = status.provider;
                    s.current_model = status.model;
                });
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
            let session_id = app_state.get().current_session_id.clone();
            let model = app_state.get().current_model.clone();
            match client
                .chat(&message, session_id.as_deref(), Some(&model))
                .await
            {
                Ok(response) => {
                    app_state.update(|s| {
                        s.current_session_id = Some(response.session_id.clone());
                        s.current_provider = response.provider.clone();
                        s.current_model = response.model.clone();
                    });

                    if response.success {
                        let assistant_msg = ChatMessage {
                            id: uuid::Uuid::new_v4().to_string(),
                            role: MessageRole::Assistant,
                            content: response.message.unwrap_or_default(),
                            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                            tools_executed: response.tools_executed.clone(),
                            tool_results: vec![],
                        };
                        app_state.update(|s| s.messages.push_back(assistant_msg));
                    } else {
                        set_error.set(response.error.or_else(|| Some("Chat failed".to_string())));
                    }
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
                Ok(tool_list) => {
                    // Convert ToolDefinition to ToolInfo
                    let tool_infos: Vec<ToolInfo> = tool_list.tools.into_iter().map(|tool| ToolInfo {
                        name: tool.name,
                        description: tool.description,
                        category: tool.category,
                        input_schema: tool.input_schema.unwrap_or_else(|| serde_json::json!({})),
                    }).collect();
                    app_state.update(|s| s.tools = tool_infos);
                    set_loading.set(false);
                }
                Err(e) => {
                    _set_error.set(Some(e));
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
                    Err(e) => {
                        _set_error.set(Some(e));
                    }
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
    let (loading, set_loading) = create_signal(true);
    let (error, set_error) = create_signal::<Option<String>>(None);
    let (switching, set_switching) = create_signal(false);
    
    let provider = move || app_state.get().current_provider.clone();
    let model = move || app_state.get().current_model.clone();
    let models = move || app_state.get().available_models.clone();
    let providers = move || app_state.get().available_providers.clone();

    create_effect(move |_| {
        spawn_local(async move {
            let client = ApiClient::default();

            match client.llm_providers().await {
                Ok(response) => {
                    app_state.update(|s| {
                        s.available_providers = response.providers;
                        s.current_provider = response.current;
                    });
                }
                Err(e) => {
                    set_error.set(Some(e));
                }
            }

            match client.llm_models().await {
                Ok(response) => {
                    app_state.update(|s| {
                        s.available_models = response
                            .models
                            .unwrap_or_default()
                            .into_iter()
                            .map(|m| m.id)
                            .collect();
                        if let Some(current) = response.current {
                            s.current_model = current;
                        }
                    });
                }
                Err(e) => {
                    set_error.set(Some(e));
                }
            }

            if let Ok(status) = client.llm_status().await {
                app_state.update(|s| {
                    s.current_provider = status.provider;
                    s.current_model = status.model;
                });
            }

            set_loading.set(false);
        });
    });

    let on_switch_model = move |new_model: String| {
        set_switching.set(true);
        set_error.set(None);
        spawn_local(async move {
            let client = ApiClient::default();
            match client.switch_model(&new_model).await {
                Ok(response) => {
                    if response.success {
                        app_state.update(|s| s.current_model = response.model);
                    } else {
                        set_error.set(response.note.or_else(|| Some("Model switch failed".to_string())));
                    }
                }
                Err(e) => {
                    set_error.set(Some(e));
                }
            }
            set_switching.set(false);
        });
    };

    let on_switch_provider = move |new_provider: String| {
        set_switching.set(true);
        set_error.set(None);
        spawn_local(async move {
            let client = ApiClient::default();
            match client.switch_provider(&new_provider).await {
                Ok(response) => {
                    if response.success {
                        app_state.update(|s| s.current_provider = new_provider.clone());
                        match client.llm_models().await {
                            Ok(models_response) => {
                                app_state.update(|s| {
                                    s.available_models = models_response
                                        .models
                                        .unwrap_or_default()
                                        .into_iter()
                                        .map(|m| m.id)
                                        .collect();
                                    if let Some(current) = models_response.current {
                                        s.current_model = current;
                                    }
                                });
                            }
                            Err(e) => {
                                set_error.set(Some(e));
                            }
                        }
                    } else {
                        set_error.set(response.note.or_else(|| Some("Provider switch failed".to_string())));
                    }
                }
                Err(e) => {
                    set_error.set(Some(e));
                }
            }
            set_switching.set(false);
        });
    };

    view! {
        <div class="settings-page">
            <h2>"⚙️ Settings"</h2>
            
            <section class="settings-section">
                <h3>"LLM Configuration"</h3>
                {move || loading.get().then(|| view! { <LoadingSpinner/> })}
                {move || error.get().map(|e| view! { <ErrorDisplay message=e/> })}
                <div class="setting-item">
                    <label>"Provider"</label>
                    <div class="setting-value">
                        {move || {
                            if providers().is_empty() {
                                view! { <span>{provider}</span> }.into_view()
                            } else {
                                view! {
                                    <select
                                        on:change=move |ev| on_switch_provider(event_target_value(&ev))
                                        prop:value=provider
                                        disabled=move || switching.get()
                                    >
                                        {move || providers().into_iter().map(|p| view! {
                                            <option value={p.clone()}>{p}</option>
                                        }).collect_view()}
                                    </select>
                                }.into_view()
                            }
                        }}
                    </div>
                </div>
                {move || {
                    let count = providers().len();
                    if count <= 1 {
                        view! {
                            <p class="settings-note">
                                "Only one provider is available. To enable HuggingFace, set "
                                <code>"HF_TOKEN"</code>
                                " in "
                                <code>"/etc/op-dbus/environment"</code>
                                " and restart "
                                <code>"op-web"</code>
                                "."
                            </p>
                        }.into_view()
                    } else {
                        view! { <></> }.into_view()
                    }
                }}
                <div class="setting-item">
                    <label>"Model"</label>
                    <div class="setting-value">
                        {move || {
                            if models().is_empty() {
                                view! { <span>{model}</span> }.into_view()
                            } else {
                                view! {
                                    <select
                                        on:change=move |ev| on_switch_model(event_target_value(&ev))
                                        prop:value=model
                                        disabled=move || switching.get()
                                    >
                                        {move || models().into_iter().map(|m| view! {
                                            <option value={m.clone()}>{m}</option>
                                        }).collect_view()}
                                    </select>
                                }.into_view()
                            }
                        }}
                    </div>
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

/// Models page - browse providers and models
#[component]
pub fn ModelsPage() -> impl IntoView {
    let app_state = expect_context::<RwSignal<AppState>>();
    let (loading, set_loading) = create_signal(true);
    let (error, set_error) = create_signal::<Option<String>>(None);
    let (providers, set_providers) = create_signal::<Vec<String>>(Vec::new());
    let (selected_provider, set_selected_provider) = create_signal::<Option<String>>(None);
    let (models, set_models) = create_signal::<Vec<LlmModelInfo>>(Vec::new());
    let (current_model, set_current_model) = create_signal::<Option<String>>(None);
    let (switching, set_switching) = create_signal(false);

    let load_models = move |provider: String| {
        set_loading.set(true);
        set_error.set(None);
        spawn_local(async move {
            let client = ApiClient::default();
            match client.llm_models_for_provider(&provider).await {
                Ok(response) => {
                    set_models.set(response.models.unwrap_or_default());
                    set_current_model.set(response.current);
                }
                Err(e) => {
                    set_error.set(Some(e));
                    set_models.set(Vec::new());
                    set_current_model.set(None);
                }
            }
            set_loading.set(false);
        });
    };

    create_effect(move |_| {
        spawn_local(async move {
            let client = ApiClient::default();
            match client.llm_providers().await {
                Ok(response) => {
                    let current = response.current.clone();
                    set_providers.set(response.providers);
                    set_selected_provider.set(Some(current.clone()));
                    load_models(current);
                }
                Err(e) => {
                    set_error.set(Some(e));
                    set_loading.set(false);
                }
            }
        });
    });

    let on_select_provider = move |provider: String| {
        set_selected_provider.set(Some(provider.clone()));
        load_models(provider);
    };

    let on_use_model = move |provider: String, model: String| {
        set_switching.set(true);
        set_error.set(None);
        spawn_local(async move {
            let client = ApiClient::default();
            let mut switched_provider = false;

            if let Ok(response) = client.switch_provider(&provider).await {
                if response.success {
                    switched_provider = true;
                    app_state.update(|s| s.current_provider = provider.clone());
                } else {
                    set_error.set(response.note.or_else(|| Some("Provider switch failed".to_string())));
                }
            }

            if switched_provider {
                match client.switch_model(&model).await {
                    Ok(response) => {
                        if response.success {
                            app_state.update(|s| s.current_model = model.clone());
                            set_current_model.set(Some(model));
                        } else {
                            set_error.set(response.note.or_else(|| Some("Model switch failed".to_string())));
                        }
                    }
                    Err(e) => {
                        set_error.set(Some(e));
                    }
                }
            }

            set_switching.set(false);
        });
    };

    view! {
        <div class="models-page">
            <h2>"🧠 Models"</h2>
            {move || error.get().map(|e| view! { <ErrorDisplay message=e/> })}

            <div class="provider-tabs">
                {move || providers.get().into_iter().map(|p| {
                    let provider_name = p.clone();
                    let provider_label = provider_name.clone();
                    let provider_for_click = provider_name.clone();
                    let active = selected_provider.get().as_deref() == Some(provider_name.as_str());
                    view! {
                        <button
                            class="provider-tab"
                            class:active=active
                            on:click=move |_| on_select_provider(provider_for_click.clone())
                            disabled=move || switching.get()
                        >
                            {provider_label}
                        </button>
                    }
                }).collect_view()}
            </div>

            {move || loading.get().then(|| view! { <LoadingSpinner/> })}

            <div class="model-list">
                {move || models.get().into_iter().map(|m| {
                    let provider = selected_provider.get().unwrap_or_default();
                    let model_id = m.id.clone();
                    let model_name = m.name.clone();
                    let description = m.description.clone();
                    let parameters = m.parameters.clone();
                    let tags = m.tags.clone();
                    let is_current = current_model.get().as_deref() == Some(model_id.as_str());
                    view! {
                        <div class="model-card" class:active=is_current>
                            <div class="model-card-header">
                                <div>
                                    <h3>{model_name}</h3>
                                    <p class="model-id">{model_id.clone()}</p>
                                </div>
                                <button
                                    class="model-use"
                                    on:click=move |_| on_use_model(provider.clone(), model_id.clone())
                                    disabled=move || switching.get()
                                >
                                    {if is_current { "Active" } else { "Use" }}
                                </button>
                            </div>
                            {description.as_ref().map(|d| view! {
                                <p class="model-description">{d}</p>
                            })}
                            <div class="model-meta">
                                {parameters.as_ref().map(|p| view! {
                                    <span class="model-tag">{p}</span>
                                })}
                                {tags.into_iter().map(|t| view! {
                                    <span class="model-tag">{t}</span>
                                }).collect_view()}
                            </div>
                        </div>
                    }
                }).collect_view()}
            </div>
        </div>
    }
}
