use actix_web::middleware::Logger;
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use dotenv::dotenv;
use env_logger::Env;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

mod backup;
mod index;
mod videohash;

use index::create_shared_index;
use index::VideoHashIndex;
use videohash::VideoHash;

#[derive(Serialize)]
struct VideoMatch {
    video_id: String,
    similarity_percentage: f64,
    is_duplicate: bool,
}

#[derive(Serialize)]
struct SearchResponse {
    match_found: bool,
    match_details: Option<VideoMatch>,
    hash_added: bool,
    backed_up: bool,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Deserialize)]
struct SearchRequest {
    video_id: String,
    hash: String,
}

async fn search(
    req: web::Json<SearchRequest>,
    index: web::Data<Arc<VideoHashIndex>>,
) -> impl Responder {
    const MAX_HAMMING_DISTANCE: u32 = 10;

    let query_hash = match VideoHash::from_binary_string(&req.hash) {
        Ok(hash) => hash,
        Err(e) => {
            return HttpResponse::BadRequest().json(ErrorResponse {
                error: format!("Invalid hash format: {}", e),
            });
        }
    };

    // Always backup to BigQuery
    let backed_up = match backup::backup_hash(&req.video_id, &query_hash).await {
        Ok(result) => result,
        Err(e) => {
            log::error!("Failed to backup hash to BigQuery: {}", e);
            false
        }
    };

    let similar_hashes = match index.find_within_distance(&query_hash, MAX_HAMMING_DISTANCE) {
        Ok(results) => results,
        Err(e) => {
            return HttpResponse::InternalServerError().json(ErrorResponse {
                error: format!("Search failed: {}", e),
            });
        }
    };

    if !similar_hashes.is_empty() {
        let (video_id, distance) = similar_hashes[0].clone();
        let similarity = 100.0 * (64.0 - distance as f64) / 64.0;

        let response = SearchResponse {
            match_found: true,
            match_details: Some(VideoMatch {
                video_id,
                similarity_percentage: similarity,
                is_duplicate: true,
            }),
            hash_added: false,
            backed_up,
        };

        HttpResponse::Ok().json(response)
    } else {
        match index.add(req.video_id.clone(), &query_hash) {
            Ok(_) => {
                let response = SearchResponse {
                    match_found: false,
                    match_details: None,
                    hash_added: true,
                    backed_up,
                };

                HttpResponse::Ok().json(response)
            }
            Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
                error: format!("Failed to add hash: {}", e),
            }),
        }
    }
}

async fn delete_hash(
    path: web::Path<String>,
    index: web::Data<Arc<VideoHashIndex>>,
) -> impl Responder {
    let video_id = path.into_inner();

    match index.remove(&video_id) {
        Ok(true) => HttpResponse::Ok().json(()),
        Ok(false) => HttpResponse::NotFound().json(ErrorResponse {
            error: format!("Hash with video_id {} not found", video_id),
        }),
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
            error: format!("Failed to remove hash: {}", e),
        }),
    }
}

async fn health_check(index: web::Data<Arc<VideoHashIndex>>) -> impl Responder {
    // Check if BigQuery connection is healthy
    let bq_status = match backup::check_bigquery_health().await {
        Ok(true) => "healthy",
        _ => "unhealthy", 
    };
    
    // Check if index is functioning
    let index_count = index.len();
    
    let health_status = serde_json::json!({
        "status": "ok",
        "components": {
            "index": {
                "status": "healthy",
                "count": index_count
            },
            "bigquery": {
                "status": bq_status
            }
        },
        "timestamp": chrono::Utc::now().to_rfc3339()
    });
    
    HttpResponse::Ok().json(health_status)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    env_logger::init_from_env(Env::default().default_filter_or("info"));

    let shared_index = create_shared_index();
    
    // Backup all existing entries when service starts
    let entries = shared_index.get_all_entries();
    let entry_count = entries.len();
    if entry_count > 0 {
        log::info!("Starting backup of {} existing entries on service startup", entry_count);
        match backup::backup_all(entries).await {
            Ok(_) => log::info!("Successfully backed up all {} existing entries", entry_count),
            Err(e) => log::error!("Failed to backup existing entries: {}", e),
        }
    } else {
        log::info!("No existing entries to back up on service startup");
    }

    println!("Starting videohash indexer service on http://0.0.0.0:8080");

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .app_data(web::Data::new(shared_index.clone()))
            .route("/search", web::post().to(search))
            .route("/hash/{video_id}", web::delete().to(delete_hash))
            .route("/health", web::get().to(health_check))
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
