//! Evidence-backed client quirks and their deterministic resolver.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// A capability that a quirk may add or remove from a declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    /// LSP workspace documentChanges support.
    DocumentChanges,
    /// LSP window showDocument support.
    ShowDocument,
}

/// A matching condition for an automatically detected quirk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Match {
    /// Match the exact authentic client name and inclusive version bounds.
    ClientVersion {
        name: &'static str,
        min: &'static str,
        max: &'static str,
    },
    /// Match an advertised initialize capability.
    Capability(CapabilityTest),
}

/// A capability-based quirk test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityTest {
    /// Client declared experimental.magit.
    DeclaresMagit,
}

/// A declared capability mutation made by a quirk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityOverride {
    /// Capability affected.
    pub capability: Capability,
    /// Whether the quirk adds or removes it.
    pub enabled: bool,
}

/// Named behavioral effects grouped under one stable quirk identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quirk {
    /// Stable configuration key.
    pub id: &'static str,
    /// Evidence condition; absent means intentionally not auto-enabled.
    pub matches: Option<Match>,
    /// Capability mutation, if any.
    pub capability: Option<CapabilityOverride>,
    /// Named server workaround, if any.
    pub workaround: Option<&'static str>,
    /// Named fake-editor behavior model, if any.
    pub model: Option<&'static str>,
    /// Whether this quirk permits Plain key policy; false imposes a hard limit.
    pub plain_keys_allowed: Option<bool>,
}

/// Validated per-client initialization identity and capabilities.
#[derive(Debug, Clone, Copy)]
pub struct Client<'a> {
    /// Exact clientInfo.name, if supplied.
    pub name: Option<&'a str>,
    /// Exact clientInfo.version, if supplied.
    pub version: Option<&'a str>,
    /// Initialize capabilities object.
    pub capabilities: &'a Value,
}

/// Resolved quirk set after automatic matching and user overrides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    /// Enabled stable quirk identifiers.
    pub enabled: BTreeSet<&'static str>,
}

/// Registry of supported evidence-backed client quirks.
pub const REGISTRY: &[Quirk] = &[
    Quirk {
        id: "assume-document-changes",
        matches: Some(Match::ClientVersion {
            name: "Neovim",
            min: "0.12.6",
            max: "0.12.6",
        }),
        capability: Some(CapabilityOverride {
            capability: Capability::DocumentChanges,
            enabled: true,
        }),
        workaround: None,
        model: Some("neovim-accepts-undeclared-documentChanges"),
        plain_keys_allowed: None,
    },
    Quirk {
        id: "apply-result-unreliable",
        matches: Some(Match::ClientVersion {
            name: "Neovim",
            min: "0.12.6",
            max: "0.12.6",
        }),
        capability: None,
        workaround: Some("reconcile-apply-result"),
        model: Some("silent-stale-apply"),
        plain_keys_allowed: None,
    },
    Quirk {
        id: "decomposed-changes",
        matches: Some(Match::ClientVersion {
            name: "helix",
            min: "25.07.1",
            max: "25.07.1",
        }),
        capability: None,
        workaround: None,
        model: Some("decomposed-changes"),
        plain_keys_allowed: None,
    },
    // Initialize omission alone is not evidence that Helix handles this request.
    Quirk {
        id: "assume-show-document",
        matches: None,
        capability: Some(CapabilityOverride {
            capability: Capability::ShowDocument,
            enabled: true,
        }),
        workaround: None,
        model: None,
        plain_keys_allowed: None,
    },
];

/// Resolve automatic evidence matches, then apply validated user on/off overrides.
pub fn resolve(client: Client<'_>, overrides: Option<&Value>) -> Result<Resolution, String> {
    let user = parse_overrides(overrides)?;
    Ok(resolve_validated(client, user.into_iter()))
}

/// Resolve quirks using already-validated configuration overrides.
pub fn resolve_validated<'a>(
    client: Client<'_>,
    overrides: impl IntoIterator<Item = (&'a str, bool)>,
) -> Resolution {
    let mut enabled: BTreeSet<_> = REGISTRY
        .iter()
        .filter(|q| {
            q.matches
                .is_some_and(|condition| matches(client, condition))
        })
        .map(|q| q.id)
        .collect();
    for (id, on) in overrides {
        let quirk = REGISTRY
            .iter()
            .find(|quirk| quirk.id == id)
            .expect("validated quirk configuration contains only registry ids");
        let id = quirk.id;
        if on {
            enabled.insert(id);
        } else {
            enabled.remove(id);
        }
    }
    Resolution { enabled }
}

/// Parse an object of quirk-id to boolean overrides, rejecting unknown ids and types.
pub fn parse_overrides(value: Option<&Value>) -> Result<BTreeMap<&'static str, bool>, String> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let object = value
        .as_object()
        .ok_or_else(|| "quirk overrides must be an object".to_owned())?;
    let mut parsed = BTreeMap::new();
    for (id, value) in object {
        let q = REGISTRY
            .iter()
            .find(|q| q.id == id)
            .ok_or_else(|| format!("unknown quirk id: {id}"))?;
        let enabled = value
            .as_bool()
            .ok_or_else(|| format!("quirk override {id} must be boolean"))?;
        parsed.insert(q.id, enabled);
    }
    Ok(parsed)
}

/// Test one registry match condition against client identity and declarations.
pub fn matches(client: Client<'_>, condition: Match) -> bool {
    match condition {
        Match::Capability(CapabilityTest::DeclaresMagit) => {
            client.capabilities.pointer("/experimental/magit").is_some()
        }
        Match::ClientVersion { name, min, max } => {
            client.name == Some(name)
                && client
                    .version
                    .is_some_and(|v| version_inclusive(v, min, max))
        }
    }
}

fn version_inclusive(value: &str, min: &str, max: &str) -> bool {
    fn parts(v: &str) -> Option<Vec<u64>> {
        v.split('.').map(str::parse).collect::<Result<_, _>>().ok()
    }
    let (Some(v), Some(lo), Some(hi)) = (parts(value), parts(min), parts(max)) else {
        return false;
    };
    v >= lo && v <= hi
}
