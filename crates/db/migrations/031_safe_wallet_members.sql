-- Mapping from on-chain Safe owner addresses to workspace user IDs.
--
-- Before this migration the only user permitted to operate a
-- registered Safe was the `registered_by` admin (plus workspace
-- Owner). That stopgap collapses as soon as a real Safe has multiple
-- internal signers — they would each need the registering admin's
-- session to propose/sign, which is exactly the key-sharing we built
-- this service to avoid.
--
-- With this table, proposing/signing/executing on a Safe is gated
-- on membership: the authenticated user must be linked to at least
-- one of the Safe's on-chain owner addresses. The membership row is
-- also what the sign endpoint checks to ensure `req.signer` (the
-- EVM address producing the EIP-712 signature) actually belongs to
-- the caller — preventing a linked user from claiming to sign on
-- behalf of a co-owner.
--
-- `owner_address` is stored lowercased (see application insert path)
-- so EIP-55 casing mismatches don't cause lookup misses.

CREATE TABLE IF NOT EXISTS safe_wallet_members (
    safe_address   TEXT        NOT NULL,
    chain          TEXT        NOT NULL,
    owner_address  TEXT        NOT NULL,
    user_id        UUID        NOT NULL REFERENCES workspace_users(id),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (safe_address, chain, owner_address)
);

CREATE INDEX IF NOT EXISTS idx_safe_wallet_members_user
    ON safe_wallet_members(user_id);

CREATE INDEX IF NOT EXISTS idx_safe_wallet_members_safe
    ON safe_wallet_members(safe_address, chain);
