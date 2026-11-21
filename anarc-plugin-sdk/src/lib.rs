#![forbid(unsafe_code)]

//! Small, versioned guest-side helpers for Lumen component plugins.
//!
//! The host API contract lives in `../../wit/lumen.wit`. The generated bindings
//! are available with the default `component` feature. The host remains
//! responsible for enforcing manifest permissions on every imported call.

#[cfg(feature = "component")]
pub use wit_bindgen;

/// Guest bindings generated from the versioned Lumen WIT world.
#[cfg(feature = "component")]
pub mod bindings {
    wit_bindgen::generate!({
        path: "../wit",
        world: "lumen-plugin",
    });
}

/// Metadata a plugin exposes to the Lumen host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PluginInfo {
    pub id: &'static str,
    pub version: &'static str,
    pub api: &'static str,
}

/// Declare static plugin metadata without hand-writing a struct literal.
///
/// The runtime manifest, including permissions and contributions, remains the
/// authoritative install-time record. This declaration supplies the identity
/// returned by the component's `get-manifest` export.
#[macro_export]
macro_rules! plugin_info {
    (id: $id:literal, version: $version:literal, api: $api:literal $(,)?) => {
        $crate::PluginInfo {
            id: $id,
            version: $version,
            api: $api,
        }
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn plugin_info_macro_keeps_manifest_identity() {
        let info = crate::plugin_info!(id: "dev.example.tidy-tabs", version: "0.1.0", api: "1");
        assert_eq!(info.id, "dev.example.tidy-tabs");
        assert_eq!(info.version, "0.1.0");
        assert_eq!(info.api, "1");
    }
}
