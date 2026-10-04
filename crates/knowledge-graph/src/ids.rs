//! Exact v1 length-prefixed identity and restricted canonical JSON.
use crate::error::{Error, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
/// SHA-256 encoded as lowercase hexadecimal.
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
/// Hash typed UTF-8 components with collision-safe length prefixes.
pub fn id(kind: &str, parts: &[&str]) -> String {
    let mut h = Sha256::new();
    h.update(b"MUDCRAB-KG-ID-v1\0");
    for p in std::iter::once(kind).chain(parts.iter().copied()) {
        h.update((p.len() as u64).to_be_bytes());
        h.update(p.as_bytes());
    }
    format!("{kind}_{h:x}", h = h.finalize())
}
/// Validate restricted identity values before canonical serialization.
pub fn canonical(value: &Value) -> Result<Vec<u8>> {
    fn check(v: &Value, depth: usize) -> Result<()> {
        if depth > 128 {
            return Err(Error::new("INVALID_INPUT", "JSON nesting exceeds 128"));
        }
        match v {
            Value::Number(n)
                if n.as_i64()
                    .is_some_and(|n| n.unsigned_abs() <= 9_007_199_254_740_991) => {}
            Value::Number(_) => {
                return Err(Error::new(
                    "INVALID_INPUT",
                    "identity values require safe integers; use tagged bits for floats",
                ));
            }
            Value::Array(a) => {
                for x in a {
                    check(x, depth + 1)?;
                }
            }
            Value::Object(o) => {
                for x in o.values() {
                    check(x, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    check(value, 0)?;
    Ok(serde_json::to_vec(value)?)
}
/// Digest a canonical payload.
pub fn digest(value: &Value) -> Result<String> {
    Ok(sha256(&canonical(value)?))
}
/// Portable repository paths retain case and reject platform escape forms.
pub fn portable_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains(['\\', '\0', ':'])
        || path
            .split('/')
            .any(|x| x.is_empty() || x == ".." || x == ".")
    {
        return Err(Error::new("INVALID_INPUT", "noncanonical portable path"));
    }
    Ok(())
}
/// Verify an ordinary file below a root, rejecting every symlink component.
pub fn resolve_file(root: &std::path::Path, path: &str) -> Result<std::path::PathBuf> {
    portable_path(path)?;
    let mut p = root.canonicalize()?;
    for component in path.split('/') {
        p.push(component);
        if std::fs::symlink_metadata(&p)?.file_type().is_symlink() {
            return Err(Error::new("POLICY", "symlink artifact is forbidden"));
        }
    }
    if !p.is_file() {
        return Err(Error::new(
            "MISSING_INPUT",
            "artifact is not an ordinary file",
        ));
    }
    Ok(p)
}
