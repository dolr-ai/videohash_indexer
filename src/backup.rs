use super::videohash::VideoHash;
use std::error::Error;
use uuid::Uuid;

/// Backup a video hash to persistence storage
/// Currently just logs the operation, can be extended with real BigQuery implementation later
pub async fn backup_hash(
    uuid: &Uuid,
    hash: &VideoHash,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    todo!("Implement real backup to BigQuery");
}
