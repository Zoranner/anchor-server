//! 管理员密码、会话 Cookie 和 CSRF 校验。

use std::sync::OnceLock;

use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
};
use axum::http::{header, HeaderMap, HeaderValue};
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, Set,
};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use thiserror::Error;
use uuid::Uuid;

use crate::entity::{admin_session, admin_user};

pub const ADMIN_SESSION_COOKIE: &str = "anchor_admin_session";
pub const ADMIN_CSRF_COOKIE: &str = "anchor_admin_csrf";
pub const ADMIN_SESSION_TTL_HOURS: i64 = 12;
const SESSION_TOUCH_INTERVAL_MINUTES: i64 = 5;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("missing admin session")]
    MissingSession,
    #[error("invalid admin session")]
    InvalidSession,
    #[error("admin session expired")]
    ExpiredSession,
    #[error("admin account disabled")]
    DisabledAdmin,
    #[error("missing CSRF token")]
    MissingCsrf,
    #[error("invalid CSRF token")]
    InvalidCsrf,
    #[error(transparent)]
    Database(#[from] DbErr),
}

#[derive(Debug, Clone)]
pub struct AuthContext {
    pub admin: admin_user::Model,
    pub session: admin_session::Model,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedSession {
    pub token: String,
    pub csrf_token: String,
    pub expires_at: DateTime<Utc>,
}

pub fn validate_username(username: &str) -> Result<(), &'static str> {
    if !(3..=64).contains(&username.len()) {
        return Err("username must contain 3 to 64 characters");
    }
    if !username
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err("username may only contain ASCII letters, digits, dot, underscore and hyphen");
    }
    let first = username.as_bytes()[0];
    let last = username.as_bytes()[username.len() - 1];
    if !first.is_ascii_alphanumeric() || !last.is_ascii_alphanumeric() {
        return Err("username must start and end with an ASCII letter or digit");
    }
    Ok(())
}

pub fn validate_password(password: &str) -> Result<(), &'static str> {
    if !(12..=1024).contains(&password.len()) {
        return Err("password must contain 12 to 1024 bytes");
    }
    if password.trim().is_empty() {
        return Err("password cannot be blank");
    }
    Ok(())
}

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| anyhow::anyhow!("hash admin password: {error}"))
}

