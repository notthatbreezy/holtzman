//! Field guides for the ship computer: game concepts (fuel, heat, ...) and
//! mission profiles (trading, exploration, ...), served by the `read_guide`
//! tool and as MCP resources (`guide://<name>`).
//!
//! The built-in guides are compiled in from `guides/`. A `guides_dir` in
//! holtzman.toml adds Markdown files of its own; a file named like a
//! built-in guide replaces it.

use std::{collections::BTreeMap, path::Path};

use anyhow::{Result, bail};
use serde::Serialize;

pub struct Guide {
    pub name: &'static str,
    pub kind: &'static str,
    pub summary: &'static str,
    pub text: &'static str,
}

macro_rules! guide {
    ($name:literal, $kind:literal, $summary:literal) => {
        Guide {
            name: $name,
            kind: $kind,
            summary: $summary,
            text: include_str!(concat!("../guides/", $name, ".md")),
        }
    };
}

pub const BUILTIN: &[Guide] = &[
    guide!(
        "docking",
        "procedure",
        "Request docking using discovered bindings and observed Contacts menus"
    ),
    guide!(
        "fuel",
        "concept",
        "Scoopable stars, the fuel gauge, running dry"
    ),
    guide!(
        "heat",
        "concept",
        "Overheating from scooping, stars and combat"
    ),
    guide!(
        "jump-range",
        "concept",
        "What limits a jump: mass, FSD, neutron boosts"
    ),
    guide!(
        "interdiction",
        "concept",
        "Being pulled out of supercruise, mass lock"
    ),
    guide!(
        "rebuy",
        "concept",
        "Insurance, and everything lost on death"
    ),
    guide!(
        "engineering",
        "concept",
        "Engineers, blueprints and materials"
    ),
    guide!("trading", "profile", "Commodity runs and community goals"),
    guide!(
        "exploration",
        "profile",
        "Scanning systems: prep, scan order, safety"
    ),
    guide!(
        "exploration-for-profit",
        "profile",
        "Finding unexplored space and what pays"
    ),
    guide!(
        "mining",
        "profile",
        "Laser and core mining, hotspots, selling"
    ),
];

/// One entry in the guide index.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GuideInfo {
    pub name: String,
    pub kind: String,
    pub summary: String,
    /// "built-in" or "custom".
    pub source: &'static str,
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// First line of prose in a Markdown file, for custom guides' summaries.
fn first_line(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .unwrap_or("");
    let mut s: String = line.chars().take(100).collect();
    if line.chars().count() > 100 {
        s.push('…');
    }
    s
}

/// Custom guides in `dir`: `<name>.md` files with valid names.
fn custom(dir: Option<&Path>) -> BTreeMap<String, std::path::PathBuf> {
    let mut out = BTreeMap::new();
    let Some(entries) = dir.and_then(|d| std::fs::read_dir(d).ok()) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            && valid_name(stem)
        {
            out.insert(stem.to_string(), path);
        }
    }
    out
}

/// Every guide, built-in first, custom ones replacing built-ins by name.
pub fn list(dir: Option<&Path>) -> Vec<GuideInfo> {
    let custom = custom(dir);
    let mut out: Vec<GuideInfo> = BUILTIN
        .iter()
        .map(|g| GuideInfo {
            name: g.name.into(),
            kind: g.kind.into(),
            summary: g.summary.into(),
            source: "built-in",
        })
        .collect();
    for (name, path) in &custom {
        let summary = std::fs::read_to_string(path)
            .map(|t| first_line(&t))
            .unwrap_or_default();
        let entry = GuideInfo {
            name: name.clone(),
            kind: "custom".into(),
            summary,
            source: "custom",
        };
        match out.iter_mut().find(|g| &g.name == name) {
            Some(existing) => {
                existing.summary = entry.summary;
                existing.source = "custom";
            }
            None => out.push(entry),
        }
    }
    out
}

/// A guide's Markdown text.
pub fn read(dir: Option<&Path>, name: &str) -> Result<String> {
    let name = name.trim().trim_end_matches(".md");
    if !valid_name(name) {
        bail!("guide names are lowercase letters, digits and dashes");
    }
    if let Some(path) = custom(dir).get(name) {
        return Ok(std::fs::read_to_string(path)?);
    }
    match BUILTIN.iter().find(|g| g.name == name) {
        Some(g) => Ok(g.text.to_string()),
        None => {
            let known: Vec<String> = list(dir).into_iter().map(|g| g.name).collect();
            bail!("no guide `{name}`; available: {}", known.join(", "))
        }
    }
}

pub const URI_PREFIX: &str = "guide://";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_readable_and_titled() {
        for g in BUILTIN {
            assert!(valid_name(g.name), "{}", g.name);
            assert!(g.text.starts_with("# "), "{} needs a title", g.name);
            assert_eq!(read(None, g.name).unwrap(), g.text);
        }
        assert!(read(None, "fuel.md").is_ok());
    }

    #[test]
    fn rejects_unknown_and_unsafe_names() {
        let err = read(None, "nope").unwrap_err().to_string();
        assert!(err.contains("exploration-for-profit"), "{err}");
        assert!(read(None, "../Cargo").is_err());
        assert!(read(None, "Fuel").is_err());
    }

    #[test]
    fn custom_guides_add_and_override() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("fuel.md"), "# Fuel\n\nMy own rules.\n").unwrap();
        std::fs::write(dir.path().join("bgs.md"), "# BGS\n\nFaction work.\n").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "ignored").unwrap();
        let all = list(Some(dir.path()));
        let fuel = all.iter().find(|g| g.name == "fuel").unwrap();
        assert_eq!(
            (fuel.source, fuel.summary.as_str()),
            ("custom", "My own rules.")
        );
        assert!(all.iter().any(|g| g.name == "bgs" && g.kind == "custom"));
        assert!(!all.iter().any(|g| g.name == "notes"));
        assert!(
            read(Some(dir.path()), "fuel")
                .unwrap()
                .contains("My own rules")
        );
        assert!(
            read(Some(dir.path()), "heat")
                .unwrap()
                .starts_with("# Heat")
        );
    }
}
