//! Reusable UI Components
//!
//! Leptos components for the server administration interface.

use leptos::*;
use leptos_router::*;
use crate::state::*;

/// Application header with navigation
#[component]
pub fn Header() -> impl IntoView {
    let app_state = expect_context::<RwSignal<AppState>>();
    
    let connected = move || app_state.get().connected;
    let provider = move || app_state.get().current_provider.clone();
    let model = move || app_state.get().current_model.clone();

    view! {
        <header class="header">
            <div class="header-brand">
                <h1>"🖥️ op-dbus Admin"</h1>
                <span class="connection-status" class:connected=connected>
                    {move || if connected() { "●" } else { "○" }}
                    " "
                    {move || if connected() { "Connected" } else { "Disconnected" }}
                </span>
            </div>
            <nav class="header-nav">
                <A href="/" class="nav-link">"💬 Chat"</A>
                <A href="/tools" class="nav-link">"🔧 Tools"</A>
                <A href="/status" class="nav-link">"📊 Status"</A>
                <A href="/models" class="nav-link">"🧠 Models"</A>
                <A href="/settings" class="nav-link">"⚙️ Settings"</A>
            </nav>
            <div class="header-info">
                <span class="provider-info">
                    {provider}
                    " / "
                    {model}
                </span>
            </div>
        </header>
    }
}

/// Chat message display component
#[component]
pub fn MessageBubble(message: ChatMessage) -> impl IntoView {
    let is_user = message.role == MessageRole::User;
    let has_tools = !message.tools_executed.is_empty();

    view! {
        <div class="message" class:user=is_user class:assistant=!is_user>
            <div class="message-header">
                <span class="message-role">
                    {match message.role {
                        MessageRole::User => "You",
                        MessageRole::Assistant => "Assistant",
                        MessageRole::System => "System",
                    }}
                </span>
                <span class="message-time">{&message.timestamp}</span>
            </div>
            <div class="message-content">
                {&message.content}
            </div>
            {has_tools.then(|| view! {
                <div class="tool-results">
                    <h4>"Tools Executed:"</h4>
                    <ul>
                        {message.tools_executed.iter().map(|t| view! {
                            <li class="tool-name">{t}</li>
                        }).collect_view()}
                    </ul>
                    {message.tool_results.iter().map(|r| view! {
                        <div class="tool-result" class:success=r.success class:error=!r.success>
                            <strong>{&r.tool_name}</strong>
                            {r.success.then(|| view! {
                                <pre class="result-data">
                                    {serde_json::to_string_pretty(&r.result).unwrap_or_default()}
                                </pre>
                            })}
                            {(!r.success).then(|| view! {
                                <span class="error-message">{r.error.clone().unwrap_or_default()}</span>
                            })}
                        </div>
                    }).collect_view()}
                </div>
            })}
        </div>
    }
}

/// Chat input component
#[component]
pub fn ChatInput(
    #[prop(into)] on_send: Callback<String>,
    #[prop(default = false)] disabled: bool,
) -> impl IntoView {
    let (input_value, set_input_value) = create_signal(String::new());

    let handle_submit = move |ev: web_sys::SubmitEvent| {
        ev.prevent_default();
        let value = input_value.get();
        if !value.trim().is_empty() {
            on_send.call(value.clone());
            set_input_value.set(String::new());
        }
    };

    let handle_keydown = move |ev: web_sys::KeyboardEvent| {
        if ev.key() == "Enter" && !ev.shift_key() {
            ev.prevent_default();
            let value = input_value.get();
            if !value.trim().is_empty() {
                on_send.call(value.clone());
                set_input_value.set(String::new());
            }
        }
    };

    view! {
        <form class="chat-input-form" on:submit=handle_submit>
            <textarea
                class="chat-input"
                placeholder="Type a message... (e.g., 'Create OVS bridge ovsbr0' or 'Restart nginx')"
                prop:value=input_value
                on:input=move |ev| set_input_value.set(event_target_value(&ev))
                on:keydown=handle_keydown
                disabled=disabled
                rows=3
            />
            <button type="submit" class="send-button" disabled=disabled>
                "Send"
            </button>
        </form>
    }
}

/// Tool card component
#[component]
pub fn ToolCard(
    tool: ToolInfo,
    #[prop(into)] on_execute: Callback<String>,
) -> impl IntoView {
    let tool_name = tool.name.clone();
    let category = tool.category.clone().unwrap_or_else(|| "general".to_string());

    view! {
        <div class="tool-card">
            <div class="tool-header">
                <h3 class="tool-name">{&tool.name}</h3>
                <span class="tool-category">{category}</span>
            </div>
            <p class="tool-description">{&tool.description}</p>
            <div class="tool-actions">
                <button
                    class="execute-button"
                    on:click=move |_| on_execute.call(tool_name.clone())
                >
                    "Execute"
                </button>
            </div>
        </div>
    }
}

/// Service status row
#[component]
pub fn ServiceRow(service: ServiceStatus) -> impl IntoView {
    let is_active = service.active_state == "active";

    view! {
        <tr class="service-row" class:active=is_active class:inactive=!is_active>
            <td class="service-name">{&service.name}</td>
            <td class="service-state">
                <span class="state-badge" class:running=is_active>
                    {&service.active_state}
                </span>
            </td>
            <td class="service-substate">{&service.sub_state}</td>
            <td class="service-description">{&service.description}</td>
        </tr>
    }
}

/// Network interface card
#[component]
pub fn InterfaceCard(interface: InterfaceInfo) -> impl IntoView {
    let is_up = interface.state == "up" || interface.state == "UP";

    view! {
        <div class="interface-card" class:up=is_up class:down=!is_up>
            <div class="interface-header">
                <h4>{&interface.name}</h4>
                <span class="interface-state">{&interface.state}</span>
            </div>
            <div class="interface-details">
                {interface.mac_address.as_ref().map(|mac| view! {
                    <div class="mac-address">"MAC: " {mac}</div>
                })}
                <div class="ip-addresses">
                    {interface.ip_addresses.iter().map(|ip| view! {
                        <span class="ip-address">{ip}</span>
                    }).collect_view()}
                </div>
            </div>
        </div>
    }
}

/// OVS bridge card
#[component]
pub fn OvsBridgeCard(bridge: OvsBridge) -> impl IntoView {
    view! {
        <div class="ovs-bridge-card">
            <div class="bridge-header">
                <h4>{&bridge.name}</h4>
                <span class="bridge-uuid" title=&bridge.uuid>
                    {bridge.uuid.chars().take(8).collect::<String>()}"..."
                </span>
            </div>
            <div class="bridge-ports">
                <strong>"Ports: "</strong>
                {if bridge.ports.is_empty() {
                    view! { <span class="no-ports">"(none)"</span> }.into_view()
                } else {
                    bridge.ports.iter().map(|p| view! {
                        <span class="port-name">{p}</span>
                    }).collect_view()
                }}
            </div>
        </div>
    }
}

/// Loading spinner
#[component]
pub fn LoadingSpinner() -> impl IntoView {
    view! {
        <div class="loading-spinner">
            <div class="spinner"></div>
            <span>"Loading..."</span>
        </div>
    }
}

/// Error display
#[component]
pub fn ErrorDisplay(message: String) -> impl IntoView {
    view! {
        <div class="error-display">
            <span class="error-icon">"⚠️"</span>
            <span class="error-message">{message}</span>
        </div>
    }
}