pub fn verify_password(password_hash: &str, password: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(password_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// 用户不存在时也执行一次 Argon2 校验，避免明显的用户名枚举时间差。
pub fn verify_dummy_password(password: &str) {
    static DUMMY_HASH: OnceLock<String> = OnceLock::new();
    let hash = DUMMY_HASH.get_or_init(|| {
        hash_password("anchor-guard-dummy-password").expect("dummy Argon2 hash must be valid")
    });
    let _ = verify_password(hash, password);
}

pub fn generate_secret() -> String {
    let mut bytes = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub fn secret_hash(secret: &str) -> Vec<u8> {
    Sha256::digest(secret.as_bytes()).to_vec()
}

pub fn secrets_equal(left: &str, right: &str) -> bool {
    let left = Sha256::digest(left.as_bytes());
    let right = Sha256::digest(right.as_bytes());
    left.ct_eq(&right).into()
}

pub async fn create_session(
    db: &DatabaseConnection,
    admin_user_id: Uuid,
) -> Result<IssuedSession, DbErr> {
    let token = generate_secret();
    let csrf_token = generate_secret();
    let now = Utc::now();
    let expires_at = now + Duration::hours(ADMIN_SESSION_TTL_HOURS);
    admin_session::ActiveModel {
        id: Set(Uuid::new_v4()),
        admin_user_id: Set(admin_user_id),
        session_hash: Set(secret_hash(&token)),
        csrf_hash: Set(secret_hash(&csrf_token)),
        created_at: Set(now),
        expires_at: Set(expires_at),
        last_seen_at: Set(now),
        revoked_at: Set(None),
    }
    .insert(db)
    .await?;

    Ok(IssuedSession {
        token,
        csrf_token,
        expires_at,
    })
}

pub async fn authenticate(
    db: &DatabaseConnection,
    headers: &HeaderMap,
) -> Result<AuthContext, AuthError> {
    let token = cookie_value(headers, ADMIN_SESSION_COOKIE).ok_or(AuthError::MissingSession)?;
    if token.is_empty() || token.len() > 512 {
        return Err(AuthError::InvalidSession);
    }

    let mut session = admin_session::Entity::find()
        .filter(admin_session::Column::SessionHash.eq(secret_hash(&token)))
        .one(db)
        .await?
        .ok_or(AuthError::InvalidSession)?;
    let now = Utc::now();
    if session.revoked_at.is_some() {
        return Err(AuthError::InvalidSession);
    }
    if now >= session.expires_at {
        return Err(AuthError::ExpiredSession);
    }

    let admin = admin_user::Entity::find_by_id(session.admin_user_id)
        .one(db)
        .await?
        .ok_or(AuthError::InvalidSession)?;
    if admin.disabled_at.is_some() {
        return Err(AuthError::DisabledAdmin);
    }

    if now - session.last_seen_at >= Duration::minutes(SESSION_TOUCH_INTERVAL_MINUTES) {
        let mut active: admin_session::ActiveModel = session.clone().into();
        active.last_seen_at = Set(now);
        session = active.update(db).await?;
    }

    Ok(AuthContext { admin, session })
}

pub fn validate_csrf(headers: &HeaderMap, session: &admin_session::Model) -> Result<(), AuthError> {
    let header_token = header_value(headers, "x-csrf-token").ok_or(AuthError::MissingCsrf)?;
    let cookie_token = cookie_value(headers, ADMIN_CSRF_COOKIE).ok_or(AuthError::MissingCsrf)?;
    if !secrets_equal(&header_token, &cookie_token) {
        return Err(AuthError::InvalidCsrf);
    }
    let provided = secret_hash(&header_token);
    if !bool::from(provided.ct_eq(&session.csrf_hash)) {
        return Err(AuthError::InvalidCsrf);
    }
    Ok(())
}

pub async fn revoke_session(
    db: &DatabaseConnection,
    session: &admin_session::Model,
) -> Result<(), DbErr> {
    let mut active: admin_session::ActiveModel = session.clone().into();
    active.revoked_at = Set(Some(Utc::now()));
    active.update(db).await?;
    Ok(())
}

pub fn session_cookie(token: &str) -> HeaderValue {
    cookie_header(
        ADMIN_SESSION_COOKIE,
        token,
        true,
        ADMIN_SESSION_TTL_HOURS * 60 * 60,
    )
}

pub fn csrf_cookie(token: &str) -> HeaderValue {
    cookie_header(
        ADMIN_CSRF_COOKIE,
        token,
        false,
        ADMIN_SESSION_TTL_HOURS * 60 * 60,
    )
}

pub fn clear_cookies() -> [HeaderValue; 2] {
    [
        cookie_header(ADMIN_SESSION_COOKIE, "", true, 0),
        cookie_header(ADMIN_CSRF_COOKIE, "", false, 0),
    ]
}

fn cookie_header(name: &str, value: &str, http_only: bool, max_age: i64) -> HeaderValue {
    let http_only = if http_only { "; HttpOnly" } else { "" };
    HeaderValue::from_str(&format!(
        "{name}={value}; Path=/; Secure; SameSite=Strict; Max-Age={max_age}{http_only}"
    ))
    .expect("cookie header contains only static ASCII and a hex token")
}

pub fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find_map(|(key, value)| (key.trim() == name).then(|| value.trim().to_owned()))
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn username_and_password_policy_rejects_invalid_values() {
        assert!(validate_username("admin").is_ok());
        assert!(validate_username("ops.admin-1").is_ok());
        assert!(validate_username("-admin").is_err());
        assert!(validate_username("a b").is_err());
        assert!(validate_password("correct-horse-battery").is_ok());
        assert!(validate_password("short").is_err());
        assert!(validate_password("            ").is_err());
    }

    #[test]
    fn password_hash_verifies_only_the_original_password() {
        let hash = hash_password("correct-horse-battery").unwrap();
        assert!(verify_password(&hash, "correct-horse-battery"));
        assert!(!verify_password(&hash, "wrong-password"));
        assert!(!verify_password("not-a-phc-hash", "correct-horse-battery"));
    }

    #[test]
    fn generated_secrets_are_256_bit_hex() {
        let first = generate_secret();
        let second = generate_secret();
        assert_eq!(first.len(), 64);
        assert_eq!(second.len(), 64);
        assert_ne!(first, second);
    }

    #[test]
    fn cookies_use_secure_strict_attributes() {
        let session = session_cookie("session-token");
        let session = session.to_str().unwrap();
        assert!(session.contains("Path=/"));
        assert!(session.contains("Secure"));
        assert!(session.contains("SameSite=Strict"));
        assert!(session.contains("HttpOnly"));

        let csrf = csrf_cookie("csrf-token");
        let csrf = csrf.to_str().unwrap();
        assert!(!csrf.contains("HttpOnly"));
    }

    #[test]
    fn cookie_parser_reads_multiple_cookie_headers() {
        let mut headers = HeaderMap::new();
        headers.append(
            header::COOKIE,
            HeaderValue::from_static("first=one; anchor_admin_session=abc"),
        );
        headers.append(
            header::COOKIE,
            HeaderValue::from_static("anchor_admin_csrf=def; last=two"),
        );
        assert_eq!(
            cookie_value(&headers, ADMIN_SESSION_COOKIE).as_deref(),
            Some("abc")
        );
        assert_eq!(
            cookie_value(&headers, ADMIN_CSRF_COOKIE).as_deref(),
            Some("def")
        );
    }

    #[test]
    fn csrf_requires_matching_header_cookie_and_session_hash() {
        let csrf = "csrf-token";
        let session = admin_session::Model {
            id: Uuid::new_v4(),
            admin_user_id: Uuid::new_v4(),
            session_hash: secret_hash("session-token"),
            csrf_hash: secret_hash(csrf),
            created_at: Utc.with_ymd_and_hms(2026, 10, 29, 8, 0, 0).unwrap(),
            expires_at: Utc.with_ymd_and_hms(2026, 10, 29, 20, 0, 0).unwrap(),
            last_seen_at: Utc.with_ymd_and_hms(2026, 10, 29, 8, 0, 0).unwrap(),
            revoked_at: None,
        };
        let mut headers = HeaderMap::new();
        headers.insert("x-csrf-token", HeaderValue::from_static("csrf-token"));
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("anchor_admin_csrf=csrf-token"),
        );
        assert!(validate_csrf(&headers, &session).is_ok());

        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("anchor_admin_csrf=other"),
        );
        assert!(matches!(
            validate_csrf(&headers, &session),
            Err(AuthError::InvalidCsrf)
        ));
    }
}
