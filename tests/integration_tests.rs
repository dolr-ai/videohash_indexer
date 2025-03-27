// tests/integration_tests.rs

use actix_web::{test, web, App};
use videohash_indexer::{
    create_shared_index, delete_hash, search, SearchRequest, VideoHashIndex,
};
use std::sync::Arc;

#[actix_web::test]
async fn test_search_add_new_hash() {
    let shared_index = create_shared_index();
    
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(shared_index.clone()))
            .route("/search", web::post().to(search))
    ).await;
    
    let req = test::TestRequest::post()
        .uri("/search")
        .set_json(&SearchRequest {
            video_id: "test-video-1".to_string(),
            hash: "0".repeat(64),
        })
        .to_request();
    
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    
    let body = test::read_body(resp).await;
    let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(response["match_found"], false);
    assert_eq!(response["hash_added"], true);
}

#[actix_web::test]
async fn test_search_find_similar_hash() {
    let shared_index = create_shared_index();
    
    // First add a hash
    shared_index.add(
        "test-video-1".to_string(), 
        &videohash_indexer::VideoHash { hash: "0".repeat(64) }
    ).unwrap();
    
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(shared_index.clone()))
            .route("/search", web::post().to(search))
    ).await;
    
    // Search with a slightly different hash (5 bits different)
    let req = test::TestRequest::post()
        .uri("/search")
        .set_json(&SearchRequest {
            video_id: "test-video-2".to_string(),
            hash: "0".repeat(59) + "11111",
        })
        .to_request();
    
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    
    let body = test::read_body(resp).await;
    let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(response["match_found"], true);
    assert_eq!(response["match_details"]["video_id"], "test-video-1");
    assert!(response["match_details"]["similarity_percentage"].as_f64().unwrap() > 90.0);
}

#[actix_web::test]
async fn test_delete_hash() {
    let shared_index = create_shared_index();
    
    // First add a hash
    shared_index.add(
        "test-video-1".to_string(), 
        &videohash_indexer::VideoHash { hash: "0".repeat(64) }
    ).unwrap();
    
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(shared_index.clone()))
            .route("/hash/{video_id}", web::delete().to(delete_hash))
    ).await;
    
    // Delete the hash
    let req = test::TestRequest::delete()
        .uri("/hash/test-video-1")
        .to_request();
    
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    
    // Verify it's deleted
    assert_eq!(shared_index.len(), 0);
}