use holtzman::playbooks::{Procedure, ReadArgs, SaveArgs, SearchArgs, Store, Validation};
use std::sync::{Arc, Barrier};

fn save(name: &str, expected_revision: u64) -> SaveArgs {
    SaveArgs {
        name: name.into(),
        expected_revision,
        change_note: "Captain requested this workflow".into(),
        procedure: Procedure {
            title: "Request docking".into(),
            summary: "Inspect before requesting docking".into(),
            when_to_use: "Captain requests docking at a station".into(),
            tags: vec!["docking".into()],
            required_tools: vec!["screenshot".into(), "press_binding".into()],
            steps: vec!["Inspect current cockpit and distance; use observed Contacts menu".into()],
            success_checks: vec!["Observe a grant or DockingGranted journal event".into()],
            stop_conditions: vec!["Stop if the menu or distance cannot be verified".into()],
            validation: Validation::Draft,
            evidence: None,
            archived: false,
        },
    }
}

fn search(query: &str) -> SearchArgs {
    SearchArgs {
        query: Some(query.into()),
        limit: None,
        offset: None,
        include_archived: None,
    }
}

#[test]
fn survives_reopen_and_preserves_old_revisions() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::new(Some(temp.path().into()));
    let first = store.save(save("request-docking", 0)).unwrap();
    assert_eq!(first.revision, 1);
    let mut update = save("request-docking", 1);
    update.procedure.validation = Validation::ReportedWorking;
    update.procedure.evidence =
        Some("Captain reported success on 2026-10-26; not independently verified".into());
    assert_eq!(store.save(update).unwrap().revision, 2);
    let reopened = Store::new(Some(temp.path().into()));
    assert_eq!(
        reopened
            .read(&ReadArgs {
                name: first.name.clone(),
                revision: None
            })
            .unwrap()
            .revision,
        2
    );
    let old = reopened
        .read(&ReadArgs {
            name: first.name,
            revision: Some(1),
        })
        .unwrap();
    assert_eq!(old.procedure.validation, Validation::Draft);
    assert!(
        reopened
            .save(save("request-docking", 1))
            .unwrap_err()
            .to_string()
            .contains("conflict")
    );
    assert_eq!(reopened.search(&search("")).unwrap().matches.len(), 1);
}

#[test]
fn racing_creates_publish_exactly_one_complete_record() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::new(Some(temp.path().into()));
    let barrier = Arc::new(Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let store = store.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store.save(save("docking", 0)).is_ok()
            })
        })
        .collect();
    assert_eq!(
        threads
            .into_iter()
            .map(|t| usize::from(t.join().unwrap()))
            .sum::<usize>(),
        1
    );
    assert_eq!(
        store
            .read(&ReadArgs {
                name: "docking".into(),
                revision: None
            })
            .unwrap()
            .revision,
        1
    );
    assert_eq!(
        std::fs::read_dir(temp.path().join("docking"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn search_ranks_pages_and_excludes_archived_records() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::new(Some(temp.path().into()));
    store.save(save("docking", 0)).unwrap();
    let mut other = save("trade", 0);
    other.procedure.title = "Trade route".into();
    other.procedure.tags.clear();
    other.procedure.summary = "Inspect markets".into();
    other.procedure.when_to_use = "Moving commodities".into();
    store.save(other).unwrap();
    assert_eq!(
        store
            .search(&search("How do I request docking please?"))
            .unwrap()
            .matches[0]
            .name,
        "docking"
    );
    assert_eq!(store.search(&search("mining asteroids")).unwrap().total, 0);
    let mut list = search("");
    list.limit = Some(1);
    list.offset = Some(1);
    let page = store.search(&list).unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.matches[0].name, "trade");
    let mut archive = save("docking", 1);
    archive.procedure.archived = true;
    store.save(archive).unwrap();
    assert_eq!(store.search(&search("docking")).unwrap().total, 0);
    list.include_archived = Some(true);
    list.offset = None;
    assert_eq!(store.search(&list).unwrap().total, 2);
    assert!(
        store
            .read(&ReadArgs {
                name: "docking".into(),
                revision: None
            })
            .unwrap()
            .procedure
            .archived
    );
    store.save(save("docking", 2)).unwrap();
    assert_eq!(store.search(&search("docking")).unwrap().total, 1);
}

#[test]
fn rejects_unsafe_names_missing_evidence_and_oversized_data() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::new(Some(temp.path().into()));
    for name in [
        "../outside",
        "CON",
        "con",
        "aux",
        "com1",
        "a/b",
        "-a",
        "",
        "a.",
    ] {
        assert!(store.save(save(name, 0)).is_err(), "accepted {name}");
    }
    let mut args = save("docking", 0);
    args.procedure.validation = Validation::ReportedWorking;
    assert!(store.save(args).is_err());
    let mut args = save("docking", 0);
    args.procedure.steps = vec!["x".repeat(2_000); 40];
    assert!(store.save(args).is_err());
    let mut args = save("docking", 0);
    args.procedure.success_checks.clear();
    assert!(store.save(args).is_err());
    assert!(!temp.path().join("docking").exists());
}

#[test]
fn absent_storage_is_empty_and_corruption_is_not_silently_ignored() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::new(Some(temp.path().join("missing")));
    assert_eq!(store.search(&search("")).unwrap().total, 0);
    assert!(!temp.path().join("missing").exists());
    assert!(Store::new(None).save(save("docking", 0)).is_err());
    store.save(save("docking", 0)).unwrap();
    std::fs::write(
        temp.path()
            .join("missing/docking/00000000000000000001.json"),
        b"{",
    )
    .unwrap();
    assert!(store.search(&search("")).is_err());
    assert!(store.save(save("docking", 1)).is_err());
}

#[cfg(unix)]
#[test]
fn linked_playbook_directories_cannot_escape_storage() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("docking")).unwrap();
    let store = Store::new(Some(root.path().into()));
    assert!(store.save(save("docking", 0)).is_err());
    assert!(store.search(&search("")).is_err());
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}
