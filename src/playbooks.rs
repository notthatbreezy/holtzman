//! Durable procedural knowledge. Revisions are immutable; publishing never replaces a file.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const MAX_BYTES: usize = 65_536;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Procedure {
    pub title: String,
    pub summary: String,
    pub when_to_use: String,
    pub tags: Vec<String>,
    pub required_tools: Vec<String>,
    pub steps: Vec<String>,
    pub success_checks: Vec<String>,
    pub stop_conditions: Vec<String>,
    /// `reported_working` records supplied evidence, not independent server verification.
    pub validation: Validation,
    pub evidence: Option<String>,
    #[serde(default)]
    pub archived: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Validation {
    Draft,
    ReportedWorking,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveArgs {
    /// Stable lowercase slug, e.g. request-docking. Reuse it for updates.
    pub name: String,
    /// Zero creates; updates must match the latest revision returned by read_playbook.
    pub expected_revision: u64,
    pub procedure: Procedure,
    pub change_note: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadArgs {
    pub name: String,
    /// Omit to read the latest revision, including archived records.
    pub revision: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchArgs {
    /// Omit or leave empty to list. Matches title, tags, purpose and summary lexically.
    pub query: Option<String>,
    /// Default 5, maximum 20.
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub include_archived: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    pub schema_version: u32,
    pub name: String,
    pub revision: u64,
    pub saved_at: String,
    pub change_note: String,
    pub procedure: Procedure,
}

#[derive(Serialize)]
pub struct SearchResult {
    pub matches: Vec<IndexEntry>,
    pub total: usize,
    pub offset: usize,
}

#[derive(Serialize)]
pub struct IndexEntry {
    pub name: String,
    pub revision: u64,
    pub saved_at: String,
    pub title: String,
    pub summary: String,
    pub when_to_use: String,
    pub tags: Vec<String>,
    pub validation: Validation,
    pub archived: bool,
}

#[derive(Clone)]
pub struct Store {
    root: Option<PathBuf>,
}

impl Store {
    pub fn new(root: Option<PathBuf>) -> Self {
        Self { root }
    }

    fn root(&self) -> Result<&Path> {
        self.root.as_deref().context(
            "playbook storage unavailable: set playbooks_dir or provide a server config path",
        )
    }

    fn directory(&self, name: &str, create: bool) -> Result<PathBuf> {
        validate_name(name)?;
        let root = self.root()?;
        if create {
            fs::create_dir_all(root)?;
        }
        let root = root.canonicalize().context("opening playbook storage")?;
        let dir = root.join(name);
        if create {
            fs::create_dir_all(&dir)?;
        }
        // A configured root is trusted; individual agent-controlled children cannot redirect it.
        ensure!(
            dir.canonicalize()? == dir,
            "playbook directory must not be a link"
        );
        Ok(dir)
    }

    fn revisions(dir: &Path) -> Result<Vec<u64>> {
        let mut numbers = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.len() == 25
                && name.ends_with(".json")
                && name.as_bytes()[..20].iter().all(|c| c.is_ascii_digit())
            {
                ensure!(
                    entry.file_type()?.is_file(),
                    "revision must be a regular file"
                );
                numbers.push(name[..20].parse()?);
            }
        }
        numbers.sort_unstable();
        Ok(numbers)
    }

    fn load(dir: &Path, name: &str, number: u64) -> Result<Revision> {
        let path = dir.join(format!("{number:020}.json"));
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            metadata.is_file() && metadata.len() <= MAX_BYTES as u64,
            "invalid revision file"
        );
        let record: Revision = serde_json::from_slice(&fs::read(&path)?)
            .with_context(|| format!("reading playbook {name} revision {number}"))?;
        ensure!(
            record.schema_version == 1 && record.name == name && record.revision == number,
            "invalid revision identity"
        );
        validate_procedure(&record.procedure)?;
        Ok(record)
    }

    pub fn read(&self, args: &ReadArgs) -> Result<Revision> {
        let dir = self.directory(&args.name, false)?;
        let number = match args.revision {
            Some(n) => n,
            None => *Self::revisions(&dir)?
                .last()
                .context("playbook has no saved revisions")?,
        };
        Self::load(&dir, &args.name, number)
    }

    pub fn save(&self, args: SaveArgs) -> Result<Revision> {
        validate_name(&args.name)?;
        validate_procedure(&args.procedure)?;
        bounded(&args.change_note, 2_000)?;
        let number = args
            .expected_revision
            .checked_add(1)
            .context("revision overflow")?;
        let record = Revision {
            schema_version: 1,
            name: args.name,
            revision: number,
            saved_at: chrono::Utc::now().to_rfc3339(),
            change_note: args.change_note,
            procedure: args.procedure,
        };
        let bytes = serde_json::to_vec_pretty(&record)?;
        ensure!(bytes.len() <= MAX_BYTES, "playbook exceeds 64 KiB");
        let dir = self.directory(&record.name, true)?;
        let latest = Self::revisions(&dir)?.last().copied().unwrap_or(0);
        ensure!(
            latest == args.expected_revision,
            "revision conflict: latest is {latest}; read before updating"
        );
        // Reject corruption before adding another revision to an existing record.
        if latest > 0 {
            Self::load(&dir, &record.name, latest)?;
        }
        let temp = dir.join(format!(".pending-{:032x}", rand::random::<u128>()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        let pending = Pending(temp.clone());
        let written = file.write_all(&bytes).and_then(|()| file.sync_all());
        drop(file);
        written?;
        let destination = dir.join(format!("{number:020}.json"));
        // Hard-link publication is atomic and create-only, also across server processes.
        fs::hard_link(&temp, destination).context(
            "publishing revision (possible concurrent revision conflict; read and retry)",
        )?;
        drop(pending);
        Ok(record)
    }

    pub fn search(&self, args: &SearchArgs) -> Result<SearchResult> {
        let mut found = Vec::new();
        if let Some(query) = &args.query {
            ensure!(query.len() <= 4_000, "query too long");
        }
        let words: Vec<String> = args
            .query
            .as_deref()
            .unwrap_or("")
            .split(|c: char| !c.is_alphanumeric())
            .map(str::to_lowercase)
            .filter(|w| {
                w.len() > 2
                    && ![
                        "the", "and", "for", "you", "can", "how", "that", "this", "please", "with",
                        "what", "holt",
                    ]
                    .contains(&w.as_str())
            })
            .collect();
        if let Some(root) = &self.root {
            if !root.exists() {
                return Ok(SearchResult {
                    matches: vec![],
                    total: 0,
                    offset: args.offset.unwrap_or(0),
                });
            }
            for entry in fs::read_dir(root)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if validate_name(&name).is_err() {
                    continue;
                }
                ensure!(
                    entry.file_type()?.is_dir(),
                    "playbook entry must be a directory: {name}"
                );
                let dir = self.directory(&name, false)?;
                let Some(number) = Self::revisions(&dir)?.last().copied() else {
                    continue;
                };
                let r = Self::load(&dir, &name, number)?;
                if r.procedure.archived && !args.include_archived.unwrap_or(false) {
                    continue;
                }
                let p = r.procedure;
                let title = format!("{} {} {}", name, p.title, p.tags.join(" ")).to_lowercase();
                let purpose = format!("{} {}", p.summary, p.when_to_use).to_lowercase();
                let score = words
                    .iter()
                    .map(|w| {
                        // Prefixes allow ordinary inflection, e.g. dock/docking; no semantic search claim.
                        let needle = &w[..w.char_indices().nth(5).map_or(w.len(), |(i, _)| i)];
                        3 * usize::from(title.contains(needle))
                            + usize::from(purpose.contains(needle))
                    })
                    .sum::<usize>();
                if !words.is_empty() && score == 0 {
                    continue;
                }
                found.push((
                    score,
                    IndexEntry {
                        name,
                        revision: number,
                        saved_at: r.saved_at,
                        title: p.title,
                        summary: p.summary,
                        when_to_use: p.when_to_use,
                        tags: p.tags,
                        validation: p.validation,
                        archived: p.archived,
                    },
                ));
            }
        }
        found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.name.cmp(&b.1.name)));
        let total = found.len();
        let offset = args.offset.unwrap_or(0);
        Ok(SearchResult {
            total,
            offset,
            matches: found
                .into_iter()
                .skip(offset)
                .take(args.limit.unwrap_or(5).clamp(1, 20))
                .map(|(_, r)| r)
                .collect(),
        })
    }
}

