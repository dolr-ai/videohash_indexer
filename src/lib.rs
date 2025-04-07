pub mod bigquery;
pub mod index;
pub mod videohash;
pub use index::{create_shared_index, VideoHashIndex};
pub use videohash::VideoHash;

use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize)]
pub struct VideoMatch {
    pub video_id: String,
    pub similarity_percentage: f64,
    pub is_duplicate: bool,
}

#[derive(Serialize)]
pub struct SearchResponse {
    pub match_found: bool,
    pub match_details: Option<VideoMatch>,
    pub hash_added: bool,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Deserialize, Serialize)]
pub struct SearchRequest {
    pub video_id: String,
    pub hash: String,
}

pub async fn search(
    req: web::Json<SearchRequest>,
    index: web::Data<Arc<VideoHashIndex>>,
) -> HttpResponse {
    const MAX_HAMMING_DISTANCE: u32 = 10;

    let query_hash = match VideoHash::from_binary_string(&req.hash) {
        Ok(hash) => hash,
        Err(e) => {
            return HttpResponse::BadRequest().json(ErrorResponse {
                error: format!("Invalid hash format: {}", e),
            });
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
        };

        HttpResponse::Ok().json(response)
    } else {
        match index.add(req.video_id.clone(), &query_hash) {
            Ok(_) => {
                let response = SearchResponse {
                    match_found: false,
                    match_details: None,
                    hash_added: true,
                };

                HttpResponse::Ok().json(response)
            }
            Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
                error: format!("Failed to add hash: {}", e),
            }),
        }
    }
}

pub async fn delete_hash(
    path: web::Path<String>,
    index: web::Data<Arc<VideoHashIndex>>,
) -> HttpResponse {
    let video_id = path.into_inner();

    match index.remove(&video_id) {
        Ok(true) => HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "message": format!("Hash with video_id {} successfully deleted", video_id)
        })),
        Ok(false) => HttpResponse::NotFound().json(ErrorResponse {
            error: format!("Hash with video_id {} not found", video_id),
        }),
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse {
            error: format!("Failed to remove hash: {}", e),
        }),
    }
}
