-- NFT custody table — tracks ERC-721, ERC-1155, and Solana Metaplex NFTs
-- held in custody vaults.
--
-- Design decisions:
--   - token_id is stored as TEXT because ERC-721/1155 token IDs are uint256
--     (too large for BIGINT).
--   - quantity is BIGINT: 1 for ERC-721/Metaplex, N for ERC-1155.
--   - UNIQUE on (chain, contract_address, token_id, owner_vault_id) prevents
--     duplicate registration of the same token in the same vault.

CREATE TABLE IF NOT EXISTS custodied_nfts (
    id               UUID        PRIMARY KEY,
    chain            TEXT        NOT NULL,
    contract_address TEXT        NOT NULL,
    token_id         TEXT        NOT NULL,
    standard         TEXT        NOT NULL,   -- 'erc721' | 'erc1155' | 'metaplex'
    owner_vault_id   UUID        NOT NULL,
    metadata_uri     TEXT,
    name             TEXT,
    image_url        TEXT,
    quantity         BIGINT      NOT NULL DEFAULT 1,
    acquired_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (chain, contract_address, token_id, owner_vault_id)
);

CREATE INDEX IF NOT EXISTS idx_custodied_nfts_vault
    ON custodied_nfts (owner_vault_id);

CREATE INDEX IF NOT EXISTS idx_custodied_nfts_chain
    ON custodied_nfts (chain);