struct Pending(PathBuf);
impl Drop for Pending {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn validate_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.len() <= 80
            && name
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            && !name.starts_with('-')
            && !name.ends_with('-'),
        "name must be a lowercase slug, at most 80 characters"
    );
    if [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ]
    .contains(&name)
    {
        bail!("reserved playbook name");
    }
    Ok(())
}

fn bounded(text: &str, max: usize) -> Result<()> {
    ensure!(
        !text.trim().is_empty() && text.len() <= max,
        "required text must contain 1..{max} bytes"
    );
    Ok(())
}

fn validate_procedure(p: &Procedure) -> Result<()> {
    bounded(&p.title, 200)?;
    bounded(&p.summary, 1_000)?;
    bounded(&p.when_to_use, 1_000)?;
    ensure!(
        !p.steps.is_empty() && !p.success_checks.is_empty() && !p.stop_conditions.is_empty(),
        "steps, success_checks and stop_conditions are required"
    );
    for list in [
        &p.tags,
        &p.required_tools,
        &p.steps,
        &p.success_checks,
        &p.stop_conditions,
    ] {
        ensure!(list.len() <= 50, "at most 50 entries per field");
        for text in list {
            bounded(text, 2_000)?;
        }
    }
    if let Some(evidence) = &p.evidence {
        bounded(evidence, 4_000)?;
    }
    ensure!(
        p.validation != Validation::ReportedWorking || p.evidence.is_some(),
        "reported_working requires evidence; otherwise use draft"
    );
    Ok(())
}
