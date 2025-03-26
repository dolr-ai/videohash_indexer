use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, RwLock};

use mih_rs::Index;
use uuid::Uuid;

use super::videohash::VideoHash;

fn binary_string_to_u64(binary_str: &str) -> Result<u64, Box<dyn Error + Send + Sync>> {
    if binary_str.len() != 64 {
        return Err(format!("Binary string must be 64 bits, got {}", binary_str.len()).into());
    }
    
    u64::from_str_radix(binary_str, 2)
        .map_err(|e| format!("Invalid binary string: {}", e).into())
}

pub struct VideoHashIndex {
    hashes: RwLock<HashMap<Uuid, u64>>,
    index: RwLock<Option<(Index<u64>, Vec<Uuid>)>>, // Store UUIDs alongside the index
}

impl VideoHashIndex {
    pub fn new() -> Self {
        Self {
            hashes: RwLock::new(HashMap::new()),
            index: RwLock::new(None),
        }
    }

    pub fn add(&self, uuid: Uuid, hash: &VideoHash) -> Result<(), Box<dyn Error + Send + Sync>> {
        let hash_value = binary_string_to_u64(&hash.hash)?;

        let mut hashes = self.hashes.write().unwrap();
        hashes.insert(uuid, hash_value);

        // Invalidate the index when the hash map changes
        let mut index = self.index.write().unwrap();
        *index = None;

        Ok(())
    }

    fn ensure_index_built(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut index_lock = self.index.write().unwrap();

        if index_lock.is_none() {
            let hashes = self.hashes.read().unwrap();
            if hashes.is_empty() {
                *index_lock = None;
                return Ok(());
            }

            // Create ordered vectors of UUIDs and hash values to ensure consistent ordering
            let mut uuid_hash_pairs: Vec<(Uuid, u64)> = hashes.iter()
                .map(|(&uuid, &hash)| (uuid, hash))
                .collect();

            // Sort by UUID to ensure deterministic ordering
            uuid_hash_pairs.sort_by_key(|(uuid, _)| *uuid);

            // Split into separate vectors
            let uuids: Vec<Uuid> = uuid_hash_pairs.iter().map(|(uuid, _)| *uuid).collect();
            let codes: Vec<u64> = uuid_hash_pairs.iter().map(|(_, code)| *code).collect();

            // Create the index with the ordered hash values
            *index_lock = Some((Index::new(codes)?, uuids));
        }

        Ok(())
    }

    pub fn find_nearest_neighbor(
        &self,
        hash: &VideoHash,
    ) -> Result<Option<(Uuid, u32)>, Box<dyn Error + Send + Sync>> {
        let hash_value = binary_string_to_u64(&hash.hash)?;

        self.ensure_index_built()?;

        let index_lock = self.index.read().unwrap();
        if index_lock.is_none() {
            return Ok(None);
        }

        let (index, uuids) = index_lock.as_ref().unwrap();
        
        let mut searcher = index.topk_searcher();
        let answers = searcher.run(hash_value, 1);

        if answers.is_empty() {
            return Ok(None);
        }

        let idx = answers[0] as usize;
        if idx >= uuids.len() {
            return Err("Index inconsistency: invalid vector index".into());
        }

        let uuid = uuids[idx];
        let hashes = self.hashes.read().unwrap();
        let stored_hash = *hashes.get(&uuid).unwrap();
        let hamming_dist = (hash_value ^ stored_hash).count_ones();

        Ok(Some((uuid, hamming_dist)))
    }

    pub fn find_within_distance(
        &self,
        hash: &VideoHash,
        max_distance: u32,
    ) -> Result<Vec<(Uuid, u32)>, Box<dyn Error + Send + Sync>> {
        let hash_value = binary_string_to_u64(&hash.hash)?;

        self.ensure_index_built()?;

        let index_lock = self.index.read().unwrap();
        if index_lock.is_none() {
            return Ok(Vec::new());
        }

        let (index, uuids) = index_lock.as_ref().unwrap();
        let hashes = self.hashes.read().unwrap();

        let mut searcher = index.range_searcher();
        let answers = searcher.run(hash_value, max_distance as usize);

        let mut neighbors = Vec::new();
        for idx in answers {
            let idx_usize = *idx as usize;
            if idx_usize < uuids.len() {
                let uuid = uuids[idx_usize];
                let stored_hash = *hashes.get(&uuid).unwrap();
                let hamming_dist = (hash_value ^ stored_hash).count_ones();
                neighbors.push((uuid, hamming_dist));
            }
        }

        neighbors.sort_by_key(|&(_, dist)| dist);

        Ok(neighbors)
    }

    pub fn remove(&self, uuid: &Uuid) -> Result<bool, Box<dyn Error + Send + Sync>> {
        let mut hashes = self.hashes.write().unwrap();
        let removed = hashes.remove(uuid).is_some();

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

    // Remove unnecessary methods
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

        let uuid1 = Uuid::new_v4();
        let uuid2 = Uuid::new_v4();
        let uuid3 = Uuid::new_v4();

        index.add(uuid1, &hash1)?;
        index.add(uuid2, &hash2)?;
        index.add(uuid3, &hash3)?;

        let result = index.find_nearest_neighbor(&hash1)?;
        assert!(result.is_some());
        let (found_uuid, distance) = result.unwrap();
        assert_eq!(found_uuid, uuid1);
        assert_eq!(distance, 0);

        let query = VideoHash {
            hash: "0".repeat(60) + &"1".repeat(4),
        };
        let result = index.find_nearest_neighbor(&query)?;
        assert!(result.is_some());
        let (found_uuid, distance) = result.unwrap();
        assert_eq!(found_uuid, uuid1);
        assert_eq!(distance, 4);

        Ok(())
    }
    
    #[test]
    fn test_consistent_ordering() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let index = VideoHashIndex::new();

        // Add hashes in random order
        let uuid1 = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let uuid2 = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let uuid3 = Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();
        
        let hash1 = VideoHash { hash: "0".repeat(64) };
        let hash2 = VideoHash { hash: "1".repeat(64) };
        let hash3 = VideoHash { hash: "0".repeat(32) + &"1".repeat(32) };

        // Add in non-sequential order
        index.add(uuid2, &hash2)?;
        index.add(uuid3, &hash3)?;
        index.add(uuid1, &hash1)?;

        // Test if UUIDs are mapped correctly
        let result = index.find_nearest_neighbor(&hash1)?;
        assert!(result.is_some());
        let (found_uuid, distance) = result.unwrap();
        assert_eq!(found_uuid, uuid1);
        assert_eq!(distance, 0);
        
        // Test once more to ensure the order is consistent
        let result = index.find_nearest_neighbor(&hash1)?;
        assert!(result.is_some());
        let (found_uuid, distance) = result.unwrap();
        assert_eq!(found_uuid, uuid1);
        assert_eq!(distance, 0);

        Ok(())
    }
}
