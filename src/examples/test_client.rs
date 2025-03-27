// examples/test_client.rs

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Serialize)]
struct SearchRequest {
    video_id: String,
    hash: String,
}

#[derive(Deserialize, Debug)]
struct VideoMatch {
    video_id: String,
    similarity_percentage: f64,
    is_duplicate: bool,
}

#[derive(Deserialize, Debug)]
struct SearchResponse {
    match_found: bool,
    match_details: Option<VideoMatch>,
    hash_added: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let client = Client::new();
    
    // Add first hash
    let req1 = SearchRequest {
        video_id: "video-001".to_string(),
        hash: "0".repeat(64),
    };
    
    let resp1 = client.post("http://localhost:8080/search")
        .json(&req1)
        .send()
        .await?
        .json::<SearchResponse>()
        .await?;
    
    println!("First response: {:?}", resp1);
    
    // Add similar hash
    let req2 = SearchRequest {
        video_id: "video-002".to_string(),
        hash: "0".repeat(60) + "1111",
    };
    
    let resp2 = client.post("http://localhost:8080/search")
        .json(&req2)
        .send()
        .await?
        .json::<SearchResponse>()
        .await?;
    
    println!("Second response: {:?}", resp2);
    
    // Delete first hash
    let delete_resp = client.delete("http://localhost:8080/hash/video-001")
        .send()
        .await?;
    
    println!("Delete status: {}", delete_resp.status());
    
    Ok(())
}