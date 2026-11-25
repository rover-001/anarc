//! The single, fixed wire schema. No free-form strings from pages exist here.
//!
//! Every enum is encoded as a numeric code on the wire; the only JSON string
//! fields are `app_version` (from Cargo) and `install_id` (a typed UUID).
//! `bucket` values are numeric band codes.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

pub const SCHEMA_VERSION: u8 = 1;

macro_rules! code_enum {
    ($name:ident { $($variant:ident => $code:literal),+ $(,)? }) => {
        #[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
        #[serde(into = "u8", try_from = "u8")]
        pub enum $name {
            $($variant),+
        }

        impl From<$name> for u8 {
            fn from(value: $name) -> u8 {
                match value {
                    $($name::$variant => $code),+
                }
            }
        }

        impl TryFrom<u8> for $name {
            type Error = &'static str;

            fn try_from(code: u8) -> Result<Self, Self::Error> {
                match code {
                    $($code => Ok($name::$variant)),+,
                    _ => Err("unknown enum code"),
                }
            }
        }
    };
}

code_enum!(Distro {
    Arch => 0,
    Ubuntu => 1,
    Fedora => 2,
    Debian => 3,
    Nixos => 4,
    Other => 5,
});

code_enum!(Arch {
    X86_64 => 0,
    Aarch64 => 1,
});

code_enum!(Harness {
    Claude => 0,
    Codex => 1,
    Antigravity => 2,
    Other => 3,
});

code_enum!(ActionKind {
    Click => 0,
    Type => 1,
    Navigate => 2,
    Read => 3,
    Other => 4,
});

code_enum!(ErrorCode {
    StaleRef => 0,
    NotFound => 1,
    Timeout => 2,
    Other => 3,
});

/// A bucketed number. Band codes are context-specific (see `buckets.rs`);
/// only the code, never the underlying measurement, leaves the machine.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Bucket(u8);

impl Bucket {
    pub const fn new(code: u8) -> Self {
        Self(code)
    }

    pub const fn code(self) -> u8 {
        self.0
    }
}

impl Serialize for Bucket {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.0)
    }
}

impl<'de> Deserialize<'de> for Bucket {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = u8::deserialize(deserializer)?;
        // Widest band set in use is the 10-point success scale (0..=9).
        if code > 9 {
            return Err(serde::de::Error::custom("bucket code out of range"));
        }
        Ok(Bucket(code))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ping {
    pub schema_version: u8,
    pub app_version: String,
    pub distro: Distro,
    pub arch: Arch,
    pub install_id: Uuid,
    pub agent: Option<AgentStats>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentStats {
    pub enabled: bool,
    pub harness: Vec<Harness>,
    pub sessions: Bucket,
    pub calls_by_action: ActionCounts,
    pub snapshot_token_ratio: Bucket,
    pub snapshot_ms: Bucket,
    pub element_count: Bucket,
    pub ref_resolve_success: Bucket,
    pub errors_by_code: ErrorCounts,
    pub plugins_installed: Bucket,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionCounts {
    pub click: Bucket,
    pub ty: Bucket,
    pub navigate: Bucket,
    pub read: Bucket,
    pub other: Bucket,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorCounts {
    pub stale_ref: Bucket,
    pub not_found: Bucket,
    pub timeout: Bucket,
    pub other: Bucket,
}

/// Canonical serialization. The consent preview and the outgoing request body
/// both use this function, so they are byte-for-byte identical.
pub fn ping_json(ping: &Ping) -> Result<String, String> {
    serde_json::to_string(ping).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_ping() -> Ping {
        Ping {
            schema_version: SCHEMA_VERSION,
            app_version: "0.1.0".into(),
            distro: Distro::Arch,
            arch: Arch::X86_64,
            install_id: Uuid::from_u128(1),
            agent: Some(AgentStats {
                enabled: true,
                harness: vec![Harness::Claude],
                sessions: Bucket::new(1),
                calls_by_action: ActionCounts::default(),
                snapshot_token_ratio: Bucket::new(0),
                snapshot_ms: Bucket::new(1),
                element_count: Bucket::new(2),
                ref_resolve_success: Bucket::new(9),
                errors_by_code: ErrorCounts::default(),
                plugins_installed: Bucket::new(0),
            }),
        }
    }

    #[test]
    fn extra_fields_are_rejected_at_every_level() {
        let mut value = serde_json::to_value(sample_ping()).expect("sample serializes");
        value
            .as_object_mut()
            .expect("ping is an object")
            .insert("url".into(), serde_json::json!("https://example.com"));
        assert!(serde_json::from_value::<Ping>(value).is_err());

        let mut value = serde_json::to_value(sample_ping().agent.expect("agent present"))
            .expect("agent serializes");
        value
            .as_object_mut()
            .expect("agent is an object")
            .insert("prompt".into(), serde_json::json!("secret"));
        assert!(serde_json::from_value::<AgentStats>(value).is_err());

        let mut value = serde_json::to_value(ActionCounts::default()).expect("counts serialize");
        value
            .as_object_mut()
            .expect("counts are an object")
            .insert("host".into(), serde_json::json!("example.com"));
        assert!(serde_json::from_value::<ActionCounts>(value).is_err());

        let mut value = serde_json::to_value(ErrorCounts::default()).expect("counts serialize");
        value
            .as_object_mut()
            .expect("counts are an object")
            .insert("message".into(), serde_json::json!("oops"));
        assert!(serde_json::from_value::<ErrorCounts>(value).is_err());
    }

    #[test]
    fn enums_and_buckets_are_numbers_on_the_wire() {
        let json = ping_json(&sample_ping()).expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse");
        let object = value.as_object().expect("object");
        assert_eq!(object["schema_version"], serde_json::json!(1));
        assert_eq!(object["distro"], serde_json::json!(0));
        assert_eq!(object["arch"], serde_json::json!(0));
        assert_eq!(object["agent"]["harness"][0], serde_json::json!(0));
        assert_eq!(object["agent"]["sessions"], serde_json::json!(1));
    }

    #[test]
    fn only_version_and_uuid_are_strings() {
        let json = ping_json(&sample_ping()).expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse");
        let mut strings = Vec::new();
        collect_string_paths(&value, "$", &mut strings);
        assert_eq!(strings, vec!["$.app_version", "$.install_id"]);
    }

    fn collect_string_paths(value: &serde_json::Value, path: &str, out: &mut Vec<String>) {
        match value {
            serde_json::Value::String(_) => out.push(path.to_owned()),
            serde_json::Value::Object(map) => {
                for (key, child) in map {
                    collect_string_paths(child, &format!("{path}.{key}"), out);
                }
            }
            serde_json::Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    collect_string_paths(child, &format!("{path}[{index}]"), out);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn enum_codes_round_trip() {
        for code in 0u8..=5 {
            let distro = Distro::try_from(code).expect("valid distro code");
            assert_eq!(u8::from(distro), code);
        }
        for code in 0u8..=4 {
            let action = ActionKind::try_from(code).expect("valid action code");
            assert_eq!(u8::from(action), code);
        }
        assert!(Distro::try_from(6).is_err());
        assert!(ActionKind::try_from(5).is_err());
        assert!(serde_json::from_str::<Bucket>("10").is_err());
    }
}
