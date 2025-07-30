// src/daemon/service.rs - FIXED to use working endpoint pattern
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use reqwest;
use serde_json;

use crate::daemon::{DaemonConfig, protocol::*};
use crate::wallet::KaspaAuthWallet;

/// Fixed daemon authentication implementation
impl AuthDaemon {
    /// Run authentication using WORKING web UI endpoint pattern (3 transactions)
    async fn run_working_authentication_flow(
        &self, 
        wallet: &KaspaAuthWallet, 
        server_url: &str
    ) -> Result<crate::auth::authentication::AuthenticationResult, Box<dyn std::error::Error>> {
        let client = reqwest::Client::new();
        let public_key_hex = wallet.get_public_key_hex();
        
        println!("🔑 Using wallet public key: {}", public_key_hex);
        println!("🎯 Following EXACT Web UI pattern (3 transactions)");
        
        // Step 1: Create episode (HTTP coordination only)
        println!("📝 Step 1: Creating episode via /auth/start...");
        let start_response = client
            .post(&format!("{}/auth/start", server_url))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "public_key": public_key_hex
            }))
            .send()
            .await?;
        
        if !start_response.status().is_success() {
            let status = start_response.status();
            let body = start_response.text().await?;
            return Err(format!("Failed to start auth: HTTP {} - {}", status, body).into());
        }
        
        let start_data: serde_json::Value = start_response.json().await?;
        let episode_id = start_data["episode_id"].as_u64()
            .ok_or("Server did not return valid episode_id")?;
        
        println!("✅ Episode {} created (HTTP coordination)", episode_id);
        
        // Step 2: Request challenge (HTTP coordination only) 
        println!("📨 Step 2: Requesting challenge via /auth/request-challenge...");
        let challenge_response = client
            .post(&format!("{}/auth/request-challenge", server_url))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "episode_id": episode_id,
                "public_key": public_key_hex
            }))
            .send()
            .await?;
        
        if !challenge_response.status().is_success() {
            let status = challenge_response.status();
            let body = challenge_response.text().await?;
            return Err(format!("Failed to request challenge: HTTP {} - {}", status, body).into());
        }
        
        // Get challenge immediately from response (no polling needed with fixed endpoints)
        let challenge_data: serde_json::Value = challenge_response.json().await?;
        let challenge = challenge_data["nonce"].as_str()
            .ok_or("Server did not return challenge")?
            .to_string();
        
        println!("🎯 Challenge received immediately: {}", challenge);
        
        // Step 3: Sign challenge locally
        println!("✍️ Step 3: Signing challenge locally...");
        let msg = kdapp::pki::to_message(&challenge);
        let signature = kdapp::pki::sign_message(&wallet.keypair.secret_key(), &msg);
        let signature_hex = hex::encode(signature.0.serialize_der());
        
        println!("✅ Challenge signed with wallet private key");
        
        // Step 4: Submit verification (this triggers ALL 3 blockchain transactions)
        println!("📤 Step 4: Submitting verification via /auth/verify...");
        println!("⚡ This will trigger all 3 blockchain transactions:");
        println!("   1. NewEpisode");
        println!("   2. RequestChallenge");
        println!("   3. SubmitResponse");
        
        let verify_response = client
            .post(&format!("{}/auth/verify", server_url))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "episode_id": episode_id,
                "signature": signature_hex,
                "nonce": challenge
            }))
            .send()
            .await?;
        
        if !verify_response.status().is_success() {
            let status = verify_response.status();
            let body = verify_response.text().await?;
            return Err(format!("Failed to verify: HTTP {} - {}", status, body).into());
        }
        
        let verify_data: serde_json::Value = verify_response.json().await?;
        let transaction_id = verify_data["transaction_id"].as_str();
        
        println!("✅ All transactions submitted!");
        if let Some(tx_id) = transaction_id {
            println!("📋 Final transaction ID: {}", tx_id);
        }
        
        // Step 5: Poll for authentication completion
        println!("⏳ Step 5: Waiting for blockchain confirmation...");
        let mut session_token = String::new();
        let max_attempts = 60; // 30 seconds timeout
        
        for attempt in 1..=max_attempts {
            let status_response = client
                .get(&format!("{}/auth/status/{}", server_url, episode_id))
                .send()
                .await?;
            
            if status_response.status().is_success() {
                let status_data: serde_json::Value = status_response.json().await?;
                
                // Debug logging
                if attempt == 1 {
                    println!("📊 Episode status: {}", serde_json::to_string_pretty(&status_data)?);
                }
                
                if let (Some(authenticated), Some(token)) = (
                    status_data["authenticated"].as_bool(),
                    status_data["session_token"].as_str()
                ) {
                    if authenticated && !token.is_empty() {
                        session_token = token.to_string();
                        println!("✅ Authentication confirmed by blockchain!");
                        println!("🎫 Session token: {}", session_token);
                        break;
                    }
                }
            }
            
            if attempt % 10 == 0 {
                println!("⏳ Still waiting... ({}/{})", attempt, max_attempts);
            }
            
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
        
        if session_token.is_empty() {
            return Err("❌ Timeout waiting for blockchain confirmation".into());
        }
        
        Ok(crate::auth::authentication::AuthenticationResult {
            episode_id,
            session_token,
            authenticated: true,
        })
    }
    
    /// Perform full authentication flow (public interface)
    async fn authenticate(&self, username: &str, server_url: &str) -> DaemonResponse {
        println!("🔐 Authenticating {} with {}", username, server_url);
        
        // Check if identity is unlocked
        let wallet = {
            let identities = self.unlocked_identities.lock().unwrap();
            match identities.get(username) {
                Some(wallet) => wallet.clone(),
                None => {
                    return DaemonResponse::Error {
                        error: format!("Identity '{}' not unlocked", username),
                    };
                }
            }
        };
        
        // Broadcast event
        let _ = self.event_tx.send(DaemonEvent::AuthenticationStarted {
            username: username.to_string(),
            server_url: server_url.to_string(),
        });
        
        // Use the FIXED authentication flow
        println!("🌐 Using FIXED endpoint pattern (3 blockchain transactions)");
        
        match self.run_working_authentication_flow(&wallet, server_url).await {
            Ok(auth_result) => {
                println!("✅ BLOCKCHAIN AUTHENTICATION SUCCESS!");
                println!("📧 Episode ID: {}", auth_result.episode_id);
                println!("🎫 Session Token: {}", auth_result.session_token);
                println!("🔗 All 3 transactions confirmed on blockchain");
                
                // Create active session
                let session = ActiveSession {
                    username: username.to_string(),
                    server_url: server_url.to_string(),
                    episode_id: auth_result.episode_id,
                    session_token: auth_result.session_token.clone(),
                    created_at: Instant::now(),
                };
                
                // Store session
                {
                    let mut sessions = self.active_sessions.lock().unwrap();
                    sessions.insert(auth_result.episode_id, session);
                }
                
                // Broadcast success event
                let _ = self.event_tx.send(DaemonEvent::AuthenticationCompleted {
                    username: username.to_string(),
                    success: true,
                });
                
                DaemonResponse::AuthResult {
                    success: true,
                    episode_id: Some(auth_result.episode_id),
                    session_token: Some(auth_result.session_token),
                    message: format!("Authentication successful - 3 transactions confirmed"),
                }
            }
            Err(e) => {
                println!("❌ AUTHENTICATION FAILED: {}", e);
                
                // Broadcast failure event
                let _ = self.event_tx.send(DaemonEvent::AuthenticationCompleted {
                    username: username.to_string(),
                    success: false,
                });
                
                DaemonResponse::Error {
                    error: format!("Authentication failed: {}", e),
                }
            }
        }
    }
}