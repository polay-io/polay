const EXPLORER_API = "http://localhost:3001/api/v1";
const RPC_URL = "http://178.104.202.101:9944";

// ---------------------------------------------------------------------------
// Explorer-API helpers
// ---------------------------------------------------------------------------

export interface BlockSummary {
  height: number;
  hash: string;
  parent_hash: string;
  timestamp: number;
  tx_count: number;
  proposer: string;
  chain_id: string;
  state_root: string;
  transactions_root: string;
  transactions: unknown[];
}

export interface BlockDetail extends BlockSummary {}

export interface TransactionSummary {
  hash: string;
  sender: string;
  action_type: string;
  fee: string;
  block_height: number;
  status: string;
}

interface BlocksResponse {
  blocks: BlockSummary[];
  limit: number;
  offset: number;
  total: number;
}

export async function fetchRecentBlocks(limit = 10): Promise<BlockSummary[]> {
  const res = await fetch(`${EXPLORER_API}/blocks?limit=${limit}`);
  if (!res.ok) throw new Error(`Failed to fetch blocks: ${res.statusText}`);
  const data: BlocksResponse = await res.json();
  return data.blocks;
}

export async function fetchBlock(height: number | string): Promise<BlockDetail> {
  const res = await fetch(`${EXPLORER_API}/blocks/${height}`);
  if (!res.ok) throw new Error(`Failed to fetch block ${height}: ${res.statusText}`);
  return res.json();
}

export async function fetchTransaction(hash: string): Promise<TransactionSummary> {
  const res = await fetch(`${EXPLORER_API}/transactions/${hash}`);
  if (!res.ok) throw new Error(`Failed to fetch tx ${hash}: ${res.statusText}`);
  return res.json();
}

// ---------------------------------------------------------------------------
// JSON-RPC helpers
// ---------------------------------------------------------------------------

let rpcId = 0;

async function rpcCall<T>(method: string, params: unknown[] = []): Promise<T> {
  const res = await fetch(RPC_URL, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: ++rpcId,
      method,
      params,
    }),
  });
  if (!res.ok) throw new Error(`RPC ${method} failed: ${res.statusText}`);
  const json = await res.json();
  if (json.error) throw new Error(`RPC error: ${json.error.message}`);
  return json.result as T;
}

export interface ChainInfo {
  chain_id: string;
  height: number;
  latest_hash: string;
  state_root: string;
  block_time: number;
}

export interface AccountInfo {
  address: string;
  balance: number;
  nonce: number;
}

export interface SupplyInfo {
  total_supply: number;
  circulating_supply: number;
  total_staked: number;
  total_burned: number;
  treasury_balance: number;
  total_minted: number;
  total_fees_collected: number;
}

export interface HealthStatus {
  status: string;
  height: number;
  syncing: boolean;
}

export function fetchChainInfo(): Promise<ChainInfo> {
  return rpcCall<ChainInfo>("polay_getChainInfo");
}

export function fetchAccount(address: string): Promise<AccountInfo> {
  return rpcCall<AccountInfo>("polay_getAccount", [address]);
}

export function fetchSupplyInfo(): Promise<SupplyInfo> {
  return rpcCall<SupplyInfo>("polay_getSupplyInfo");
}

export function fetchHealth(): Promise<HealthStatus> {
  return rpcCall<HealthStatus>("polay_health");
}

export function fetchBlockReward(): Promise<number> {
  return rpcCall<number>("polay_getBlockReward");
}
