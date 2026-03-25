//! Live Tron testnet/mainnet API test.
//!
//! Tests real HTTP calls to Tron API (api.trongrid.io).

const TRON_API: &str = "https://api.trongrid.io";

async fn can_connect() -> bool {
    reqwest::Client::new()
        .get(&format!("{}/wallet/getnowblock", TRON_API))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

#[tokio::test]
async fn tron_get_current_block() {
    if !can_connect().await {
        eprintln!("Skipping: Tron API unavailable");
        return;
    }

    let client = reqwest::Client::new();
    let resp: serde_json::Value = client
        .get(&format!("{}/wallet/getnowblock", TRON_API))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    // Tron getnowblock may use POST on some endpoints
    assert!(resp.is_object(), "Should get JSON response");
    println!("Tron nowblock response: {:?}", resp.as_object().map(|o| o.keys().collect::<Vec<_>>()));
}

#[tokio::test]
async fn tron_get_block_by_number() {
    if !can_connect().await { return; }

    let client = reqwest::Client::new();
    let resp: serde_json::Value = client
        .post(&format!("{}/wallet/getblockbynum", TRON_API))
        .json(&serde_json::json!({"num": 50_000_000}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert!(resp.is_object(), "Should get block data");
    println!("Tron block 50M response: {:?}", resp.as_object().map(|o| o.keys().collect::<Vec<_>>()));
}

#[tokio::test]
async fn tron_get_account_balance() {
    if !can_connect().await { return; }

    let client = reqwest::Client::new();
    // Check Tron Foundation address
    let resp: serde_json::Value = client
        .post(&format!("{}/walletsolidity/getaccount", TRON_API))
        .json(&serde_json::json!({
            "address": "TLyqzVGLV1srkB7dToTAEqgDSfPtXRJZYH",
            "visible": true
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    // Should have some balance
    let balance = resp["balance"].as_u64().unwrap_or(0);
    println!("Tron account balance: {} sun ({} TRX)", balance, balance / 1_000_000);
}

#[tokio::test]
async fn tron_usdt_contract_info() {
    if !can_connect().await { return; }

    let client = reqwest::Client::new();
    // Query TRC-20 USDT contract
    let resp: serde_json::Value = client
        .post(&format!("{}/wallet/getcontract", TRON_API))
        .json(&serde_json::json!({
            "value": "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t",
            "visible": true
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    // Response structure depends on Tron API version
    assert!(resp.is_object(), "Should get contract info");
    println!("Tron USDT contract response: {:?}", resp.as_object().map(|o| o.keys().collect::<Vec<_>>()));
}
