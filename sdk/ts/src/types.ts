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
// Transaction actions -- discriminated union over all 40 on-chain operations
// ---------------------------------------------------------------------------

export type TransactionAction =
  // --- Core Financial ---
  | { type: "Transfer"; to: Address; amount: string }
  // --- Asset Management ---
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
  // --- Marketplace ---
  | {
      type: "CreateListing";
      asset_class_id: Hash;
      amount: string;
      price_per_unit: string;
      currency: Hash;
    }
  | { type: "CancelListing"; listing_id: Hash }
  | { type: "BuyListing"; listing_id: Hash }
  // --- Identity ---
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
  // --- Staking ---
  | { type: "RegisterValidator"; commission_bps: number }
  | { type: "DelegateStake"; validator: Address; amount: string }
  | { type: "UndelegateStake"; validator: Address; amount: string }
  // --- Attestation ---
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
    }
  // --- Governance ---
  | {
      type: "SubmitProposal";
      action: ProposalAction;
      title: string;
      description: string;
      deposit: string;
    }
  | { type: "VoteProposal"; proposal_id: Hash; option: VoteOption }
  | { type: "ExecuteProposal"; proposal_id: Hash }
  // --- Session Keys ---
  | {
      type: "CreateSession";
      session_pubkey: string;
      permissions: SessionPermission;
      expires_at: number;
      spending_limit: string;
    }
  | { type: "RevokeSession"; session_address: Address }
  // --- Asset Rentals ---
  | {
      type: "ListForRent";
      asset_class_id: Hash;
      asset_id: Hash;
      price_per_block: string;
      deposit: string;
      min_duration: number;
      max_duration: number;
    }
  | { type: "RentAsset"; rental_id: Hash; duration: number }
  | { type: "ReturnRental"; rental_id: Hash }
  | { type: "ClaimExpiredRental"; rental_id: Hash }
  | { type: "CancelRentalListing"; rental_id: Hash }
  // --- Guilds ---
  | { type: "CreateGuild"; name: string; description: string; max_members: number }
  | { type: "JoinGuild"; guild_id: Hash }
  | { type: "LeaveGuild"; guild_id: Hash }
  | { type: "GuildDeposit"; guild_id: Hash; amount: string }
  | { type: "GuildWithdraw"; guild_id: Hash; amount: string }
  | { type: "GuildPromote"; guild_id: Hash; member: Address; role: GuildRole }
  | { type: "GuildKick"; guild_id: Hash; member: Address }
  // --- Tournaments ---
  | {
      type: "CreateTournament";
      name: string;
      game_id: string;
      entry_fee: string;
      max_participants: number;
      min_participants: number;
      start_height: number;
      prize_distribution: number[];
    }
  | { type: "JoinTournament"; tournament_id: Hash }
  | { type: "StartTournament"; tournament_id: Hash }
  | { type: "ReportTournamentResults"; tournament_id: Hash; rankings: Address[] }
  | { type: "ClaimTournamentPrize"; tournament_id: Hash }
  | { type: "CancelTournament"; tournament_id: Hash };

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
// Governance
// ---------------------------------------------------------------------------

/** The type of on-chain proposal action. */
export type ProposalAction = "text" | "parameter_change" | "treasury_spend" | "upgrade";

/** Vote option for governance proposals. */
export type VoteOption = "yes" | "no" | "abstain";

/** Proposal status. */
export type ProposalStatus = "voting" | "passed" | "rejected" | "executed";

/** An on-chain governance proposal. */
export interface Proposal {
  id: Hash;
  proposer: Address;
  action: ProposalAction;
  title: string;
  description: string;
  deposit: string;
  status: ProposalStatus;
  yes_votes: string;
  no_votes: string;
  abstain_votes: string;
  created_at: number;
  voting_end: number;
}

// ---------------------------------------------------------------------------
// Session keys
// ---------------------------------------------------------------------------

/** Permissions granted to a session key. */
export type SessionPermission = "Transfer" | "Gaming" | "All";

