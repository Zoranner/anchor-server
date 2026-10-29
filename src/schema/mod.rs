use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr};

/// 初始化服务端 schema；所有语句幂等，按表依赖顺序执行。
pub async fn initialize(db: &DatabaseConnection) -> Result<(), DbErr> {
    for statement in STATEMENTS {
        db.execute_unprepared(statement).await?;
    }
    Ok(())
}

const STATEMENTS: &[&str] = &[
    r#"
    CREATE TABLE IF NOT EXISTS admin_users (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
        username TEXT NOT NULL UNIQUE,
        password_hash TEXT NOT NULL,
        role TEXT NOT NULL DEFAULT 'admin' CHECK (role IN ('admin')),
        created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
        disabled_at TIMESTAMPTZ
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS admin_sessions (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
        admin_user_id UUID NOT NULL REFERENCES admin_users(id) ON DELETE CASCADE,
        session_hash BYTEA NOT NULL UNIQUE,
        created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
        expires_at TIMESTAMPTZ NOT NULL,
        last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
        revoked_at TIMESTAMPTZ
    )
    "#,
    r#"
    CREATE INDEX IF NOT EXISTS admin_sessions_expires_at_idx
        ON admin_sessions (expires_at)
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS server_settings (
        key TEXT PRIMARY KEY,
        value_json JSONB NOT NULL,
        updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS devices (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
        installation_id UUID NOT NULL UNIQUE,
        hostname TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'pending'
            CHECK (status IN ('pending', 'active', 'rejected', 'disabled')),
        token_hash BYTEA NOT NULL UNIQUE,
        token_issued_at TIMESTAMPTZ NOT NULL DEFAULT now(),
        approved_at TIMESTAMPTZ,
        approved_by_admin_id UUID REFERENCES admin_users(id),
        last_seen_at TIMESTAMPTZ,
        client_version TEXT,
        created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
        disabled_at TIMESTAMPTZ
    )
    "#,
    r#"
    CREATE INDEX IF NOT EXISTS devices_status_idx ON devices (status)
    "#,
    r#"
    CREATE INDEX IF NOT EXISTS devices_last_seen_at_idx ON devices (last_seen_at)
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS device_inventory_snapshots (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
        device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
        captured_at TIMESTAMPTZ NOT NULL,
        inventory_json JSONB NOT NULL,
        inventory_hash BYTEA NOT NULL,
        created_at TIMESTAMPTZ NOT NULL DEFAULT now()
    )
    "#,
    r#"
    CREATE INDEX IF NOT EXISTS device_inventory_snapshots_device_captured_idx
        ON device_inventory_snapshots (device_id, captured_at DESC)
    "#,
    r#"
    CREATE SEQUENCE IF NOT EXISTS policy_version_seq START 1
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS policies (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
        version BIGINT NOT NULL UNIQUE,
        status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'signed')),
        policy_json JSONB NOT NULL,
        signature_hex TEXT,
        server_key_id TEXT,
        expires_at TIMESTAMPTZ NOT NULL,
        created_by_admin_id UUID NOT NULL REFERENCES admin_users(id),
        created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
        signed_at TIMESTAMPTZ,
        updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
        CHECK (
            status <> 'signed'
            OR (signature_hex IS NOT NULL AND server_key_id IS NOT NULL AND signed_at IS NOT NULL)
        )
    )
    "#,
    r#"
    CREATE INDEX IF NOT EXISTS policies_status_idx ON policies (status)
    "#,
    r#"
    CREATE INDEX IF NOT EXISTS policies_expires_at_idx ON policies (expires_at)
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS policy_assignments (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
        policy_id UUID NOT NULL REFERENCES policies(id) ON DELETE RESTRICT,
        device_id UUID REFERENCES devices(id) ON DELETE CASCADE,
        is_default BOOLEAN NOT NULL DEFAULT false,
        assigned_by_admin_id UUID NOT NULL REFERENCES admin_users(id),
        created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
        CHECK ((device_id IS NOT NULL) <> is_default)
    )
    "#,
    r#"
    CREATE UNIQUE INDEX IF NOT EXISTS policy_assignments_device_unique
        ON policy_assignments (device_id) WHERE device_id IS NOT NULL
    "#,
    r#"
    CREATE UNIQUE INDEX IF NOT EXISTS policy_assignments_default_unique
        ON policy_assignments (is_default) WHERE is_default
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS anchor_schema_migrations (
        version TEXT PRIMARY KEY,
        applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
    )
    "#,
    r#"
    INSERT INTO anchor_schema_migrations (version)
    VALUES ('0001')
    ON CONFLICT (version) DO NOTHING
    "#,
];
