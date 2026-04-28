//! Session Management
//!
//! Handles user sessions for authenticated users

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Session type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionType {
    /// Basic authentication session
    Basic,
    /// Token-based session
    Token,
    /// Cookie-based session
    Cookie,
}

/// User session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    /// Unique session ID
    pub id: String,
    /// Username
    pub username: String,
    /// Client IP address
    pub client_ip: IpAddr,
    /// Session type
    pub session_type: SessionType,
    /// Session token (for token/cookie auth)
    pub token: Option<String>,
    /// Creation time
    pub created_at: DateTime<Utc>,
    /// Last activity time
    pub last_activity: DateTime<Utc>,
    /// Expiration time
    pub expires_at: DateTime<Utc>,
    /// Whether user can only configure self
    pub configure_self_only: bool,
}

impl UserSession {
    /// Create a new session
    pub fn new(
        username: String,
        client_ip: IpAddr,
        session_type: SessionType,
        timeout_seconds: i64,
    ) -> Self {
        let now = Utc::now();
        let id = Uuid::new_v4().to_string();
        let token = if session_type != SessionType::Basic {
            Some(Uuid::new_v4().to_string())
        } else {
            None
        };

        Self {
            id,
            username,
            client_ip,
            session_type,
            token,
            created_at: now,
            last_activity: now,
            expires_at: now + Duration::seconds(timeout_seconds),
            configure_self_only: false,
        }
    }

    /// Check if session is expired
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Update last activity time and extend expiration
    pub fn touch(&mut self, timeout_seconds: i64) {
        let now = Utc::now();
        self.last_activity = now;
        self.expires_at = now + Duration::seconds(timeout_seconds);
    }
}

/// Session store for managing active sessions
#[derive(Debug, Clone)]
pub struct SessionStore {
    sessions: Arc<RwLock<HashMap<String, UserSession>>>,
    timeout_seconds: i64,
    max_sessions: usize,
}

impl SessionStore {
    /// Create a new session store
    pub fn new(timeout_seconds: u64, max_sessions: usize) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            timeout_seconds: timeout_seconds as i64,
            max_sessions,
        }
    }

    /// Generate a new user session
    pub fn create_session(
        &self,
        username: String,
        client_ip: IpAddr,
        session_type: SessionType,
    ) -> Option<UserSession> {
        let mut sessions = self.sessions.write().unwrap();

        // Clean up expired sessions
        sessions.retain(|_, session| !session.is_expired());

        // Check max sessions limit
        if sessions.len() >= self.max_sessions {
            return None;
        }

        // For Basic auth, check if session already exists for this user/IP
        if session_type == SessionType::Basic {
            for session in sessions.values() {
                if session.session_type == SessionType::Basic
                    && session.username == username
                    && session.client_ip == client_ip
                {
                    return Some(session.clone());
                }
            }
        }

        // Create new session
        let session = UserSession::new(username, client_ip, session_type, self.timeout_seconds);
        let session_id = session.id.clone();
        sessions.insert(session_id, session.clone());

        Some(session)
    }

    /// Get session by ID
    pub fn get_session(&self, session_id: &str) -> Option<UserSession> {
        let mut sessions = self.sessions.write().unwrap();
        
        if let Some(session) = sessions.get_mut(session_id) {
            if session.is_expired() {
                sessions.remove(session_id);
                return None;
            }
            session.touch(self.timeout_seconds);
            return Some(session.clone());
        }
        None
    }

    /// Get session by token
    pub fn get_session_by_token(&self, token: &str) -> Option<UserSession> {
        let mut sessions = self.sessions.write().unwrap();
        
        // Find session with matching token
        let session_id = sessions
            .iter()
            .find(|(_, s)| s.token.as_deref() == Some(token))
            .map(|(id, _)| id.clone());

        if let Some(id) = session_id {
            if let Some(session) = sessions.get_mut(&id) {
                if session.is_expired() {
                    sessions.remove(&id);
                    return None;
                }
                session.touch(self.timeout_seconds);
                return Some(session.clone());
            }
        }
        None
    }

    /// Delete a session
    pub fn delete_session(&self, session_id: &str) -> bool {
        let mut sessions = self.sessions.write().unwrap();
        sessions.remove(session_id).is_some()
    }

    /// Get all active sessions
    pub fn get_all_sessions(&self) -> Vec<UserSession> {
        let mut sessions = self.sessions.write().unwrap();
        
        // Clean up expired sessions
        sessions.retain(|_, session| !session.is_expired());
        
        sessions.values().cloned().collect()
    }

    /// Get sessions for a specific user
    pub fn get_user_sessions(&self, username: &str) -> Vec<UserSession> {
        let sessions = self.sessions.read().unwrap();
        sessions
            .values()
            .filter(|s| s.username == username && !s.is_expired())
            .cloned()
            .collect()
    }

    /// Delete all sessions for a user
    pub fn delete_user_sessions(&self, username: &str) -> usize {
        let mut sessions = self.sessions.write().unwrap();
        let to_remove: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| s.username == username)
            .map(|(id, _)| id.clone())
            .collect();
        
        let count = to_remove.len();
        for id in to_remove {
            sessions.remove(&id);
        }
        count
    }

    /// Clean up expired sessions
    pub fn cleanup_expired(&self) -> usize {
        let mut sessions = self.sessions.write().unwrap();
        let before = sessions.len();
        sessions.retain(|_, session| !session.is_expired());
        before - sessions.len()
    }

    /// Get session count
    pub fn session_count(&self) -> usize {
        let sessions = self.sessions.read().unwrap();
        sessions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_session_creation() {
        let session = UserSession::new(
            "testuser".to_string(),
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            SessionType::Token,
            3600,
        );

        assert_eq!(session.username, "testuser");
        assert_eq!(session.session_type, SessionType::Token);
        assert!(session.token.is_some());
        assert!(!session.is_expired());
    }

    #[test]
    fn test_session_store() {
        let store = SessionStore::new(3600, 10);
        
        let session = store.create_session(
            "testuser".to_string(),
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            SessionType::Token,
        );

        assert!(session.is_some());
        let session = session.unwrap();
        
        // Retrieve by ID
        let retrieved = store.get_session(&session.id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().username, "testuser");

        // Retrieve by token
        if let Some(token) = &session.token {
            let retrieved = store.get_session_by_token(token);
            assert!(retrieved.is_some());
        }
    }

    #[test]
    fn test_session_deletion() {
        let store = SessionStore::new(3600, 10);
        
        let session = store.create_session(
            "testuser".to_string(),
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            SessionType::Token,
        ).unwrap();

        assert_eq!(store.session_count(), 1);
        
        store.delete_session(&session.id);
        assert_eq!(store.session_count(), 0);
    }

    #[test]
    fn test_basic_auth_session_reuse() {
        let store = SessionStore::new(3600, 10);
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        
        // Create first session
        let session1 = store.create_session(
            "testuser".to_string(),
            ip,
            SessionType::Basic,
        ).unwrap();

        // Try to create another session for same user/IP
        let session2 = store.create_session(
            "testuser".to_string(),
            ip,
            SessionType::Basic,
        ).unwrap();

        // Should return the same session
        assert_eq!(session1.id, session2.id);
        assert_eq!(store.session_count(), 1);
    }
}
