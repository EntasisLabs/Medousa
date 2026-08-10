use anyhow::Result;
use locus_core_rs::domain::contracts::{SemanticIndexStore, SemanticIndexStoreInitializer};
use locus_core_rs::domain::models::SemanticTagNodeRef;
use locus_core_rs::storage::surrealdb::client::{QueryParams, SurrealDbClient};
use medousa::runtime::locus_semantic_index_store::MedousaSurrealSemanticIndexStore;
use medousa::runtime::locus_surreal_client::StasisSurrealDbClient;
use serde_json::json;
use surrealdb::Surreal;
use surrealdb::engine::any::Any;

async fn mem_db() -> Surreal<Any> {
    let db = surrealdb::engine::any::connect("mem://")
        .await
        .expect("mem db should connect");
    db.use_ns("test")
        .use_db("test")
        .await
        .expect("namespace should be set");
    db
}

#[tokio::test]
async fn raw_query_surfaces_schema_errors_before_decode() -> Result<()> {
    let db = mem_db().await;
    db.query(
        r#"
        DEFINE TABLE IF NOT EXISTS temporal_node SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS tenant_id ON temporal_node TYPE string;
        "#,
    )
    .await?;

    let client = StasisSurrealDbClient::new(db);

    let err = client
        .raw_query(
            "CREATE temporal_node:`bad` SET tenant_id = $tenant_id",
            QueryParams::from([("tenant_id".to_string(), json!(123))]),
        )
        .await
        .expect_err("type mismatch should fail");

    let message = format!("{err:#}");
    assert!(
        !message.contains("decode failed"),
        "expected surreal schema error, got: {message}"
    );
    assert!(
        message.contains("tenant_id") || message.contains("string"),
        "expected field/type context, got: {message}"
    );
    Ok(())
}

#[tokio::test]
async fn raw_query_create_returns_rows_as_vec() -> Result<()> {
    let db = mem_db().await;
    db.query(
        r#"
        DEFINE TABLE IF NOT EXISTS temporal_node SCHEMALESS;
        "#,
    )
    .await?;

    let client = StasisSurrealDbClient::new(db);

    let rows = client
        .raw_query(
            "CREATE temporal_node:`ok` SET tenant_id = $tenant_id",
            QueryParams::from([("tenant_id".to_string(), json!("default"))]),
        )
        .await?;

    assert!(!rows.is_empty(), "CREATE should return at least one row");
    Ok(())
}

#[tokio::test]
async fn raw_query_select_empty_returns_vec() -> Result<()> {
    let db = mem_db().await;
    db.query("DEFINE TABLE IF NOT EXISTS temporal_node SCHEMALESS;")
        .await?;

    let client = StasisSurrealDbClient::new(db);

    let rows = client
        .raw_query("SELECT * FROM temporal_node LIMIT 1", QueryParams::new())
        .await?;

    assert!(rows.is_empty());
    Ok(())
}

#[tokio::test]
async fn semantic_tag_queries_run_on_bundled_surrealql() -> Result<()> {
    let db = mem_db().await;
    let client = StasisSurrealDbClient::new(db.clone());
    let index = MedousaSurrealSemanticIndexStore::new(client);
    index.initialize_async().await?;

    db
        .query(
            r#"
            CREATE semantic_tag_index SET tenant_id = 'default', session_id = 'session-a',
                node_id = 'node-both', sync_key = 'sync-both', tag = 'memory', updated_at = time::now();
            CREATE semantic_tag_index SET tenant_id = 'default', session_id = 'session-a',
                node_id = 'node-both', sync_key = 'sync-both', tag = 'runtime', updated_at = time::now();
            CREATE semantic_tag_index SET tenant_id = 'default', session_id = 'session-a',
                node_id = 'node-one', sync_key = 'sync-one', tag = 'memory', updated_at = time::now();
            "#,
        )
        .await?
        .check()?;

    let sync_keys = index
        .find_sync_keys_by_tags_async(
            "default",
            &["memory".into(), "runtime".into()],
            true,
            Some("session-a"),
            10,
        )
        .await?;

    assert_eq!(sync_keys, vec!["sync-both"]);

    let mut sync_keys = index
        .find_sync_keys_by_tags_async("default", &["memory".into()], false, Some("session-a"), 10)
        .await?;
    sync_keys.sort();

    assert_eq!(sync_keys, vec!["sync-both", "sync-one"]);
    Ok(())
}

#[tokio::test]
async fn semantic_tag_sync_is_recallable_on_bundled_surrealql() -> Result<()> {
    let db = mem_db().await;
    let client = StasisSurrealDbClient::new(db);
    let index = MedousaSurrealSemanticIndexStore::new(client);
    index.initialize_async().await?;

    let node_ref = SemanticTagNodeRef {
        tenant_id: "default".into(),
        session_id: "session-a".into(),
        node_id: "node-written".into(),
        sync_key: "sync-written".into(),
    };
    index
        .sync_node_tags_async(
            node_ref.clone(),
            &["memory".into(), "runtime".into()],
            None,
        )
        .await?;

    let sync_keys = index
        .find_sync_keys_by_tags_async(
            "default",
            &["memory".into(), "runtime".into()],
            true,
            Some("session-a"),
            10,
        )
        .await?;

    assert_eq!(sync_keys, vec!["sync-written"]);

    index
        .sync_node_tags_async(node_ref, &["memory".into(), "durable".into()], None)
        .await?;
    let old_keys = index
        .find_sync_keys_by_tags_async(
            "default",
            &["memory".into(), "runtime".into()],
            true,
            Some("session-a"),
            10,
        )
        .await?;
    let new_keys = index
        .find_sync_keys_by_tags_async(
            "default",
            &["memory".into(), "durable".into()],
            true,
            Some("session-a"),
            10,
        )
        .await?;

    assert!(old_keys.is_empty());
    assert_eq!(new_keys, vec!["sync-written"]);
    Ok(())
}
