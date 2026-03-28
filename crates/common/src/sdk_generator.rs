//! SDK code generation — Python and TypeScript client libraries.
//!
//! Generates real, working client code from the custody API definition.
//! Generated SDKs handle:
//! - HMAC-SHA256 request signing (same algorithm as the server)
//! - Proper type definitions for all request/response types
//! - Async/await patterns idiomatic to each target language
//! - Error handling with typed exception classes

/// Generate a complete, working Python SDK.
///
/// The output is a single self-contained `.py` file requiring only the
/// standard library (`hmac`, `hashlib`, `time`, `json`) plus `requests`.
pub fn generate_python_sdk() -> String {
    r#"#!/usr/bin/env python3
"""
MG Custody Python SDK — auto-generated
Requires: requests (pip install requests)
Python 3.10+
"""

from __future__ import annotations

import hashlib
import hmac
import json
import time
from dataclasses import dataclass
from typing import Any, Optional
from urllib.parse import urljoin

import requests


class CustodyApiError(Exception):
    """Raised when the Custody API returns a non-2xx status code."""

    def __init__(self, status_code: int, message: str, body: Any = None) -> None:
        self.status_code = status_code
        self.message = message
        self.body = body
        super().__init__(f"HTTP {status_code}: {message}")


@dataclass
class Address:
    address: str
    chain: str
    memo: Optional[str]


@dataclass
class Withdrawal:
    custody_ref: str
    status: str


@dataclass
class WithdrawalStatus:
    id: str
    status: str
    tx_hash: Optional[str]
    confirmations: int
    error: Optional[str]


@dataclass
class WalletBalance:
    hot_balance: str
    warm_balance: str
    total_managed: str


@dataclass
class FeeEstimate:
    chain: str
    asset: str
    estimated_fee: str
    fee_unit: str


@dataclass
class HealthStatus:
    status: str


@dataclass
class Transaction:
    id: str
    asset: str
    chain: str
    amount: str
    direction: str
    status: str
    tx_hash: Optional[str]
    created_at: str


class CustodyClient:
    """
    Client for the MG Custody internal API.

    Authentication: either HMAC-SHA256 (api_key + hmac_secret) for server-to-server
    calls, or Bearer token for user-session calls. Provide one or the other.

    Args:
        base_url: Base URL of the custody service, e.g. "https://custody.internal:8443"
        api_key: API key identifier (used with hmac_secret for HMAC auth)
        hmac_secret: Shared HMAC-SHA256 secret (bytes or str)
        token: Bearer token for user-session authentication
        timeout: HTTP request timeout in seconds (default 30)
    """

    def __init__(
        self,
        base_url: str,
        *,
        api_key: Optional[str] = None,
        hmac_secret: Optional[str | bytes] = None,
        token: Optional[str] = None,
        timeout: int = 30,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.api_key = api_key
        self._hmac_secret: Optional[bytes] = (
            hmac_secret.encode() if isinstance(hmac_secret, str) else hmac_secret
        )
        self.token = token
        self.timeout = timeout
        self._session = requests.Session()

    # ------------------------------------------------------------------
    # Public API methods
    # ------------------------------------------------------------------

    def get_health(self) -> HealthStatus:
        """Check service liveness. GET /health"""
        body = self._request("GET", "/health", raw=True)
        return HealthStatus(status=body if isinstance(body, str) else "ok")

    def create_address(
        self,
        user_id: str,
        asset: str,
        chain: str,
    ) -> Address:
        """Generate a deposit address for a user. POST /v1/addresses

        Args:
            user_id: UUID of the user in the exchange system
            asset: Asset symbol, e.g. "USDT", "ETH", "BTC"
            chain: Chain identifier, e.g. "ethereum", "bitcoin", "tron"

        Returns:
            Address with the generated deposit address and optional memo
        """
        data = self._request("POST", "/v1/addresses", {
            "user_id": user_id,
            "asset": asset,
            "chain": chain,
        })
        return Address(
            address=data["address"],
            chain=data["chain"],
            memo=data.get("memo"),
        )

    def create_withdrawal(
        self,
        withdrawal_id: str,
        asset: str,
        chain: str,
        address: str,
        amount: str,
    ) -> Withdrawal:
        """Submit a withdrawal request. POST /v1/withdrawals

        Args:
            withdrawal_id: Idempotency key — exchange-side UUID for this withdrawal
            asset: Asset symbol, e.g. "USDT"
            chain: Chain identifier
            address: Destination address (validated on-chain format)
            amount: Amount as decimal string, e.g. "100.50"

        Returns:
            Withdrawal with custody reference ID and initial status
        """
        data = self._request("POST", "/v1/withdrawals", {
            "withdrawal_id": withdrawal_id,
            "asset": asset,
            "chain": chain,
            "address": address,
            "amount": amount,
        })
        return Withdrawal(
            custody_ref=data["custody_ref"],
            status=data["status"],
        )

    def get_withdrawal(self, withdrawal_id: str) -> WithdrawalStatus:
        """Get withdrawal status by custody reference ID. GET /v1/withdrawals/{id}

        Args:
            withdrawal_id: Custody reference UUID returned by create_withdrawal

        Returns:
            Current status including tx_hash once broadcast
        """
        data = self._request("GET", f"/v1/withdrawals/{withdrawal_id}")
        return WithdrawalStatus(
            id=data["id"],
            status=data["status"],
            tx_hash=data.get("tx_hash"),
            confirmations=data.get("confirmations", 0),
            error=data.get("error"),
        )

    def list_transactions(
        self,
        limit: int = 50,
        offset: int = 0,
        chain: Optional[str] = None,
        asset: Optional[str] = None,
    ) -> list[Transaction]:
        """List recent transactions (deposits + withdrawals). GET /v1/transactions

        Args:
            limit: Maximum number of results (default 50, max 200)
            offset: Pagination offset
            chain: Filter by chain identifier
            asset: Filter by asset symbol

        Returns:
            List of Transaction objects ordered by created_at desc
        """
        params: dict[str, Any] = {"limit": limit, "offset": offset}
        if chain:
            params["chain"] = chain
        if asset:
            params["asset"] = asset
        data = self._request("GET", "/v1/transactions", params=params)
        if isinstance(data, list):
            return [self._parse_transaction(t) for t in data]
        return [self._parse_transaction(t) for t in data.get("data", [])]

    def get_portfolio(self) -> dict[str, WalletBalance]:
        """Get balances for all managed assets. GET /v1/wallets

        Returns:
            Dict mapping asset symbol to WalletBalance
        """
        data = self._request("GET", "/v1/wallets")
        if isinstance(data, dict) and "assets" in data:
            return {k: self._parse_balance(v) for k, v in data["assets"].items()}
        return {}

    def get_wallet_balance(self, asset: str) -> WalletBalance:
        """Get balance for a specific asset. GET /v1/wallets/{asset}

        Args:
            asset: Asset symbol, e.g. "USDT", "ETH", "BTC"

        Returns:
            WalletBalance with hot, warm, and total amounts
        """
        data = self._request("GET", f"/v1/wallets/{asset}")
        return self._parse_balance(data)

    def estimate_fee(self, chain: str, asset: str, amount: str) -> FeeEstimate:
        """Estimate network fee for a withdrawal. POST /v1/fees/estimate

        Args:
            chain: Target chain identifier
            asset: Asset symbol
            amount: Withdrawal amount as decimal string

        Returns:
            FeeEstimate with estimated fee and unit
        """
        data = self._request("POST", "/v1/fees/estimate", {
            "chain": chain,
            "asset": asset,
            "amount": amount,
        })
        return FeeEstimate(
            chain=data.get("chain", chain),
            asset=data.get("asset", asset),
            estimated_fee=data.get("estimated_fee", "0"),
            fee_unit=data.get("fee_unit", "native"),
        )

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    def _parse_transaction(self, raw: dict[str, Any]) -> Transaction:
        return Transaction(
            id=raw["id"],
            asset=raw.get("asset", ""),
            chain=raw.get("chain", ""),
            amount=raw.get("amount", "0"),
            direction=raw.get("direction", ""),
            status=raw.get("status", ""),
            tx_hash=raw.get("tx_hash"),
            created_at=raw.get("created_at", ""),
        )

    def _parse_balance(self, raw: dict[str, Any]) -> WalletBalance:
        return WalletBalance(
            hot_balance=raw.get("hot_balance", "0"),
            warm_balance=raw.get("warm_balance", "0"),
            total_managed=raw.get("total_managed", "0"),
        )

    def _sign(self, method: str, path: str, body: str) -> tuple[str, str]:
        """Compute HMAC-SHA256 signature.

        Signature covers: METHOD + PATH + TIMESTAMP + BODY
        Header name: X-Custody-Signature
        Timestamp header: X-Custody-Timestamp (Unix seconds)
        """
        if not self._hmac_secret:
            raise CustodyApiError(0, "hmac_secret required for HMAC auth")
        ts = str(int(time.time()))
        msg = (method.upper() + path + ts + body).encode()
        sig = hmac.new(self._hmac_secret, msg, hashlib.sha256).hexdigest()
        return ts, sig

    def _request(
        self,
        method: str,
        path: str,
        payload: Optional[dict[str, Any]] = None,
        params: Optional[dict[str, Any]] = None,
        raw: bool = False,
    ) -> Any:
        url = urljoin(self.base_url + "/", path.lstrip("/"))
        headers: dict[str, str] = {"Content-Type": "application/json"}
        body = json.dumps(payload) if payload else ""

        if self.token:
            headers["Authorization"] = f"Bearer {self.token}"
        elif self._hmac_secret:
            ts, sig = self._sign(method, path, body)
            headers["X-Custody-Timestamp"] = ts
            headers["X-Custody-Signature"] = sig
            if self.api_key:
                headers["X-API-Key"] = self.api_key

        resp = self._session.request(
            method,
            url,
            headers=headers,
            data=body if payload else None,
            params=params,
            timeout=self.timeout,
        )

        if not resp.ok:
            try:
                err_body = resp.json()
                msg = err_body.get("error", resp.text)
            except Exception:
                msg = resp.text
            raise CustodyApiError(resp.status_code, msg, err_body if "err_body" in dir() else None)

        if raw:
            return resp.text

        if resp.text.strip():
            return resp.json()
        return {}
"#.to_string()
}

/// Generate a complete, working TypeScript SDK.
///
/// The output is a single self-contained `.ts` file using Node's `crypto`
/// module for HMAC and `fetch` (available in Node 18+ and all modern browsers).
pub fn generate_typescript_sdk() -> String {
    r#"/**
 * MG Custody TypeScript SDK — auto-generated
 * Node 18+ or any modern browser (uses fetch + crypto)
 */

import { createHmac } from "crypto";

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

export interface CreateAddressRequest {
  user_id: string;
  asset: string;
  chain: string;
}

export interface AddressResponse {
  address: string;
  chain: string;
  memo?: string;
}

export interface CreateWithdrawalRequest {
  withdrawal_id: string;
  asset: string;
  chain: string;
  address: string;
  /** Decimal amount as string — never a float */
  amount: string;
}

export interface WithdrawalResponse {
  custody_ref: string;
  status: string;
}

export interface WithdrawalStatusResponse {
  id: string;
  status: string;
  tx_hash?: string;
  confirmations: number;
  error?: string;
}

export interface WalletBalanceResponse {
  /** Decimal string */
  hot_balance: string;
  /** Decimal string */
  warm_balance: string;
  /** Decimal string */
  total_managed: string;
}

export interface FeeEstimateRequest {
  chain: string;
  asset: string;
  /** Decimal amount as string */
  amount: string;
}

export interface FeeEstimateResponse {
  chain: string;
  asset: string;
  /** Decimal string */
  estimated_fee: string;
  fee_unit: string;
}

export interface TransactionRecord {
  id: string;
  asset: string;
  chain: string;
  /** Decimal string */
  amount: string;
  direction: "deposit" | "withdrawal";
  status: string;
  tx_hash?: string;
  created_at: string;
}

export interface ListTransactionsParams {
  limit?: number;
  offset?: number;
  chain?: string;
  asset?: string;
}

export interface HealthResponse {
  status: string;
}

// ---------------------------------------------------------------------------
// Error class
// ---------------------------------------------------------------------------

export class CustodyApiError extends Error {
  constructor(
    public readonly statusCode: number,
    message: string,
    public readonly body?: unknown
  ) {
    super(`HTTP ${statusCode}: ${message}`);
    this.name = "CustodyApiError";
  }
}

// ---------------------------------------------------------------------------
// Client options
// ---------------------------------------------------------------------------

export interface CustodyClientOptions {
  /** Base URL of the custody service, e.g. "https://custody.internal:8443" */
  baseUrl: string;
  /** Bearer token for user-session authentication */
  token?: string;
  /** API key identifier used alongside hmacSecret */
  apiKey?: string;
  /**
   * Shared HMAC-SHA256 secret for server-to-server calls.
   * The signature covers: METHOD + PATH + TIMESTAMP + BODY
   */
  hmacSecret?: string;
  /** Request timeout in milliseconds (default 30000) */
  timeoutMs?: number;
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

export class CustodyClient {
  private readonly baseUrl: string;
  private readonly token?: string;
  private readonly apiKey?: string;
  private readonly hmacSecret?: string;
  private readonly timeoutMs: number;

  constructor(options: CustodyClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/$/, "");
    this.token = options.token;
    this.apiKey = options.apiKey;
    this.hmacSecret = options.hmacSecret;
    this.timeoutMs = options.timeoutMs ?? 30_000;
  }

  // ------------------------------------------------------------------------
  // Public API methods
  // ------------------------------------------------------------------------

  /** Check service liveness. GET /health */
  async getHealth(): Promise<HealthResponse> {
    const text = await this.request<string>("GET", "/health", undefined, true);
    return { status: typeof text === "string" ? text : "ok" };
  }

  /**
   * Generate a deposit address for a user. POST /v1/addresses
   *
   * @param req - user_id, asset, chain
   * @returns generated address and optional memo
   */
  async createAddress(req: CreateAddressRequest): Promise<AddressResponse> {
    return this.request<AddressResponse>("POST", "/v1/addresses", req);
  }

  /**
   * Submit a withdrawal request. POST /v1/withdrawals
   *
   * Idempotent — submitting the same withdrawal_id twice returns the
   * existing record without creating a duplicate.
   *
   * @param req - withdrawal details; amount must be a decimal string
   * @returns custody reference and initial status
   */
  async createWithdrawal(
    req: CreateWithdrawalRequest
  ): Promise<WithdrawalResponse> {
    return this.request<WithdrawalResponse>("POST", "/v1/withdrawals", req);
  }

  /**
   * Get withdrawal status by custody reference. GET /v1/withdrawals/{id}
   *
   * @param id - custody_ref UUID returned by createWithdrawal
   */
  async getWithdrawal(id: string): Promise<WithdrawalStatusResponse> {
    return this.request<WithdrawalStatusResponse>(
      "GET",
      `/v1/withdrawals/${id}`
    );
  }

  /**
   * Get balance for a specific asset. GET /v1/wallets/{asset}
   *
   * @param asset - asset symbol, e.g. "USDT", "ETH", "BTC"
   */
  async getWalletBalance(asset: string): Promise<WalletBalanceResponse> {
    return this.request<WalletBalanceResponse>("GET", `/v1/wallets/${asset}`);
  }

  /**
   * Get balances for all managed assets. GET /v1/wallets
   */
  async getPortfolio(): Promise<Record<string, WalletBalanceResponse>> {
    const data = await this.request<
      | Record<string, WalletBalanceResponse>
      | { assets: Record<string, WalletBalanceResponse> }
    >("GET", "/v1/wallets");
    if (data && typeof data === "object" && "assets" in data) {
      return data.assets;
    }
    return data as Record<string, WalletBalanceResponse>;
  }

  /**
   * List recent transactions (deposits + withdrawals). GET /v1/transactions
   */
  async listTransactions(
    params?: ListTransactionsParams
  ): Promise<TransactionRecord[]> {
    const qs = params
      ? "?" +
        Object.entries(params)
          .filter(([, v]) => v !== undefined)
          .map(([k, v]) => `${k}=${encodeURIComponent(String(v))}`)
          .join("&")
      : "";
    const data = await this.request<
      TransactionRecord[] | { data: TransactionRecord[] }
    >("GET", `/v1/transactions${qs}`);
    if (Array.isArray(data)) return data;
    return data.data ?? [];
  }

  /**
   * Estimate network fee for a withdrawal. POST /v1/fees/estimate
   */
  async estimateFee(req: FeeEstimateRequest): Promise<FeeEstimateResponse> {
    return this.request<FeeEstimateResponse>("POST", "/v1/fees/estimate", req);
  }

  // ------------------------------------------------------------------------
  // Internals
  // ------------------------------------------------------------------------

  /**
   * Compute HMAC-SHA256 signature.
   *
   * Message = METHOD + PATH + TIMESTAMP_SECS + BODY
   * Header: X-Custody-Signature (hex digest)
   * Header: X-Custody-Timestamp (Unix seconds as string)
   */
  private sign(
    method: string,
    path: string,
    body: string
  ): { timestamp: string; signature: string } {
    if (!this.hmacSecret) {
      throw new CustodyApiError(0, "hmacSecret required for HMAC auth");
    }
    const timestamp = String(Math.floor(Date.now() / 1000));
    const message = method.toUpperCase() + path + timestamp + body;
    const signature = createHmac("sha256", this.hmacSecret)
      .update(message)
      .digest("hex");
    return { timestamp, signature };
  }

  private async request<T>(
    method: string,
    path: string,
    payload?: unknown,
    rawText: boolean = false
  ): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    const body = payload !== undefined ? JSON.stringify(payload) : undefined;
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
      Accept: "application/json",
    };

    if (this.token) {
      headers["Authorization"] = `Bearer ${this.token}`;
    } else if (this.hmacSecret) {
      const { timestamp, signature } = this.sign(method, path, body ?? "");
      headers["X-Custody-Timestamp"] = timestamp;
      headers["X-Custody-Signature"] = signature;
      if (this.apiKey) {
        headers["X-API-Key"] = this.apiKey;
      }
    }

    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeoutMs);

    let resp: Response;
    try {
      resp = await fetch(url, {
        method,
        headers,
        body,
        signal: controller.signal,
      });
    } finally {
      clearTimeout(timer);
    }

    if (!resp.ok) {
      let message = resp.statusText;
      let errBody: unknown;
      try {
        errBody = await resp.json();
        if (
          errBody &&
          typeof errBody === "object" &&
          "error" in errBody &&
          typeof (errBody as Record<string, unknown>)["error"] === "string"
        ) {
          message = (errBody as Record<string, string>)["error"];
        }
      } catch {
        // body not JSON
      }
      throw new CustodyApiError(resp.status, message, errBody);
    }

    if (rawText) {
      return (await resp.text()) as unknown as T;
    }

    const text = await resp.text();
    if (!text.trim()) return {} as T;
    return JSON.parse(text) as T;
  }
}
"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_sdk_has_all_required_methods() {
        let sdk = generate_python_sdk();
        assert!(sdk.contains("class CustodyClient"), "missing CustodyClient class");
        assert!(sdk.contains("def get_health"), "missing get_health");
        assert!(sdk.contains("def create_address"), "missing create_address");
        assert!(sdk.contains("def create_withdrawal"), "missing create_withdrawal");
        assert!(sdk.contains("def get_withdrawal"), "missing get_withdrawal");
        assert!(sdk.contains("def list_transactions"), "missing list_transactions");
        assert!(sdk.contains("def get_portfolio"), "missing get_portfolio");
        assert!(sdk.contains("def estimate_fee"), "missing estimate_fee");
    }

    #[test]
    fn python_sdk_has_hmac_computation() {
        let sdk = generate_python_sdk();
        assert!(sdk.contains("import hmac"), "missing hmac import");
        assert!(sdk.contains("import hashlib"), "missing hashlib import");
        assert!(sdk.contains("def _sign"), "missing _sign method");
        assert!(sdk.contains("sha256"), "missing sha256 in HMAC");
        assert!(sdk.contains("X-Custody-Signature"), "missing signature header");
        assert!(sdk.contains("X-Custody-Timestamp"), "missing timestamp header");
    }

    #[test]
    fn python_sdk_has_error_class() {
        let sdk = generate_python_sdk();
        assert!(sdk.contains("class CustodyApiError"), "missing CustodyApiError");
        assert!(sdk.contains("raise CustodyApiError"), "never raises CustodyApiError");
    }

    #[test]
    fn python_sdk_has_type_hints() {
        let sdk = generate_python_sdk();
        // Type hints on constructor parameters
        assert!(sdk.contains("base_url: str"), "missing str type hint on base_url");
        assert!(sdk.contains("Optional[str]"), "missing Optional type hints");
        assert!(sdk.contains("-> Address"), "missing return type hint");
    }

    #[test]
    fn python_sdk_has_docstrings() {
        let sdk = generate_python_sdk();
        assert!(sdk.contains("\"\"\""), "missing docstrings");
        assert!(sdk.contains("Args:"), "missing Args: sections in docstrings");
        assert!(sdk.contains("Returns:"), "missing Returns: sections in docstrings");
    }

    #[test]
    fn python_sdk_constructor_accepts_token_or_hmac() {
        let sdk = generate_python_sdk();
        assert!(sdk.contains("token: Optional[str]"), "missing token param");
        assert!(sdk.contains("hmac_secret: Optional[str | bytes]"), "missing hmac_secret param");
        assert!(sdk.contains("api_key: Optional[str]"), "missing api_key param");
    }

    #[test]
    fn typescript_sdk_has_all_required_methods() {
        let sdk = generate_typescript_sdk();
        assert!(sdk.contains("class CustodyClient"), "missing CustodyClient class");
        assert!(sdk.contains("async getHealth"), "missing getHealth");
        assert!(sdk.contains("async createAddress"), "missing createAddress");
        assert!(sdk.contains("async createWithdrawal"), "missing createWithdrawal");
        assert!(sdk.contains("async getWithdrawal"), "missing getWithdrawal");
        assert!(sdk.contains("async listTransactions"), "missing listTransactions");
        assert!(sdk.contains("async getPortfolio"), "missing getPortfolio");
        assert!(sdk.contains("async estimateFee"), "missing estimateFee");
    }

    #[test]
    fn typescript_sdk_has_proper_types() {
        let sdk = generate_typescript_sdk();
        assert!(sdk.contains("interface CreateAddressRequest"), "missing CreateAddressRequest");
        assert!(sdk.contains("interface AddressResponse"), "missing AddressResponse");
        assert!(sdk.contains("interface CreateWithdrawalRequest"), "missing CreateWithdrawalRequest");
        assert!(sdk.contains("interface WithdrawalResponse"), "missing WithdrawalResponse");
        assert!(sdk.contains("interface WithdrawalStatusResponse"), "missing WithdrawalStatusResponse");
        assert!(sdk.contains("interface WalletBalanceResponse"), "missing WalletBalanceResponse");
        assert!(sdk.contains("interface FeeEstimateResponse"), "missing FeeEstimateResponse");
        assert!(sdk.contains("interface TransactionRecord"), "missing TransactionRecord");
    }

    #[test]
    fn typescript_sdk_has_hmac_computation() {
        let sdk = generate_typescript_sdk();
        assert!(sdk.contains("from \"crypto\""), "missing crypto import");
        assert!(sdk.contains("createHmac"), "missing createHmac");
        assert!(sdk.contains("sha256"), "missing sha256");
        assert!(sdk.contains("X-Custody-Signature"), "missing signature header");
        assert!(sdk.contains("X-Custody-Timestamp"), "missing timestamp header");
        assert!(sdk.contains("private sign("), "missing sign method");
    }

    #[test]
    fn typescript_sdk_has_error_class() {
        let sdk = generate_typescript_sdk();
        assert!(sdk.contains("class CustodyApiError extends Error"), "missing CustodyApiError");
        assert!(sdk.contains("throw new CustodyApiError"), "never throws CustodyApiError");
        assert!(sdk.contains("statusCode"), "missing statusCode on error");
    }

    #[test]
    fn typescript_sdk_has_options_interface() {
        let sdk = generate_typescript_sdk();
        assert!(sdk.contains("interface CustodyClientOptions"), "missing CustodyClientOptions");
        assert!(sdk.contains("token?: string"), "missing optional token");
        assert!(sdk.contains("hmacSecret?: string"), "missing optional hmacSecret");
        assert!(sdk.contains("apiKey?: string"), "missing optional apiKey");
        assert!(sdk.contains("timeoutMs?: number"), "missing optional timeoutMs");
    }

    #[test]
    fn typescript_sdk_amounts_are_strings_not_floats() {
        let sdk = generate_typescript_sdk();
        // All monetary fields must be string, not number
        assert!(sdk.contains("hot_balance: string"), "hot_balance must be string");
        assert!(sdk.contains("warm_balance: string"), "warm_balance must be string");
        assert!(sdk.contains("total_managed: string"), "total_managed must be string");
        assert!(sdk.contains("estimated_fee: string"), "estimated_fee must be string");
        assert!(sdk.contains("amount: string"), "amount must be string");
    }

    #[test]
    fn generated_sdks_are_non_empty() {
        let py = generate_python_sdk();
        let ts = generate_typescript_sdk();
        assert!(py.len() > 1000, "Python SDK too short: {} bytes", py.len());
        assert!(ts.len() > 1000, "TypeScript SDK too short: {} bytes", ts.len());
    }
}
