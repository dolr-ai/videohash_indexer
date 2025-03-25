use std::error::Error;

pub struct VideoHash {
    pub hash: String,
}

impl VideoHash {
    pub fn from_binary_string(binary_str: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        if binary_str.len() != 64 {
            return Err(format!("Binary string must be 64 bits, got {}", binary_str.len()).into());
        }

        for ch in binary_str.chars() {
            if ch != '0' && ch != '1' {
                return Err(format!("Invalid character in binary string: {}", ch).into());
            }
        }

        Ok(Self {
            hash: binary_str.to_string(),
        })
    }
}
