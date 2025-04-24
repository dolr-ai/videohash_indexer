use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, RwLock};

use crate::bigquery;
use mih_rs::Index;

use super::videohash::VideoHash;

fn binary_string_to_u64(binary_str: &str) -> Result<u64, Box<dyn Error + Send + Sync>> {
    if binary_str.len() != 64 {
        return Err(format!("Binary string must be 64 bits, got {}", binary_str.len()).into());
    }

    u64::from_str_radix(binary_str, 2).map_err(|e| format!("Invalid binary string: {}", e).into())
}

pub struct VideoHashIndex {
    hashes: RwLock<HashMap<String, u64>>,
    index: RwLock<Option<(Index<u64>, Vec<String>)>>, // Store video_ids alongside the index
}

impl VideoHashIndex {
    pub fn new() -> Self {
        Self {
            hashes: RwLock::new(HashMap::new()),
            index: RwLock::new(None),
        }
    }

    pub fn add(
        &self,
        video_id: String,
        hash: &VideoHash,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let hash_value = binary_string_to_u64(&hash.hash)?;

        let mut index = self.index.write().unwrap();
        *index = None;

        let mut hashes = self.hashes.write().unwrap();
        hashes.insert(video_id, hash_value);

        Ok(())
    }

    pub fn has_exact_match(
        &self,
        video_id: &str,
        hash: &VideoHash,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        let hash_value = binary_string_to_u64(&hash.hash)?;

        let hashes = self.hashes.read().unwrap();
        if let Some(&existing_hash) = hashes.get(video_id) {
            return Ok(existing_hash == hash_value);
        }

        Ok(false)
    }

    fn ensure_index_built(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut index_lock = self.index.write().unwrap();

        if index_lock.is_none() {
            let hashes = self.hashes.read().unwrap();
            if hashes.is_empty() {
                *index_lock = None;
                return Ok(());
            }

            // Create ordered vectors of video_ids and hash values to ensure consistent ordering
            let mut video_id_hash_pairs: Vec<(String, u64)> = hashes
                .iter()
                .map(|(video_id, &hash)| (video_id.clone(), hash))
                .collect();

            // Split into separate vectors
            let video_ids: Vec<String> = video_id_hash_pairs
                .iter()
                .map(|(id, _)| id.clone())
                .collect();
            let codes: Vec<u64> = video_id_hash_pairs.iter().map(|(_, code)| *code).collect();

            // Create the index with explicit number of blocks (8 for 64-bit hashes)
            // This is more appropriate than Index::new() which might choose inappropriate parameters
            match mih_rs::Index::with_blocks(codes, 8) {
                Ok(new_index) => {
                    *index_lock = Some((new_index, video_ids));
                }
                Err(e) => {
                    return Err(format!("Failed to create MIH index: {}", e).into());
                }
            }
        }

        Ok(())
    }

    pub fn find_nearest_neighbor(
        &self,
        hash: &VideoHash,
    ) -> Result<Option<(String, u32)>, Box<dyn Error + Send + Sync>> {
        let hash_value = binary_string_to_u64(&hash.hash)?;

        self.ensure_index_built()?;

        let index_lock = self.index.read().unwrap();
        if index_lock.is_none() {
            return Ok(None);
        }

        let (index, video_ids) = index_lock.as_ref().unwrap();

        let mut searcher = index.topk_searcher();
        let answers = searcher.run(hash_value, 1);

        if answers.is_empty() {
            return Ok(None);
        }

        let idx = answers[0] as usize;
        if idx >= video_ids.len() {
            return Err("Index inconsistency: invalid vector index".into());
        }

        let video_id = video_ids[idx].clone();
        let hashes = self.hashes.read().unwrap();
        let stored_hash = *hashes.get(&video_id).unwrap();
        let hamming_dist = (hash_value ^ stored_hash).count_ones();

        Ok(Some((video_id, hamming_dist)))
    }

    pub fn find_within_distance(
        &self,
        hash: &VideoHash,
        max_distance: u32,
    ) -> Result<Vec<(String, u32)>, Box<dyn Error + Send + Sync>> {
        let hash_value = binary_string_to_u64(&hash.hash)?;

        self.ensure_index_built()?;

        let index_lock = self.index.read().unwrap();
        if index_lock.is_none() {
            return Ok(Vec::new());
        }

        let (index, video_ids) = index_lock.as_ref().unwrap();
        let hashes = self.hashes.read().unwrap();

        let mut searcher = index.range_searcher();
        let answers = searcher.run(hash_value, max_distance as usize);

        let mut neighbors = Vec::new();
        for idx in answers {
            let idx_usize = *idx as usize;
            if idx_usize < video_ids.len() {
                let video_id = video_ids[idx_usize].clone();
                let stored_hash = *hashes.get(&video_id).unwrap();
                let hamming_dist = (hash_value ^ stored_hash).count_ones();
                neighbors.push((video_id, hamming_dist));
            }
        }

        neighbors.sort_by_key(|&(_, dist)| dist);

        Ok(neighbors)
    }

