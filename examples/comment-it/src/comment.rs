use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize, PartialEq)]
pub struct Comment {
    pub author: String,
    pub content: String,
    pub timestamp: u64,
}
