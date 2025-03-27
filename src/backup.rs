use std::error::Error;
use std::env;
use chrono::Utc;
use tokio::sync::OnceCell;
use lazy_static::lazy_static;

use super::videohash::VideoHash;

// Import the BigQuery client from off-chain-agent
// You'll need to add this dependency to your Cargo.toml
// off_chain_agent = { git = "https://github.com/yral-dapp/off-chain-agent" }
use off_chain_agent::bigquery::{BigQueryClient, BigQueryConfig, Row, TableSchema, SchemaField, FieldType};

lazy_static! {
    static ref BQ_CLIENT: OnceCell<BigQueryClient> = OnceCell::new();
}

async fn get_client() -> Result<&'static BigQueryClient, Box<dyn Error + Send + Sync>> {
    BQ_CLIENT.get_or_try_init(|| async {
        let project_id = env::var("BQ_PROJECT_ID")
            .map_err(|_| "BQ_PROJECT_ID environment variable not set")?;
        let dataset_id = env::var("BQ_DATASET_ID")
            .map_err(|_| "BQ_DATASET_ID environment variable not set")?;
        
        let config = BigQueryConfig {
            project_id,
            dataset_id,
            // Add any other required configuration from off-chain-agent
        };
        
        BigQueryClient::new(config).await
            .map_err(|e| Box::<dyn Error + Send + Sync>::from(format!("Failed to create BigQuery client: {}", e)))
    }).await
}

async fn ensure_table_exists() -> Result<(), Box<dyn Error + Send + Sync>> {
    let client = get_client().await?;
    let table_id = env::var("BQ_TABLE_ID")
        .map_err(|_| "BQ_TABLE_ID environment variable not set")?;
    
    // Define the schema using off-chain-agent's schema definition
    let schema = TableSchema {
        fields: vec![
            SchemaField {
                name: "video_id".to_string(),
                field_type: FieldType::String,
                mode: "REQUIRED".to_string(),
                description: Some("Unique identifier for the video".to_string()),
                ..Default::default()
            },
            SchemaField {
                name: "hash".to_string(),
                field_type: FieldType::String,
                mode: "REQUIRED".to_string(),
                description: Some("64-bit binary hash as string".to_string()),
                ..Default::default()
            },
            SchemaField {
                name: "timestamp".to_string(),
                field_type: FieldType::Timestamp,
                mode: "REQUIRED".to_string(),
                description: Some("When the hash was added".to_string()),
                ..Default::default()
            },
        ],
    };
    
    // Check if table exists, if not create it
    if !client.table_exists(&table_id).await? {
        client.create_table(&table_id, schema).await?;
        log::info!("Created BigQuery table {}", table_id);
    }
    
    Ok(())
}

// Add this function to the backup.rs file

async fn with_retry<F, Fut, T>(operation: F, max_retries: usize) -> Result<T, Box<dyn Error + Send + Sync>>
where
    F: Fn() -> Fut + Send,
    Fut: std::future::Future<Output = Result<T, Box<dyn Error + Send + Sync>>> + Send,
    T: Send + 'static,
{
    let mut retries = 0;
    let mut last_error = None;
    
    while retries < max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                let delay = std::time::Duration::from_millis(500 * 2u64.pow(retries as u32));
                log::warn!("Operation failed (retry {}/{}): {}. Retrying in {:?}...", 
                          retries + 1, max_retries, e, delay);
                tokio::time::sleep(delay).await;
                last_error = Some(e);
                retries += 1;
            }
        }
    }
    
    Err(last_error.unwrap_or_else(|| "Unknown error after retries".into()))
}

// Add this function to check BigQuery health

pub async fn check_bigquery_health() -> Result<bool, Box<dyn Error + Send + Sync>> {
    with_retry(|| async {
        let client = get_client().await?;
        let table_id = env::var("BQ_TABLE_ID")
            .map_err(|_| "BQ_TABLE_ID environment variable not set")?;
        
        // Check if we can access the table
        let exists = client.table_exists(&table_id).await?;
        Ok(exists)
    }, 1).await
}

// Then modify backup_hash to use retry:
pub async fn backup_hash(
    video_id: &str,
    hash: &VideoHash,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    // Ensure the table exists
    ensure_table_exists().await?;
    
    let video_id = video_id.to_string();
    let hash_clone = hash.clone();
    
    with_retry(move || {
        let video_id = video_id.clone();
        let hash = hash_clone.clone();
        async move {
            let client = get_client().await?;
            let table_id = env::var("BQ_TABLE_ID")
                .map_err(|_| "BQ_TABLE_ID environment variable not set")?;
            
            let row = Row::new()
                .with_string("video_id", video_id.clone())
                .with_string("hash", hash.hash.clone())
                .with_timestamp("timestamp", Utc::now());
            
            client.insert_row(&table_id, row).await?;
            
            log::info!("Successfully backed up hash for video_id {} to BigQuery", video_id);
            Ok(true)
        }
    }, 3).await
}

// Bulk backup function for startup or batch operations
pub async fn backup_all(entries: Vec<(String, VideoHash)>) -> Result<(), Box<dyn Error + Send + Sync>> {
    if entries.is_empty() {
        log::info!("No entries to back up");
        return Ok(());
    }
    
    log::info!("Starting bulk backup of {} entries to BigQuery", entries.len());
    
    // Ensure the table exists
    ensure_table_exists().await?;
    
    let client = get_client().await?;
    let table_id = env::var("BQ_TABLE_ID")
        .map_err(|_| "BQ_TABLE_ID environment variable not set")?;
    
    // Process in batches of 500 to avoid request size limits
    const BATCH_SIZE: usize = 500;
    for chunk in entries.chunks(BATCH_SIZE) {
        let mut rows = Vec::with_capacity(chunk.len());
        
        for (video_id, hash) in chunk {
            let row = Row::new()
                .with_string("video_id", video_id.clone())
                .with_string("hash", hash.hash.clone())
                .with_timestamp("timestamp", Utc::now());
            
            rows.push(row);
        }
        
        // Insert rows in batch
        client.insert_rows(&table_id, rows).await?;
        
        log::info!("Backed up batch of {} entries to BigQuery", chunk.len());
    }
    
    log::info!("Completed bulk backup of all entries to BigQuery");
    Ok(())
}