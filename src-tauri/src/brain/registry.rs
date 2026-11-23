//! The set of configured brains and which brain serves which BOB feature ("roles").
//!
//! Persisted as `brains.json` in the app config directory. Holds no secrets (keys are in the Keychain).
//! Invariants (property-tested): ids are unique, the built-in brain always exists, and a role only ever
//! points at an existing brain with the capability that role needs.

use super::{BrainConfig, Capabilities, ProviderKind};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const BUILTIN_ID: &str = "builtin";

/// Which brain each feature uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Roles {
    pub ghost: Option<String>,
    pub selection: Option<String>,
    pub dictation: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Ghost,
    Selection,
    Dictation,
}

impl Role {
    pub fn needs(self, c: &Capabilities) -> bool {
        match self {
            Role::Ghost => c.ghost,
            Role::Selection | Role::Dictation => c.chat,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    #[error("no brain with id {0}")]
    NotFound(String),
    #[error("a brain with id {0} already exists")]
    Duplicate(String),
    #[error("the built-in brain cannot be removed")]
    BuiltinIsPermanent,
    #[error("{0} cannot be used for this feature (missing capability)")]
    Incapable(String),
    #[error("invalid brain: {0}")]
    Invalid(String),
    #[error("cannot read or write brains.json: {0}")]
    Io(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Registry {
    pub brains: Vec<BrainConfig>,
    pub roles: Roles,
}

pub fn builtin_brain() -> BrainConfig {
    BrainConfig {
        id: BUILTIN_ID.into(),
        name: "BOB built-in (local)".into(),
        kind: ProviderKind::Builtin,
        endpoint: ProviderKind::Builtin.default_endpoint().into(),
        model: None,
        capabilities: ProviderKind::Builtin.default_capabilities(),
        has_api_key: false,
        share_context: None,
    }
}

impl Default for Registry {
    fn default() -> Self {
        let id = Some(BUILTIN_ID.to_string());
        Self {
            brains: vec![builtin_brain()],
            roles: Roles {
                ghost: id.clone(),
                selection: id.clone(),
                dictation: id,
            },
        }
    }
}

impl Registry {
    pub fn get(&self, id: &str) -> Option<&BrainConfig> {
        self.brains.iter().find(|b| b.id == id)
    }

    /// The brain serving `role`, if any.
    pub fn for_role(&self, role: Role) -> Option<&BrainConfig> {
        let id = match role {
            Role::Ghost => self.roles.ghost.as_deref(),
            Role::Selection => self.roles.selection.as_deref(),
            Role::Dictation => self.roles.dictation.as_deref(),
        }?;
        self.get(id)
    }

    fn validate(b: &BrainConfig) -> Result<(), RegistryError> {
        if b.id.trim().is_empty()
            || b.id.len() > 64
            || !b
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(RegistryError::Invalid(
                "id must be 1-64 letters, digits, '-' or '_'".into(),
            ));
        }
        if b.name.trim().is_empty() {
            return Err(RegistryError::Invalid("name must not be empty".into()));
        }
        if b.kind != ProviderKind::Builtin {
            let url = url::Url::parse(&b.endpoint)
                .map_err(|e| RegistryError::Invalid(format!("endpoint: {e}")))?;
            if !matches!(url.scheme(), "http" | "https") {
                return Err(RegistryError::Invalid(
                    "endpoint must be http or https".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn add(&mut self, b: BrainConfig) -> Result<(), RegistryError> {
        Self::validate(&b)?;
        if b.kind == ProviderKind::Builtin {
            return Err(RegistryError::Invalid(
                "only one built-in brain exists".into(),
            ));
        }
        if self.get(&b.id).is_some() {
            return Err(RegistryError::Duplicate(b.id));
        }
        self.brains.push(b);
        Ok(())
    }

    /// Replace an existing brain's configuration. Roles that lose a needed capability are cleared.
    pub fn update(&mut self, b: BrainConfig) -> Result<(), RegistryError> {
        Self::validate(&b)?;
        let slot = self
            .brains
            .iter_mut()
            .find(|x| x.id == b.id)
            .ok_or_else(|| RegistryError::NotFound(b.id.clone()))?;
        if (slot.kind == ProviderKind::Builtin) != (b.kind == ProviderKind::Builtin) {
            return Err(RegistryError::Invalid(
                "cannot change a brain to or from built-in".into(),
            ));
        }
        *slot = b;
        self.repair_roles();
        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> Result<BrainConfig, RegistryError> {
        if id == BUILTIN_ID {
            return Err(RegistryError::BuiltinIsPermanent);
        }
        let i = self
            .brains
            .iter()
            .position(|b| b.id == id)
            .ok_or_else(|| RegistryError::NotFound(id.into()))?;
        let removed = self.brains.remove(i);
        self.repair_roles();
        Ok(removed)
    }

    pub fn set_role(&mut self, role: Role, id: Option<&str>) -> Result<(), RegistryError> {
        if let Some(id) = id {
            let b = self
                .get(id)
                .ok_or_else(|| RegistryError::NotFound(id.into()))?;
            if !role.needs(&b.capabilities) {
                return Err(RegistryError::Incapable(b.name.clone()));
            }
        }
        let v = id.map(str::to_string);
        match role {
            Role::Ghost => self.roles.ghost = v,
            Role::Selection => self.roles.selection = v,
            Role::Dictation => self.roles.dictation = v,
        }
        Ok(())
    }

    /// Clear roles that point at missing or incapable brains.
    fn repair_roles(&mut self) {
        for role in [Role::Ghost, Role::Selection, Role::Dictation] {
            let ok = self.for_role(role).map(|b| role.needs(&b.capabilities));
            if ok != Some(true) {
                let _ = self.set_role(role, None);
            }
        }
    }

    /// Load from `path`, falling back to defaults when the file is missing. A corrupt file is kept
    /// aside as `brains.json.corrupt` rather than silently overwritten.
    pub fn load(path: &Path) -> Result<Self, RegistryError> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(RegistryError::Io(e.to_string())),
        };
        match serde_json::from_str::<Registry>(&text) {
            Ok(mut r) => {
                if r.get(BUILTIN_ID).is_none() {
                    r.brains.insert(0, builtin_brain());
                }
                r.repair_roles();
                Ok(r)
            }
            Err(_) => {
                let _ = std::fs::rename(path, path.with_extension("json.corrupt"));
                Ok(Self::default())
            }
        }
    }

    /// Atomic save: write a temp file next to `path`, then rename.
    pub fn save(&self, path: &Path) -> Result<(), RegistryError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| RegistryError::Io(e.to_string()))?;
        }
        let tmp = path.with_extension("json.tmp");
        let text =
            serde_json::to_string_pretty(self).map_err(|e| RegistryError::Io(e.to_string()))?;
        std::fs::write(&tmp, text).map_err(|e| RegistryError::Io(e.to_string()))?;
        std::fs::rename(&tmp, path).map_err(|e| RegistryError::Io(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn brain(id: &str, ghost: bool, chat: bool) -> BrainConfig {
        BrainConfig {
            id: id.into(),
            name: format!("Brain {id}"),
            kind: ProviderKind::Ollama,
            endpoint: "http://127.0.0.1:11434".into(),
            model: Some("qwen2.5:1.5b".into()),
            capabilities: Capabilities { ghost, chat },
            has_api_key: false,
            share_context: None,
        }
    }

    #[test]
    fn default_registry_routes_everything_to_the_builtin_brain() {
        let r = Registry::default();
        for role in [Role::Ghost, Role::Selection, Role::Dictation] {
            assert_eq!(r.for_role(role).unwrap().id, BUILTIN_ID);
        }
    }

    #[test]
    fn rejects_duplicates_invalid_ids_and_bad_endpoints() {
        let mut r = Registry::default();
        r.add(brain("o", true, true)).unwrap();
        assert_eq!(
            r.add(brain("o", true, true)),
            Err(RegistryError::Duplicate("o".into()))
        );
        assert!(matches!(
            r.add(brain("bad id!", true, true)),
            Err(RegistryError::Invalid(_))
        ));
        let mut b = brain("x", true, true);
        b.endpoint = "ftp://example.com".into();
        assert!(matches!(r.add(b), Err(RegistryError::Invalid(_))));
        assert_eq!(r.remove(BUILTIN_ID), Err(RegistryError::BuiltinIsPermanent));
    }

    #[test]
    fn roles_require_the_matching_capability() {
        let mut r = Registry::default();
        r.add(brain("chat-only", false, true)).unwrap();
        assert!(matches!(
            r.set_role(Role::Ghost, Some("chat-only")),
            Err(RegistryError::Incapable(_))
        ));
        r.set_role(Role::Selection, Some("chat-only")).unwrap();
        assert_eq!(r.for_role(Role::Selection).unwrap().id, "chat-only");
    }

    #[test]
    fn removing_or_downgrading_a_brain_clears_its_roles() {
        let mut r = Registry::default();
        r.add(brain("o", true, true)).unwrap();
        r.set_role(Role::Ghost, Some("o")).unwrap();
        r.set_role(Role::Selection, Some("o")).unwrap();
        r.update(brain("o", false, true)).unwrap();
        assert_eq!(
            r.roles.ghost, None,
            "ghost role must not keep a brain that lost the capability"
        );
        assert_eq!(r.roles.selection.as_deref(), Some("o"));
        r.remove("o").unwrap();
        assert_eq!(r.roles.selection, None);
    }

    #[test]
    fn save_and_load_round_trip_and_corrupt_files_are_kept_aside() {
        let dir = std::env::temp_dir().join(format!("bob-reg-{}", std::process::id()));
        let path = dir.join("brains.json");
        let mut r = Registry::default();
        r.add(brain("o", true, true)).unwrap();
        r.save(&path).unwrap();
        assert_eq!(Registry::load(&path).unwrap(), r);
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(Registry::load(&path).unwrap(), Registry::default());
        assert!(dir.join("brains.json.corrupt").exists());
        assert_eq!(
            Registry::load(&dir.join("missing.json")).unwrap(),
            Registry::default()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saved_file_contains_no_api_keys() {
        let mut r = Registry::default();
        let mut b = brain("cloud", false, true);
        b.has_api_key = true;
        r.add(b).unwrap();
        let text = serde_json::to_string(&r).unwrap();
        assert!(!text.to_lowercase().contains("sk-") && text.contains("has_api_key"));
    }

    #[derive(Debug, Clone)]
    enum Op {
        Add(String, bool, bool),
        Update(String, bool, bool),
        Remove(String),
        SetRole(u8, Option<String>),
    }

    fn arb_id() -> impl Strategy<Value = String> {
        prop_oneof![Just(BUILTIN_ID.to_string()), "[a-c]{1}"]
    }

    fn arb_op() -> impl Strategy<Value = Op> {
        prop_oneof![
            (arb_id(), any::<bool>(), any::<bool>()).prop_map(|(i, g, c)| Op::Add(i, g, c)),
            (arb_id(), any::<bool>(), any::<bool>()).prop_map(|(i, g, c)| Op::Update(i, g, c)),
            arb_id().prop_map(Op::Remove),
            (0u8..3, proptest::option::of(arb_id())).prop_map(|(r, i)| Op::SetRole(r, i)),
        ]
    }

    proptest! {
        /// For any sequence of operations (successful or rejected), the registry invariants hold.
        #[test]
        fn invariants_hold_for_any_operation_sequence(ops in proptest::collection::vec(arb_op(), 0..40)) {
            let mut r = Registry::default();
            for op in ops {
                let _ = match op {
                    Op::Add(i, g, c) => r.add(brain(&i, g, c)),
                    Op::Update(i, g, c) => {
                        let mut b = brain(&i, g, c);
                        if i == BUILTIN_ID { b = BrainConfig { capabilities: Capabilities { ghost: g, chat: c }, ..builtin_brain() }; }
                        r.update(b)
                    }
                    Op::Remove(i) => r.remove(&i).map(|_| ()),
                    Op::SetRole(k, i) => {
                        let role = [Role::Ghost, Role::Selection, Role::Dictation][k as usize];
                        r.set_role(role, i.as_deref())
                    }
                };
                // Built-in always present, ids unique.
                prop_assert!(r.get(BUILTIN_ID).is_some());
                let mut ids: Vec<_> = r.brains.iter().map(|b| b.id.clone()).collect();
                ids.sort();
                let n = ids.len();
                ids.dedup();
                prop_assert_eq!(ids.len(), n);
                // Every role points at an existing, capable brain.
                for role in [Role::Ghost, Role::Selection, Role::Dictation] {
                    let id = match role { Role::Ghost => &r.roles.ghost, Role::Selection => &r.roles.selection, Role::Dictation => &r.roles.dictation };
                    if let Some(id) = id {
                        let b = r.get(id);
                        prop_assert!(b.is_some(), "role points at missing brain {}", id);
                        prop_assert!(role.needs(&b.unwrap().capabilities));
                    }
                }
            }
        }
    }
}
