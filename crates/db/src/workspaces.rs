//! Workspace and user persistence.

use chrono::{DateTime, Utc};
use custody_common::platform::{
    UserRole, WorkspaceDetail, WorkspaceEnv, WorkspaceUser,
};
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    pub async fn insert_workspace(&self, w: &WorkspaceDetail) -> anyhow::Result<()> {
        let env = serde_json::to_string(&w.environment)?;
        sqlx::query(
            "INSERT INTO workspaces (id, name, environment, owner_email, enabled, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (name) DO NOTHING",
        )
        .bind(w.id)
        .bind(&w.name)
        .bind(env.trim_matches('"'))
        .bind(&w.owner_email)
        .bind(w.enabled)
        .bind(w.created_at)
        .bind(w.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn find_workspace(&self, id: Uuid) -> anyhow::Result<Option<WorkspaceDetail>> {
        let row: Option<WorkspaceRow> = sqlx::query_as(
            "SELECT id, name, environment, owner_email, enabled, created_at, updated_at
             FROM workspaces WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.into_workspace()))
    }

    pub async fn list_workspaces(&self) -> anyhow::Result<Vec<WorkspaceDetail>> {
        let rows: Vec<WorkspaceRow> = sqlx::query_as(
            "SELECT id, name, environment, owner_email, enabled, created_at, updated_at
             FROM workspaces ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.into_workspace()).collect())
    }

    pub async fn update_workspace(
        &self,
        id: Uuid,
        name: Option<&str>,
        enabled: Option<bool>,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE workspaces SET
                name = COALESCE($2, name),
                enabled = COALESCE($3, enabled),
                updated_at = NOW()
             WHERE id = $1",
        )
        .bind(id)
        .bind(name)
        .bind(enabled)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    // Users
    pub async fn insert_workspace_user(&self, u: &WorkspaceUser) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO workspace_users (id, workspace_id, email, role, enabled, created_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (workspace_id, email) DO NOTHING",
        )
        .bind(u.id)
        .bind(u.workspace_id)
        .bind(&u.email)
        .bind(u.role.to_string())
        .bind(u.enabled)
        .bind(u.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_workspace_users(&self, workspace_id: Uuid) -> anyhow::Result<Vec<WorkspaceUser>> {
        let rows: Vec<WorkspaceUserRow> = sqlx::query_as(
            "SELECT id, workspace_id, email, role, enabled, created_at, last_login_at
             FROM workspace_users WHERE workspace_id = $1 ORDER BY created_at",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.into_user()).collect())
    }

    pub async fn delete_workspace_user(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM workspace_users WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[derive(sqlx::FromRow)]
struct WorkspaceRow {
    id: Uuid,
    name: String,
    environment: String,
    owner_email: String,
    enabled: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl WorkspaceRow {
    fn into_workspace(self) -> WorkspaceDetail {
        let environment = match self.environment.as_str() {
            "mainnet" => WorkspaceEnv::Mainnet,
            "testnet" => WorkspaceEnv::Testnet,
            _ => WorkspaceEnv::Sandbox,
        };
        WorkspaceDetail {
            id: self.id, name: self.name, environment,
            owner_email: self.owner_email,
            vault_count: 0, api_key_count: 0,
            enabled: self.enabled,
            created_at: self.created_at, updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct WorkspaceUserRow {
    id: Uuid,
    workspace_id: Uuid,
    email: String,
    role: String,
    enabled: bool,
    created_at: DateTime<Utc>,
    last_login_at: Option<DateTime<Utc>>,
}

impl WorkspaceUserRow {
    fn into_user(self) -> WorkspaceUser {
        let role = match self.role.as_str() {
            "owner" => UserRole::Owner,
            "admin" => UserRole::Admin,
            "signer" => UserRole::Signer,
            _ => UserRole::Viewer,
        };
        WorkspaceUser {
            id: self.id, workspace_id: self.workspace_id,
            email: self.email, role, enabled: self.enabled,
            created_at: self.created_at, last_login_at: self.last_login_at,
        }
    }
}