/** An active session key delegation. */
export interface SessionInfo {
  granter: Address;
  session_address: Address;
  permissions: SessionPermission;
  expires_at: number;
  spending_limit: string;
  spent: string;
  created_at: number;
}

// ---------------------------------------------------------------------------
// Rentals
// ---------------------------------------------------------------------------

/** Rental listing/status. */
export type RentalStatus = "Listed" | "Active" | "Returned" | "Expired" | "Cancelled";

/** An asset rental record. */
export interface Rental {
  id: Hash;
  owner: Address;
  renter: Address | null;
  asset_class_id: Hash;
  asset_id: Hash;
  price_per_block: string;
  deposit: string;
  min_duration: number;
  max_duration: number;
  status: RentalStatus;
  start_block: number | null;
  end_block: number | null;
}

// ---------------------------------------------------------------------------
// Guilds
// ---------------------------------------------------------------------------

/** Guild member role. */
export type GuildRole = "Leader" | "Officer" | "Member";

/** An on-chain guild. */
export interface Guild {
  id: Hash;
  name: string;
  description: string;
  leader: Address;
  member_count: number;
  max_members: number;
  treasury: string;
  created_at: number;
}

/** A guild membership record. */
export interface GuildMembership {
  guild_id: Hash;
  member: Address;
  role: GuildRole;
  joined_at: number;
}

// ---------------------------------------------------------------------------
// Tournaments
// ---------------------------------------------------------------------------

/** Tournament lifecycle status. */
export type TournamentStatus = "Registration" | "Active" | "Completed" | "Cancelled";

/** An on-chain tournament. */
export interface Tournament {
  id: Hash;
  name: string;
  game_id: string;
  organizer: Address;
  entry_fee: string;
  prize_pool: string;
  max_participants: number;
  min_participants: number;
  current_participants: number;
  start_height: number;
  status: TournamentStatus;
  prize_distribution: number[];
  rankings: Address[];
  created_at: number;
}

// ---------------------------------------------------------------------------
// Economics
// ---------------------------------------------------------------------------

/** On-chain supply information. */
export interface SupplyInfo {
  total_supply: string;
  circulating: string;
  staked: string;
  burned: string;
  treasury: string;
  minted: string;
  fees_collected: string;
}

/** Current inflation rate info. */
export interface InflationRate {
  annual_rate_bps: number;
  epoch_reward: string;
}

// ---------------------------------------------------------------------------
// Epoch
// ---------------------------------------------------------------------------

/** Epoch metadata. */
export interface EpochInfo {
  epoch: number;
  start_height: number;
  end_height: number;
  block_count: number;
  total_fees: string;
  total_rewards: string;
}

// ---------------------------------------------------------------------------
// Node health & info
// ---------------------------------------------------------------------------

/** Health check response. */
export interface HealthResponse {
  status: string;
  height: number;
  syncing: boolean;
}

/** Node information response. */
export interface NodeInfo {
  chain_id: string;
  node_version: string;
  height: number;
  latest_hash: Hash;
  state_root: Hash;
  peer_count: number;
  mempool_size: number;
  uptime_seconds: number;
  block_time_ms: number;
}

/** Network-level statistics. */
export interface NetworkStats {
  height: number;
  total_transactions: number;
  active_validators: number;
  total_staked: string;
  epoch: number;
  block_time_ms: number;
}

/** Gas estimation result. */
export interface GasEstimate {
  gas_cost: number;
  max_fee_suggestion: string;
}

/** Transaction receipt. */
export interface TransactionReceipt {
  tx_hash: Hash;
  block_height: number;
  success: boolean;
  gas_used: number;
  fee: string;
  fee_payer: Address;
  events: Event[];
}

/** On-chain event. */
export interface Event {
  event_type: string;
  data: Record<string, unknown>;
}

/** Unbonding entry for staking. */
export interface UnbondingEntry {
  validator: Address;
  delegator: Address;
  amount: string;
  complete_at: number;
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
