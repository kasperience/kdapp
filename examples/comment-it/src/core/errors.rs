use borsh::{BorshDeserialize, BorshSerialize};

/// Authentication and comment-specific errors
#[derive(Debug, BorshDeserialize, BorshSerialize)]
pub enum AuthError {
    // Auth errors
    ChallengeNotFound,
    InvalidChallenge,
    SignatureVerificationFailed,
    AlreadyAuthenticated,
    NotAuthorized,
    RateLimited,
    InvalidSignature,
    ChallengeExpired,
    SessionNotFound,
    InvalidSessionToken,
    SessionAlreadyRevoked,
    
    // Comment errors
    CommentTooLong,
    CommentEmpty,
    NotAuthenticated,
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::ChallengeNotFound => write!(f, "Challenge not found for this participant."),
            AuthError::InvalidChallenge => write!(f, "Invalid or expired challenge."),
            AuthError::SignatureVerificationFailed => write!(f, "Signature verification failed."),
            AuthError::AlreadyAuthenticated => write!(f, "Participant is already authenticated."),
            AuthError::NotAuthorized => write!(f, "Participant is not authorized."),
            AuthError::RateLimited => write!(f, "Rate limit exceeded. Please try again later."),
            AuthError::InvalidSignature => write!(f, "Invalid signature format."),
            AuthError::ChallengeExpired => write!(f, "Challenge has expired."),
            AuthError::SessionNotFound => write!(f, "Session not found or not authenticated."),
            AuthError::InvalidSessionToken => write!(f, "Invalid or malformed session token."),
            AuthError::SessionAlreadyRevoked => write!(f, "Session has already been revoked."),
            AuthError::CommentTooLong => write!(f, "Comment is too long (max 2000 characters)."),
            AuthError::CommentEmpty => write!(f, "Comment cannot be empty."),
            AuthError::NotAuthenticated => write!(f, "User is not authenticated."),
        }
    }
}

impl std::error::Error for AuthError {}

/// Authentication result type
pub type AuthResult<T> = Result<T, AuthError>;