    pub fn remove(&self, video_id: &str) -> Result<bool, Box<dyn Error + Send + Sync>> {
        let mut hashes = self.hashes.write().unwrap();
        let removed = hashes.remove(video_id).is_some();

        if removed {
            let mut index = self.index.write().unwrap();
            *index = None;
        }

        Ok(removed)
    }

    pub fn len(&self) -> usize {
        self.hashes.read().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub async fn rebuild_from_bigquery(&self) -> Result<usize, Box<dyn Error + Send + Sync>> {
        log::info!("Starting index rebuild from BigQuery...");
        let video_hashes = bigquery::fetch_video_hashes().await?;

        {
            let mut hashes = self.hashes.write().unwrap();
            hashes.clear();

            for (video_id, hash) in video_hashes.iter() {
                let hash_value = binary_string_to_u64(&hash.hash)?;
                hashes.insert(video_id.clone(), hash_value);
            }

            let mut index = self.index.write().unwrap();
            *index = None;
        }

        self.ensure_index_built()?;

        let count = self.len();
        log::info!("Rebuilt index with {} hashes from BigQuery", count);
        Ok(count)
    }

    pub fn needs_rebuild(&self) -> bool {
        self.is_empty()
    }
}

pub fn create_shared_index() -> Arc<VideoHashIndex> {
    Arc::new(VideoHashIndex::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binary_string_to_u64() {
        let all_ones = "1".repeat(64);
        assert_eq!(binary_string_to_u64(&all_ones).unwrap(), u64::MAX);

        let all_zeros = "0".repeat(64);
        assert_eq!(binary_string_to_u64(&all_zeros).unwrap(), 0);

        let mixed = "1010".repeat(16);
        let expected = 0xAAAAAAAAAAAAAAAAu64;
        assert_eq!(binary_string_to_u64(&mixed).unwrap(), expected);
    }

    #[test]
    fn test_add_and_find() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let index = VideoHashIndex::new();

        let hash1 = VideoHash {
            hash: "0".repeat(64),
        };
        let hash2 = VideoHash {
            hash: "1".repeat(64),
        };
        let hash3 = VideoHash {
            hash: "0".repeat(32) + &"1".repeat(32),
        };

        let video_id1 = "video-001".to_string();
        let video_id2 = "video-002".to_string();
        let video_id3 = "video-003".to_string();

        index.add(video_id1.clone(), &hash1)?;
        index.add(video_id2.clone(), &hash2)?;
        index.add(video_id3.clone(), &hash3)?;

        let result = index.find_nearest_neighbor(&hash1)?;
        assert!(result.is_some());
        let (found_id, distance) = result.unwrap();
        assert_eq!(found_id, video_id1);
        assert_eq!(distance, 0);

        let query = VideoHash {
            hash: "0".repeat(60) + &"1".repeat(4),
        };
        let result = index.find_nearest_neighbor(&query)?;
        assert!(result.is_some());
        let (found_id, distance) = result.unwrap();
        assert_eq!(found_id, video_id1);
        assert_eq!(distance, 4);

        Ok(())
    }

    #[test]
    fn test_consistent_ordering() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let index = VideoHashIndex::new();

        // Add hashes in random order
        let video_id1 = "video-001".to_string();
        let video_id2 = "video-002".to_string();
        let video_id3 = "video-003".to_string();

        let hash1 = VideoHash {
            hash: "0".repeat(64),
        };
        let hash2 = VideoHash {
            hash: "1".repeat(64),
        };
        let hash3 = VideoHash {
            hash: "0".repeat(32) + &"1".repeat(32),
        };

        // Add in non-sequential order
        index.add(video_id2.clone(), &hash2)?;
        index.add(video_id3.clone(), &hash3)?;
        index.add(video_id1.clone(), &hash1)?;

        // Test if video_ids are mapped correctly
        let result = index.find_nearest_neighbor(&hash1)?;
        assert!(result.is_some());
        let (found_id, distance) = result.unwrap();
        assert_eq!(found_id, video_id1);
        assert_eq!(distance, 0);

        // Test once more to ensure the order is consistent
        let result = index.find_nearest_neighbor(&hash1)?;
        assert!(result.is_some());
        let (found_id, distance) = result.unwrap();
        assert_eq!(found_id, video_id1);
        assert_eq!(distance, 0);

        Ok(())
    }
}
