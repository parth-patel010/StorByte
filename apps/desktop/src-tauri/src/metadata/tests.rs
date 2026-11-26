use super::{AccountRow, MetadataDb};
use anyhow::Result;

const ACCOUNT_ID: &str = "account-a";

fn account() -> AccountRow {
    AccountRow {
        id: ACCOUNT_ID.into(),
        email: "initial@example.test".into(),
        provider: "gdrive".into(),
        status: "connected".into(),
        priority: 1,
        quota_bytes: Some(1_000),
        usage_bytes: Some(32),
        last_indexed_at: None,
        change_page_token: None,
        drive_root_id: None,
    }
}

fn add_directory(db: &MetadataDb, parent: Option<i64>, name: &str, path: &str) -> Result<i64> {
    let (id, _) = db.insert_node(
        parent, name, path, true, 0, None, path, ACCOUNT_ID, None, None,
    )?;
    Ok(id)
}

fn add_file(db: &MetadataDb, parent: i64, name: &str, path: &str) -> Result<i64> {
    let (id, _) = db.insert_node(
        Some(parent),
        name,
        path,
        false,
        32,
        Some("text/plain"),
        path,
        ACCOUNT_ID,
        None,
        Some("fixture-etag"),
    )?;
    Ok(id)
}

#[test]
fn account_upsert_updates_persisted_account_without_adding_a_second_row() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("metadata.sqlite");
    let db = MetadataDb::open(&path)?;
    let mut row = account();
    db.upsert_account(&row)?;
    assert_eq!(db.list_accounts()?[0].email, "initial@example.test");

    row.email = "updated@example.test".into();
    row.usage_bytes = Some(64);
    db.upsert_account(&row)?;
    drop(db);

    let reopened = MetadataDb::open(&path)?;
    let accounts = reopened.list_accounts()?;
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id, ACCOUNT_ID);
    assert_eq!(accounts[0].email, "updated@example.test");
    assert_eq!(accounts[0].quota_bytes, Some(1_000));
    assert_eq!(accounts[0].usage_bytes, Some(64));
    Ok(())
}

#[test]
fn normalized_lookup_and_listing_return_only_direct_children_in_order() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let db = MetadataDb::open(&temp.path().join("metadata.sqlite"))?;
    db.upsert_account(&account())?;
    let docs = add_directory(&db, None, "Docs", "/Docs")?;
    let report = add_file(&db, docs, "report.txt", "/Docs/report.txt")?;
    add_file(&db, docs, "Apple.txt", "/Docs/Apple.txt")?;
    let archive = add_directory(&db, Some(docs), "Archive", "/Docs/Archive")?;
    add_file(&db, archive, "nested.txt", "/Docs/Archive/nested.txt")?;

    let file = db.get_node_by_path("Docs//report.txt/")?.expect("report");
    assert_eq!(file.id, report);
    assert_eq!(file.parent_id, Some(docs));
    assert_eq!(file.virtual_path, r"\Docs\report.txt");
    assert_eq!(file.account_id, ACCOUNT_ID);
    assert_eq!(file.size, 32);
    assert_eq!(file.mime_type.as_deref(), Some("text/plain"));
    assert_eq!(file.etag.as_deref(), Some("fixture-etag"));
    assert!(!file.is_directory);

    let children = db.list_children("/Docs/")?;
    let names: Vec<_> = children.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(names, ["Archive", "Apple.txt", "report.txt"]);
    assert!(children[0].is_directory);
    assert!(db.list_children("/missing")?.is_empty());
    Ok(())
}

#[test]
fn directory_rename_rewrites_nested_paths_but_keeps_ids_and_neighboring_paths() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let db = MetadataDb::open(&temp.path().join("metadata.sqlite"))?;
    db.upsert_account(&account())?;
    let docs = add_directory(&db, None, "Docs", "/Docs")?;
    let archive = add_directory(&db, Some(docs), "Archive", "/Docs/Archive")?;
    let report = add_file(&db, archive, "report.txt", "/Docs/Archive/report.txt")?;
    let neighbor = add_directory(&db, None, "Docs2", "/Docs2")?;
    let untouched = add_file(&db, neighbor, "keep.txt", "/Docs2/keep.txt")?;

    db.rename_node_tree(docs, "/Docs/", "Reports", "/Reports")?;
    assert!(db.get_node_by_path("/Docs")?.is_none());
    assert!(db.get_node_by_path("/Docs/Archive/report.txt")?.is_none());
    assert_eq!(
        db.get_node_by_path("/Reports")?
            .expect("renamed directory")
            .id,
        docs
    );
    assert_eq!(db.list_children("/Reports")?[0].id, archive);
    let moved = db
        .get_node_by_path("/Reports/Archive/report.txt")?
        .expect("moved report");
    assert_eq!(moved.id, report);
    assert_eq!(moved.parent_id, Some(archive));
    assert_eq!(moved.remote_id, "/Docs/Archive/report.txt");
    assert_eq!(
        db.get_node_by_path("/Docs2/keep.txt")?
            .expect("neighbor file")
            .id,
        untouched
    );
    Ok(())
}

#[test]
fn deleting_a_directory_removes_its_descendants_without_removing_siblings() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let db = MetadataDb::open(&temp.path().join("metadata.sqlite"))?;
    db.upsert_account(&account())?;
    let docs = add_directory(&db, None, "Docs", "/Docs")?;
    let archive = add_directory(&db, Some(docs), "Archive", "/Docs/Archive")?;
    let nested = add_directory(&db, Some(archive), "Nested", "/Docs/Archive/Nested")?;
    add_file(&db, nested, "remove.txt", "/Docs/Archive/Nested/remove.txt")?;
    let neighbor = add_directory(&db, Some(docs), "Archive2", "/Docs/Archive2")?;
    let untouched = add_file(&db, neighbor, "keep.txt", "/Docs/Archive2/keep.txt")?;

    db.delete_node_by_path("/Docs/Archive/")?;
    assert!(db.get_node_by_path("/Docs/Archive")?.is_none());
    assert!(db.get_node_by_path("/Docs/Archive/Nested")?.is_none());
    assert!(db
        .get_node_by_path("/Docs/Archive/Nested/remove.txt")?
        .is_none());
    let siblings = db.list_children("/Docs")?;
    assert_eq!(siblings.len(), 1);
    assert_eq!(siblings[0].id, neighbor);
    assert_eq!(
        db.get_node_by_path("/Docs/Archive2/keep.txt")?
            .expect("neighbor file")
            .id,
        untouched
    );
    assert_eq!(db.list_accounts()?.len(), 1);
    Ok(())
}
