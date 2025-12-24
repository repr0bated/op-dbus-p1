//! State plugins - each manages a domain via native protocols
//!
//! These plugins implement the StatePlugin trait from op-state

pub mod dnsresolver;
pub mod keyring;
pub mod login1;
pub mod lxc;
pub mod net;
pub mod netmaker;
pub mod openflow;
pub mod packagekit;
pub mod pcidecl;
pub mod privacy;
pub mod privacy_router;
pub mod sessdecl;
pub mod systemd;
pub mod systemd_networkd;

// Re-export plugin types
pub use dnsresolver::DnsResolverPlugin;
pub use login1::Login1Plugin;
pub use lxc::LxcPlugin;
pub use net::NetStatePlugin;
pub use openflow::OpenFlowPlugin;
pub use packagekit::PackageKitPlugin;
pub use pcidecl::PciDeclPlugin;
pub use privacy::PrivacyPlugin;
pub use privacy_router::PrivacyRouterPlugin;
pub use sessdecl::SessDeclPlugin;
pub use systemd::SystemdStatePlugin;
// pub use systemd_networkd::SystemdNetworkdPlugin; // TODO: Plugin not yet implemented
pub use netmaker::NetmakerPlugin;
