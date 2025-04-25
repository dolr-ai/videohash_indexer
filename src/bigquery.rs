use std::env;
use std::error::Error;

use google_cloud_bigquery::client::google_cloud_auth::credentials::CredentialsFile;
use google_cloud_bigquery::client::{Client, ClientConfig};
use google_cloud_bigquery::http::job::query::QueryRequest;
use google_cloud_bigquery::http::tabledata::list::Value;

use crate::videohash::VideoHash;

pub async fn fetch_video_hashes() -> Result<Vec<(String, VideoHash)>, Box<dyn Error + Send + Sync>>
{
    let (client, project_id) = create_bigquery_client().await?;
    let mut results = Vec::new();
    let batch_size = 50000;
    let mut offset = 0;
    
    loop {
        let query_sql = format!(r#"
            SELECT video_id, videohash 
            FROM `hot-or-not-feed-intelligence.yral_ds.video_unique`
            ORDER BY created_at DESC
            LIMIT {batch_size} OFFSET {offset}
        "#);

        log::info!("Executing BigQuery query to fetch video hashes (batch: {}, offset: {})", batch_size, offset);

        let request = QueryRequest {
            query: query_sql,
            use_legacy_sql: false,
            ..Default::default()
        };

        let query_response = client
            .job()
            .query(&project_id, &request)
            .await
            .map_err(|e| format!("Failed to execute BigQuery query: {}", e))?;

        let row_count = query_response.rows.as_ref().map_or(0, |rows| rows.len());
        log::info!("BigQuery response: query successful, returned {} rows", row_count);

        // Process rows
        if let Some(rows) = query_response.rows {
            if rows.is_empty() {
                // No more results to fetch
                break;
            }
            
            for row in rows {
                let f = &row.f;

                if f.len() >= 2 {
                    let video_id = match extract_string_from_value(&f[0].v) {
                        Some(id) => id,
                        None => continue,
                    };

                    let hash_string = match extract_string_from_value(&f[1].v) {
                        Some(hash) => hash,
                        None => continue,
                    };

                    match VideoHash::from_binary_string(&hash_string) {
                        Ok(hash) => {
                            results.push((video_id, hash));
                        }
                        Err(e) => {
                            log::warn!("Failed to parse hash for video_id {}: {}", video_id, e);
                        }
                    }
                }
            }
            
            // Increase offset for next batch
            offset += row_count;
            
            // If we got fewer rows than requested, we've reached the end
            if row_count < batch_size {
                break;
            }
        } else {
            // No rows returned
            break;
        }
    }

    log::info!("Loaded {} video hashes from BigQuery in total", results.len());
    Ok(results)
}

fn extract_string_from_value(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

async fn create_bigquery_client() -> Result<(Client, String), Box<dyn Error + Send + Sync>> {
    if let Ok(sa_key_json) = env::var("GOOGLE_SA_KEY") {
        log::info!("Creating BigQuery client with GOOGLE_SA_KEY");

        let cred = CredentialsFile::new_from_str(&sa_key_json)
            .await
            .map_err(|e| format!("Failed to parse service account credentials: {}", e))?;

        let project_id = env::var("GOOGLE_CLOUD_PROJECT")
            .map_err(|_| "GOOGLE_CLOUD_PROJECT environment variable is required")?;

        let (config, _) = ClientConfig::new_with_credentials(cred)
            .await
            .map_err(|e| format!("Failed to create client config with credentials: {}", e))?;

        let client = Client::new(config)
            .await
            .map_err(|e| format!("Failed to create BigQuery client: {}", e))?;

        return Ok((client, project_id));
    }

    if let Ok(creds_path) = env::var("GOOGLE_APPLICATION_CREDENTIALS") {
        log::info!(
            "Creating BigQuery client with credentials file: {}",
            creds_path
        );

        let cred = CredentialsFile::new_from_file(creds_path)
            .await
            .map_err(|e| format!("Failed to load credentials from file: {}", e))?;

        let project_id = env::var("GOOGLE_CLOUD_PROJECT")
            .map_err(|_| "GOOGLE_CLOUD_PROJECT environment variable is required")?;

        let (config, _) = ClientConfig::new_with_credentials(cred)
            .await
            .map_err(|e| format!("Failed to create client config with credentials: {}", e))?;

        let client = Client::new(config)
            .await
            .map_err(|e| format!("Failed to create BigQuery client: {}", e))?;

        return Ok((client, project_id));
    }

    log::info!("Creating BigQuery client with application default credentials");

    let project_id = env::var("GOOGLE_CLOUD_PROJECT")
        .map_err(|_| "GOOGLE_CLOUD_PROJECT environment variable is required when using application default credentials")?;

    let (config, _) = ClientConfig::new_with_auth().await.map_err(|e| {
        format!(
            "Failed to create client config with application default credentials: {}",
            e
        )
    })?;

    let client = Client::new(config)
        .await
        .map_err(|e| format!("Failed to create BigQuery client: {}", e))?;

    Ok((client, project_id))
}
