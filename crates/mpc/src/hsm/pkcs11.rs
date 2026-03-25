//! PKCS#11 HSM adapter.
//!
//! Interfaces with hardware security modules via the PKCS#11 standard.
//! For development: use SoftHSM2.
//! For production: Thales Luna, Utimaco, AWS CloudHSM, Azure Dedicated HSM.

use anyhow::Result;

use super::HsmAdapter;

/// PKCS#11 HSM adapter configuration.
#[derive(Debug, Clone)]
pub struct Pkcs11Config {
    /// Path to the PKCS#11 library (.so/.dylib).
    pub library_path: String,
    /// HSM slot ID.
    pub slot_id: u64,
    /// HSM user PIN.
    pub pin: String,
}

impl Pkcs11Config {
    /// Default config for SoftHSM2 on Linux.
    pub fn softhsm2() -> Self {
        Self {
            library_path: "/usr/lib/softhsm/libsofthsm2.so".into(),
            slot_id: 0,
            pin: "0000".into(),
        }
    }
}

/// PKCS#11 HSM adapter.
///
/// Wraps a PKCS#11 session for key share storage and retrieval.
/// In this implementation, key shares are stored as PKCS#11 data objects.
pub struct Pkcs11Hsm {
    config: Pkcs11Config,
    // TODO: pkcs11 session handle
}

impl Pkcs11Hsm {
    /// Connect to the HSM.
    pub fn connect(config: Pkcs11Config) -> Result<Self> {
        // TODO: Load PKCS#11 library, open session, login
        tracing::info!(
            library = %config.library_path,
            slot = config.slot_id,
            "Connecting to PKCS#11 HSM (stub)"
        );
        Ok(Self { config })
    }
}

impl HsmAdapter for Pkcs11Hsm {
    fn store_share(&self, key_id: &str, party_id: u16, encrypted_share: &[u8]) -> Result<()> {
        // TODO: C_CreateObject with CKA_CLASS=CKO_DATA, CKA_LABEL=key_id:party_id
        tracing::info!(key_id, party_id, size = encrypted_share.len(), "PKCS#11 store (stub)");
        Ok(())
    }

    fn load_share(&self, _key_id: &str, _party_id: u16) -> Result<Vec<u8>> {
        // TODO: C_FindObjects + C_GetAttributeValue
        anyhow::bail!("PKCS#11 load not implemented — connect to real HSM")
    }

    fn delete_share(&self, key_id: &str, party_id: u16) -> Result<()> {
        // TODO: C_DestroyObject
        tracing::info!(key_id, party_id, "PKCS#11 delete (stub)");
        Ok(())
    }

    fn list_keys(&self) -> Result<Vec<String>> {
        // TODO: C_FindObjects with CKA_CLASS=CKO_DATA
        Ok(vec![])
    }
}

/// AWS CloudHSM adapter (uses PKCS#11 under the hood).
pub struct CloudHsmAdapter {
    inner: Pkcs11Hsm,
    cluster_id: String,
}

impl CloudHsmAdapter {
    pub fn new(cluster_id: &str, config: Pkcs11Config) -> Result<Self> {
        let inner = Pkcs11Hsm::connect(config)?;
        Ok(Self { inner, cluster_id: cluster_id.into() })
    }
}

impl HsmAdapter for CloudHsmAdapter {
    fn store_share(&self, key_id: &str, party_id: u16, data: &[u8]) -> Result<()> {
        self.inner.store_share(key_id, party_id, data)
    }
    fn load_share(&self, key_id: &str, party_id: u16) -> Result<Vec<u8>> {
        self.inner.load_share(key_id, party_id)
    }
    fn delete_share(&self, key_id: &str, party_id: u16) -> Result<()> {
        self.inner.delete_share(key_id, party_id)
    }
    fn list_keys(&self) -> Result<Vec<String>> {
        self.inner.list_keys()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn softhsm2_config() {
        let config = Pkcs11Config::softhsm2();
        assert!(config.library_path.contains("softhsm"));
        assert_eq!(config.slot_id, 0);
    }

    #[test]
    fn pkcs11_connect_stub() {
        let config = Pkcs11Config::softhsm2();
        // This won't actually connect without SoftHSM2 installed
        let hsm = Pkcs11Hsm::connect(config);
        assert!(hsm.is_ok());
    }
}
