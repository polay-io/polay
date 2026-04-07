import type {
  AccountState,
  AssetBalance,
  AssetClass,
  Attestor,
  Block,
  ChainInfo,
  JsonRpcRequest,
  JsonRpcResponse,
  Listing,
  MatchResult,
  PlayerProfile,
  SignedTransaction,
  ValidatorInfo,
} from "./types.js";

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/** Error thrown when the RPC node returns a JSON-RPC error object. */
export class RpcError extends Error {
  /** The JSON-RPC error code. */
  readonly code: number;
  /** Optional structured data attached to the error. */
  readonly data: unknown;

  constructor(code: number, message: string, data?: unknown) {
    super(message);
    this.name = "RpcError";
    this.code = code;
    this.data = data;
  }
}

/** Error thrown when the HTTP transport fails. */
export class TransportError extends Error {
  readonly status: number;

  constructor(status: number, message: string) {
    super(message);
    this.name = "TransportError";
    this.status = status;
  }
}

// ---------------------------------------------------------------------------
// PolayClient
// ---------------------------------------------------------------------------

/**
 * Client for the POLAY gaming blockchain JSON-RPC API.
 *
 * ```ts
 * const client = new PolayClient("http://localhost:9944");
 * const info = await client.getChainInfo();
 * console.log(info.chain_id, "height", info.height);
 * ```
 */
export class PolayClient {
  private readonly rpcUrl: string;
  private nextId: number = 1;

  constructor(rpcUrl: string = "http://localhost:9944") {
    // Strip trailing slash for consistency.
    this.rpcUrl = rpcUrl.replace(/\/+$/, "");
  }

  // -------------------------------------------------------------------------
  // Transaction submission
  // -------------------------------------------------------------------------

  /**
   * Submit a signed transaction to the mempool.
   *
   * @returns The hex-encoded transaction hash.
   */
  async submitTransaction(signedTx: SignedTransaction): Promise<string> {
    const res = await this.rpcCall<{ tx_hash: string }>(
      "polay_submitTransaction",
      [signedTx],
    );
    return res.tx_hash;
  }

  // -------------------------------------------------------------------------
  // Block queries
  // -------------------------------------------------------------------------

  /** Fetch a block by height. Returns `null` if the height has not been produced. */
  async getBlock(height: number): Promise<Block | null> {
    return this.rpcCall<Block | null>("polay_getBlock", [height]);
  }

  /** Fetch the latest (highest) block. Returns `null` if no blocks exist. */
  async getLatestBlock(): Promise<Block | null> {
    return this.rpcCall<Block | null>("polay_getLatestBlock", []);
  }

  // -------------------------------------------------------------------------
  // Account queries
  // -------------------------------------------------------------------------

  /** Fetch on-chain account state by address. */
  async getAccount(address: string): Promise<AccountState | null> {
    return this.rpcCall<AccountState | null>("polay_getAccount", [address]);
  }

  /**
   * Fetch the native token balance for an address.
   *
   * Returns `"0"` if the account does not exist.
   */
  async getBalance(address: string): Promise<string> {
    const result = await this.rpcCall<number>("polay_getBalance", [address]);
    return String(result);
  }

  // -------------------------------------------------------------------------
  // Asset queries
  // -------------------------------------------------------------------------

  /** Fetch an asset class definition by its ID. */
  async getAssetClass(id: string): Promise<AssetClass | null> {
    return this.rpcCall<AssetClass | null>("polay_getAssetClass", [id]);
  }

  /**
   * Fetch the balance of a specific asset class for an owner.
   *
   * Returns `"0"` if the owner holds none of that asset.
   */
  async getAssetBalance(assetClassId: string, owner: string): Promise<string> {
    const result = await this.rpcCall<AssetBalance>(
      "polay_getAssetBalance",
      [assetClassId, owner],
    );
    return String(result.amount);
  }

  // -------------------------------------------------------------------------
  // Marketplace queries
  // -------------------------------------------------------------------------

  /** Fetch a marketplace listing by its ID. */
  async getListing(id: string): Promise<Listing | null> {
    return this.rpcCall<Listing | null>("polay_getListing", [id]);
  }

  // -------------------------------------------------------------------------
  // Identity queries
  // -------------------------------------------------------------------------

  /** Fetch a player profile by address. */
  async getProfile(address: string): Promise<PlayerProfile | null> {
    return this.rpcCall<PlayerProfile | null>("polay_getProfile", [address]);
  }

  // -------------------------------------------------------------------------
  // Staking queries
  // -------------------------------------------------------------------------

  /** Fetch validator information by address. */
  async getValidator(address: string): Promise<ValidatorInfo | null> {
    return this.rpcCall<ValidatorInfo | null>("polay_getValidator", [address]);
  }

  // -------------------------------------------------------------------------
  // Attestation queries
  // -------------------------------------------------------------------------

  /** Fetch attestor information by address. */
  async getAttestor(address: string): Promise<Attestor | null> {
    return this.rpcCall<Attestor | null>("polay_getAttestor", [address]);
  }

  /** Fetch a verified match result by match ID. */
  async getMatchResult(matchId: string): Promise<MatchResult | null> {
    return this.rpcCall<MatchResult | null>("polay_getMatchResult", [matchId]);
  }

  // -------------------------------------------------------------------------
  // Chain metadata
  // -------------------------------------------------------------------------

  /** Fetch top-level chain information (chain ID, height, latest hash). */
  async getChainInfo(): Promise<ChainInfo> {
    return this.rpcCall<ChainInfo>("polay_getChainInfo", []);
  }

  /** Fetch the current number of transactions in the mempool. */
  async getMempoolSize(): Promise<number> {
    return this.rpcCall<number>("polay_getMempoolSize", []);
  }

  // -------------------------------------------------------------------------
  // Transaction lookup
  // -------------------------------------------------------------------------

  /** Look up a transaction by hash (checks mempool). */
  async getTransaction(txHash: string): Promise<SignedTransaction | null> {
    return this.rpcCall<SignedTransaction | null>(
      "polay_getTransaction",
      [txHash],
    );
  }

  // -------------------------------------------------------------------------
  // Internal JSON-RPC transport
  // -------------------------------------------------------------------------

  /**
   * Make a JSON-RPC 2.0 call to the POLAY node.
   *
   * @param method  The RPC method name (e.g. `"polay_getBlock"`).
   * @param params  Positional parameters.
   * @returns The `result` field of the JSON-RPC response.
   * @throws {RpcError} if the node returns a JSON-RPC error.
   * @throws {TransportError} if the HTTP request fails.
   */
  private async rpcCall<T>(method: string, params: unknown[]): Promise<T> {
    const id = this.nextId++;

    const body: JsonRpcRequest = {
      jsonrpc: "2.0",
      id,
      method,
      params,
    };

    let response: Response;
    try {
      response = await fetch(this.rpcUrl, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      throw new TransportError(0, `Failed to connect to ${this.rpcUrl}: ${msg}`);
    }

    if (!response.ok) {
      throw new TransportError(
        response.status,
        `HTTP ${response.status}: ${response.statusText}`,
      );
    }

    const json: JsonRpcResponse<T> = await response.json();

    if (json.error) {
      throw new RpcError(
        json.error.code,
        json.error.message,
        json.error.data,
      );
    }

    return json.result as T;
  }
}
