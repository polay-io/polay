// @polay/sdk - TypeScript SDK for the POLAY gaming blockchain
//
// Re-export the entire public API so consumers can import from "@polay/sdk".

// Types
export type {
  Address,
  Hash,
  Signature,
  AssetType,
  ListingStatus,
  ValidatorStatus,
  TransactionAction,
  Transaction,
  SignedTransaction,
  BlockHeader,
  Block,
  AccountState,
  AssetClass,
  AssetBalance,
  Listing,
  PlayerProfile,
  Achievement,
  ValidatorInfo,
  Attestor,
  MatchResult,
  ChainInfo,
  JsonRpcRequest,
  JsonRpcResponse,
  // Governance
  ProposalAction,
  VoteOption,
  ProposalStatus,
  Proposal,
  // Session keys
  SessionPermission,
  SessionInfo,
  // Rentals
  RentalStatus,
  Rental,
  // Guilds
  GuildRole,
  Guild,
  GuildMembership,
  // Tournaments
  TournamentStatus,
  Tournament,
  // Economics
  SupplyInfo,
  InflationRate,
  // Epoch
  EpochInfo,
  // Health & node info
  HealthResponse,
  NodeInfo,
  NetworkStats,
  GasEstimate,
  TransactionReceipt,
  Event,
  UnbondingEntry,
} from "./types.js";

// Client
export { PolayClient, RpcError, TransportError } from "./client.js";

// Keypair and hex utilities
export { PolayKeypair, bytesToHex, hexToBytes } from "./keypair.js";

// Transaction building and signing
export {
  TransactionBuilder,
  transactionSigningBytes,
} from "./transaction.js";

// Example workflows
export {
  exampleTokenLifecycle,
  exampleMarketplaceFlow,
  exampleAttestationFlow,
  exampleIdentityAndStaking,
  exampleGuildFlow,
  exampleTournamentFlow,
  exampleRentalFlow,
  exampleSessionKeyFlow,
} from "./examples.js";
