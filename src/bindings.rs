//! Parse Elite Dangerous `.binds` files to learn which keyboard keys trigger
//! which game actions.
//!
//! A binding looks like:
//! ```xml
//! <GalaxyMapOpen>
//!     <Primary Device="T16000MTHROTTLE" Key="Joy_5" />
//!     <Secondary Device="Keyboard" Key="Key_M">
//!         <Modifier Device="Keyboard" Key="Key_LeftShift" />
//!     </Secondary>
//! </GalaxyMapOpen>
//! ```
//! We only care about keyboard bindings (primary or secondary), since those are
//! the only ones we can synthesize.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};

use crate::keys::{ScanCode, scan_code};

/// A keyboard chord: the main key plus any held modifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChord {
    pub key: String,
    pub modifiers: Vec<String>,
}

impl KeyChord {
    pub fn scan_codes(&self) -> Result<(ScanCode, Vec<ScanCode>)> {
        let key = scan_code(&self.key).ok_or_else(|| anyhow!("unknown key name {}", self.key))?;
        let mods = self
            .modifiers
            .iter()
            .map(|m| scan_code(m).ok_or_else(|| anyhow!("unknown modifier key {m}")))
            .collect::<Result<Vec<_>>>()?;
        Ok((key, mods))
    }
}

impl std::fmt::Display for KeyChord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for m in &self.modifiers {
            write!(f, "{m}+")?;
        }
        write!(f, "{}", self.key)
    }
}

/// Every keyboard-bound action in a binds file, keyed by action name.
#[derive(Debug, Default, Clone)]
pub struct Bindings {
    pub source: Option<PathBuf>,
    pub keyboard: BTreeMap<String, KeyChord>,
}

impl Bindings {
    pub fn parse(xml: &str) -> Result<Self> {
        let doc = roxmltree::Document::parse(xml).context("parsing .binds XML")?;
        let mut keyboard = BTreeMap::new();
        for action in doc.root_element().children().filter(|n| n.is_element()) {
            let chord = ["Primary", "Secondary"].iter().find_map(|slot| {
                let node = action
                    .children()
                    .find(|c| c.is_element() && c.tag_name().name() == *slot)?;
                if node.attribute("Device") != Some("Keyboard") {
                    return None;
                }
                let key = node.attribute("Key")?.to_string();
                if key.is_empty() {
                    return None;
                }
                let modifiers = node
                    .children()
                    .filter(|c| c.is_element() && c.tag_name().name() == "Modifier")
                    .filter(|c| c.attribute("Device") == Some("Keyboard"))
                    .filter_map(|c| c.attribute("Key").map(str::to_string))
                    .collect();
                Some(KeyChord { key, modifiers })
            });
            if let Some(chord) = chord {
                keyboard.insert(action.tag_name().name().to_string(), chord);
            }
        }
        Ok(Self {
            source: None,
            keyboard,
        })
    }

    pub fn load(path: &Path) -> Result<Self> {
        let xml =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let mut b = Self::parse(&xml)?;
        b.source = Some(path.to_path_buf());
        Ok(b)
    }

    /// Find the active binds file in Elite's bindings directory.
    ///
    /// `StartPreset*.start` lists the active preset name(s); the matching file
    /// is `<preset>.<version>.binds`, and we take the most recently modified.
    pub fn discover(dir: &Path) -> Result<Self> {
        let mut presets: Vec<String> = Vec::new();
        for start in ["StartPreset.4.start", "StartPreset.start"] {
            if let Ok(text) = fs::read_to_string(dir.join(start)) {
                presets.extend(
                    text.lines()
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .map(str::to_string),
                );
                break;
            }
        }

        let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = fs::read_dir(dir)
            .with_context(|| format!("listing {}", dir.display()))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "binds"))
            .filter(|p| {
                let name = p.file_name().unwrap_or_default().to_string_lossy();
                presets.is_empty()
                    || presets
                        .iter()
                        .any(|pre| name.starts_with(&format!("{pre}.")))
            })
            .filter_map(|p| Some((fs::metadata(&p).ok()?.modified().ok()?, p)))
            .collect();
        candidates.sort();
        let (_, newest) = candidates.pop().ok_or_else(|| {
            anyhow!(
                "no custom .binds file found in {} (presets: {presets:?}). Built-in presets live \
                 in the game install; save a custom preset in Options > Controls first.",
                dir.display()
            )
        })?;
        Self::load(&newest)
    }

    pub fn get(&self, action: &str) -> Result<&KeyChord> {
        self.keyboard.get(action).ok_or_else(|| {
            anyhow!(
                "action `{action}` has no keyboard binding. Add one as a Secondary binding in \
                 Options > Controls (holtzman can only press keyboard keys)."
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<Root PresetName="Custom" MajorVersion="4" MinorVersion="2">
    <GalaxyMapOpen>
        <Primary Device="T16000MTHROTTLE" Key="Joy_5" />
        <Secondary Device="Keyboard" Key="Key_M" />
    </GalaxyMapOpen>
    <UI_Select>
        <Primary Device="Keyboard" Key="Key_Space" />
        <Secondary Device="{NoDevice}" Key="" />
    </UI_Select>
    <SystemMapOpen>
        <Primary Device="Keyboard" Key="Key_M">
            <Modifier Device="Keyboard" Key="Key_LeftShift" />
        </Primary>
        <Secondary Device="{NoDevice}" Key="" />
    </SystemMapOpen>
    <PrimaryFire>
        <Primary Device="Mouse" Key="Mouse_1" />
        <Secondary Device="{NoDevice}" Key="" />
    </PrimaryFire>
    <MouseXMode Value="" />
</Root>"#;

    #[test]
    fn parses_keyboard_bindings() {
        let b = Bindings::parse(SAMPLE).unwrap();
        assert_eq!(b.get("GalaxyMapOpen").unwrap().key, "Key_M");
        assert_eq!(b.get("UI_Select").unwrap().key, "Key_Space");
        let sys = b.get("SystemMapOpen").unwrap();
        assert_eq!(sys.modifiers, vec!["Key_LeftShift".to_string()]);
        assert_eq!(sys.to_string(), "Key_LeftShift+Key_M");
        assert!(b.get("PrimaryFire").is_err());
        assert!(b.get("MouseXMode").is_err());
    }

    #[test]
    fn discovers_newest_preset_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("StartPreset.4.start"),
            "Custom\nCustom\nCustom\nCustom\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("Other.4.2.binds"),
            SAMPLE.replace("Key_M", "Key_G"),
        )
        .unwrap();
        fs::write(dir.path().join("Custom.4.2.binds"), SAMPLE).unwrap();
        let b = Bindings::discover(dir.path()).unwrap();
        assert_eq!(b.get("GalaxyMapOpen").unwrap().key, "Key_M");
    }
}
