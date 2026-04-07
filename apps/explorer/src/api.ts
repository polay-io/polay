const EXPLORER_API = "http://localhost:3001";
const RPC_URL = "http://localhost:9944";

// ---------------------------------------------------------------------------
// Explorer-API helpers
// ---------------------------------------------------------------------------

export interface BlockSummary {
  height: number;
  hash: string;
  parent_hash: string;
  timestamp: string;
  tx_count: number;
  validator: string;
}

export interface BlockDetail extends BlockSummary {
  state_root: string;
  epoch: number;
  transactions: TransactionSummary[];
}

export interface TransactionSummary {
  hash: string;
  sender: string;
  action_type: string;
  fee: string;
  block_height: number;
  status: string;
}

export async function fetchRecentBlocks(limit = 10): Promise<BlockSummary[]> {
  const res = await fetch(`${EXPLORER_API}/blocks?limit=${limit}`);
  if (!res.ok) throw new Error(`Failed to fetch blocks: ${res.statusText}`);
  return res.json();
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
  epoch: number;
  active_validators: number;
  finalized_height: number;
}

export interface AccountInfo {
  address: string;
  balance: string;
  nonce: number;
  staked: string;
}

export interface SupplyInfo {
  total_supply: string;
  circulating_supply: string;
  staked: string;
  burned: string;
  treasury: string;
}

export interface HealthStatus {
  healthy: boolean;
  peers: number;
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
