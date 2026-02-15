use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

pub struct BybitAuth {
    api_key: String,
    api_secret: String,
}

impl BybitAuth {
    pub fn new(api_key: String, api_secret: String) -> Self {
        Self { api_key, api_secret }
    }
    
    pub fn get_api_key(&self) -> &str {
        &self.api_key
    }
    
    /// Generate HMAC signature for REST API requests
    pub fn generate_signature(&self, timestamp: u64, params: &str) -> String {
        let sign_str = format!("{}{}{}", timestamp, &self.api_key, params);
        
        let mut mac = HmacSha256::new_from_slice(self.api_secret.as_bytes())
            .expect("HMAC can take key of any size");
        
        mac.update(sign_str.as_bytes());
        
        hex::encode(mac.finalize().into_bytes())
    }
    
    /// Get current timestamp in milliseconds
    pub fn get_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis() as u64
    }
    
    /// Generate WebSocket authentication payload
    pub fn generate_ws_auth(&self) -> (String, String, String) {
        let expires = Self::get_timestamp() + 10000; // 10 seconds from now
        let sign_str = format!("GET/realtime{}", expires);
        
        let mut mac = HmacSha256::new_from_slice(self.api_secret.as_bytes())
            .expect("HMAC can take key of any size");
        
        mac.update(sign_str.as_bytes());
        
        let signature = hex::encode(mac.finalize().into_bytes());
        
        (self.api_key.clone(), expires.to_string(), signature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_signature_generation() {
        let auth = BybitAuth::new("test_key".to_string(), "test_secret".to_string());
        let timestamp = 1234567890000u64;
        let params = "symbol=BTCUSDT&side=Buy";
        
        let signature = auth.generate_signature(timestamp, params);
        assert!(!signature.is_empty());
        assert_eq!(signature.len(), 64); // SHA256 produces 64 hex characters
    }
    
    #[test]
    fn test_ws_auth_generation() {
        let auth = BybitAuth::new("test_key".to_string(), "test_secret".to_string());
        let (api_key, expires, signature) = auth.generate_ws_auth();
        
        assert_eq!(api_key, "test_key");
        assert!(!expires.is_empty());
        assert_eq!(signature.len(), 64);
    }
}
