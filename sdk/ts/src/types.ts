// ---------------------------------------------------------------------------
// Primitive type aliases
// ---------------------------------------------------------------------------

/** Hex-encoded 32-byte address (64 hex characters). */
export type Address = string;

/** Hex-encoded 32-byte hash (64 hex characters). */
export type Hash = string;

/** Hex-encoded 64-byte Ed25519 signature (128 hex characters). */
export type Signature = string;

// ---------------------------------------------------------------------------
// Asset types
// ---------------------------------------------------------------------------

/** The kind of asset represented by an asset class. */
export type AssetType = "Fungible" | "NonFungible" | "SemiFungible";

/** Listing status on the marketplace. */
export type ListingStatus = "Active" | "Sold" | "Cancelled";

/** Validator operational status. */
export type ValidatorStatus = "Active" | "Jailed" | "Unbonding";

// ---------------------------------------------------------------------------
// Transaction actions -- discriminated union over all 17 on-chain operations
// ---------------------------------------------------------------------------

export type TransactionAction =
  | { type: "Transfer"; to: Address; amount: string }
  | {
      type: "CreateAssetClass";
      name: string;
      symbol: string;
      asset_type: AssetType;
      max_supply: string | null;
      metadata_uri: string;
    }
  | {
      type: "MintAsset";
      asset_class_id: Hash;
      to: Address;
      amount: string;
      metadata: string | null;
    }
  | {
      type: "TransferAsset";
      asset_class_id: Hash;
      to: Address;
      amount: string;
    }
  | { type: "BurnAsset"; asset_class_id: Hash; amount: string }
  | {
      type: "CreateListing";
      asset_class_id: Hash;
      amount: string;
      price_per_unit: string;
      currency: Hash;
    }
  | { type: "CancelListing"; listing_id: Hash }
  | { type: "BuyListing"; listing_id: Hash }
  | {
      type: "CreateProfile";
      username: string;
      display_name: string;
      metadata: string | null;
    }
  | {
      type: "AddAchievement";
      player: Address;
      achievement_id: string;
      name: string;
      metadata: string;
    }
  | { type: "UpdateReputation"; player: Address; delta: number; reason: string }
  | { type: "RegisterValidator"; commission_bps: number }
  | { type: "DelegateStake"; validator: Address; amount: string }
  | { type: "UndelegateStake"; validator: Address; amount: string }
  | {
      type: "RegisterAttestor";
      game_id: string;
      endpoint: string;
      metadata: string;
    }
  | { type: "SubmitMatchResult"; match_result: MatchResult }
  | {
      type: "DistributeReward";
      match_id: Hash;
      rewards: [Address, string][];
    };

// ---------------------------------------------------------------------------
// Transaction
// ---------------------------------------------------------------------------

/** An unsigned transaction ready to be signed. */
export interface Transaction {
  /** Chain identifier to prevent cross-chain replay. */
  chain_id: string;
  /** Monotonically increasing per-account nonce. */
  nonce: number;
  /** The account that authorizes this transaction (hex address). */
  signer: Address;
  /** The operation to execute. */
  action: TransactionAction;
  /** Maximum fee (in native tokens) the signer is willing to pay. */
  max_fee: string;
  /** Unix timestamp (seconds) when the transaction was created. */
  timestamp: number;
  /** If set, this transaction was signed by a session key. */
  session?: Address;
  /** If set, this address pays the gas fee instead of the signer (gas sponsorship). */
  sponsor?: Address;
}

/** A signed transaction with its hash. */
export interface SignedTransaction {
  /** The underlying transaction. */
  transaction: Transaction;
  /** Hex-encoded Ed25519 signature over the transaction bytes. */
  signature: Signature;
  /** Hex-encoded SHA-256 hash of the signed transaction. */
  tx_hash: Hash;
  /** Hex-encoded Ed25519 public key of the signer (32 bytes / 64 hex chars). */
  signer_pubkey: string;
}

// ---------------------------------------------------------------------------
// Block
// ---------------------------------------------------------------------------

/** Block header metadata. */
export interface BlockHeader {
  height: number;
  timestamp: number;
  hash: Hash;
  parent_hash: Hash;
  state_root: Hash;
  transactions_root: Hash;
  proposer: Address;
  chain_id: string;
}

/** A full block including transactions. */
export interface Block {
  height: number;
  timestamp: number;
  hash: Hash;
  parent_hash: Hash;
  state_root: Hash;
  transactions_root: Hash;
  proposer: Address;
  chain_id: string;
  tx_count: number;
  transactions: SignedTransaction[];
}

// ---------------------------------------------------------------------------
// State objects
// ---------------------------------------------------------------------------

/** On-chain account state. */
export interface AccountState {
  address: Address;
  nonce: number;
  balance: string;
  created_at: number;
}

/** An asset class definition. */
export interface AssetClass {
  id: Hash;
  name: string;
  symbol: string;
  asset_type: AssetType;
  total_supply: string;
  max_supply: string | null;
  creator: Address;
  metadata_uri: string;
  created_at: number;
}

/** An asset balance response. */
export interface AssetBalance {
  owner: Address;
  asset_class_id: Hash;
  amount: string;
}

/** A marketplace listing. */
export interface Listing {
  id: Hash;
  seller: Address;
  asset_class_id: Hash;
  amount: string;
  price_per_unit: string;
  currency: Hash;
  status: ListingStatus;
  royalty_bps: number;
  created_at: number;
}

/** A player's on-chain profile. */
export interface PlayerProfile {
  address: Address;
  username: string;
  display_name: string;
  reputation: number;
  metadata: string | null;
  created_at: number;
}

/** A soulbound achievement awarded to a player. */
export interface Achievement {
  player: Address;
  achievement_id: string;
  name: string;
  metadata: string;
  awarded_at: number;
}

/** Validator information. */
export interface ValidatorInfo {
  address: Address;
  stake: string;
  commission_bps: number;
  status: ValidatorStatus;
  jailed_until: number | null;
  blocks_produced: number;
}

/** An attestor registered for a specific game. */
export interface Attestor {
  address: Address;
  game_id: string;
  endpoint: string;
  metadata: string;
  registered_at: number;
}

/** A verified match result from a game attestor. */
export interface MatchResult {
  match_id: Hash;
  game_id: string;
  timestamp: number;
  players: Address[];
  scores: number[];
  winners: Address[];
  reward_pool: string;
  server_signature: number[];
  anti_cheat_score: number | null;
  replay_ref: string | null;
}

// ---------------------------------------------------------------------------
// Chain info
// ---------------------------------------------------------------------------

/** Top-level chain metadata. */
export interface ChainInfo {
  chain_id: string;
  height: number;
  latest_hash: Hash;
  block_time: number;
}

// ---------------------------------------------------------------------------
// JSON-RPC envelope types
// ---------------------------------------------------------------------------

/** A JSON-RPC 2.0 request. */
export interface JsonRpcRequest {
  jsonrpc: "2.0";
  id: number;
  method: string;
  params: unknown[];
}

/** A JSON-RPC 2.0 response. */
export interface JsonRpcResponse<T = unknown> {
  jsonrpc: "2.0";
  id: number;
  result?: T;
  error?: {
    code: number;
    message: string;
    data?: unknown;
  };
}
