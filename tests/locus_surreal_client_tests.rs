use anyhow::Result;
use locus_core_rs::storage::surrealdb::client::{QueryParams, SurrealDbClient};
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
