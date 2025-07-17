// src/utils/validation.rs - Input validation utilities

use std::error::Error;

/// Validate episode ID format
pub fn validate_episode_id(episode_id_str: &str) -> Result<u64, Box<dyn Error>> {
    episode_id_str.parse()
        .map_err(|_| "Invalid episode ID".into())
}

/// Validate timeout seconds
pub fn validate_timeout(timeout_str: &str) -> Result<u64, Box<dyn Error>> {
    timeout_str.parse()
        .map_err(|_| "Invalid timeout value".into())
}

/// Validate port number
pub fn validate_port(port_str: &str) -> Result<u16, Box<dyn Error>> {
    port_str.parse()
        .map_err(|_| "Invalid port number".into())
}

/// Validate comment text
pub fn validate_comment_text(text: &str) -> Result<(), Box<dyn Error>> {
    if text.trim().is_empty() {
        return Err("Comment cannot be empty".into());
    }
    
    if text.len() > 2000 {
        return Err("Comment too long (max 2000 characters)".into());
    }
    
    Ok(())
}