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
} from "./examples.js";
