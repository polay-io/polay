const RPC_URL = "http://127.0.0.1:9944";

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

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ChainInfo {
  chain_id: string;
  height: number;
  latest_hash: string;
  state_root: string;
  block_time: number;
}

export interface NodeInfo {
  chain_id: string;
  node_version: string;
  height: number;
  peer_count: number;
  mempool_size: number;
  uptime_seconds: number;
  block_time_ms: number;
}

export interface NetworkStats {
  height: number;
  total_transactions: number;
  active_validators: number;
  total_staked: number;
  epoch: number;
  block_time_ms: number;
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

export interface InflationRate {
  rate_bps: number;
  epoch_reward: number;
}

export interface HealthStatus {
  status: string;
  height: number;
  syncing: boolean;
}

export interface ValidatorInfo {
  address: string;
  stake: number;
  commission_bps: number;
  status: string;
  jailed_until: number | null;
  blocks_produced: number;
}

export interface BlockResponse {
  height: number;
  hash: string;
  parent_hash: string;
  timestamp: number;
  tx_count: number;
  proposer: string;
  state_root: string;
  chain_id: string;
  transactions: TxSummary[];
}

export interface TxSummary {
  hash: string;
  sender: string;
  action_type: string;
  fee: string;
  block_height: number;
  status: string;
}

export interface ProposalInfo {
  id: string;
  proposer: string;
  title: string;
  description: string;
  status: string;
  yes_votes: number;
  no_votes: number;
  voting_start_height: number;
  voting_end_height: number;
}

export interface EpochInfo {
  validator_set: ValidatorInfo[];
  total_staked: number;
  rewards_distributed: number;
}

// ---------------------------------------------------------------------------
// RPC calls
// ---------------------------------------------------------------------------

export const fetchChainInfo = () => rpcCall<ChainInfo>("polay_getChainInfo");
export const fetchNodeInfo = () => rpcCall<NodeInfo>("polay_getNodeInfo");
export const fetchNetworkStats = () => rpcCall<NetworkStats>("polay_getNetworkStats");
export const fetchSupplyInfo = () => rpcCall<SupplyInfo>("polay_getSupplyInfo");
export const fetchInflationRate = () => rpcCall<InflationRate>("polay_getInflationRate");
export const fetchHealth = () => rpcCall<HealthStatus>("polay_health");
export const fetchMempoolSize = () => rpcCall<number>("polay_getMempoolSize");
export const fetchBlockReward = () => rpcCall<number>("polay_getBlockReward");
export const fetchLatestBlock = () => rpcCall<BlockResponse>("polay_getLatestBlock");
export const fetchBlock = (height: number) => rpcCall<BlockResponse>("polay_getBlock", [height]);
export const fetchValidators = () => rpcCall<ValidatorInfo[]>("polay_getActiveValidatorSet");
export const fetchValidator = (addr: string) => rpcCall<ValidatorInfo>("polay_getValidator", [addr]);
export const fetchProposals = () => rpcCall<ProposalInfo[]>("polay_getProposals");
export const fetchCurrentEpoch = () => rpcCall<number>("polay_getCurrentEpoch");
export const fetchEpochInfo = (epoch: number) => rpcCall<EpochInfo>("polay_getEpochInfo", [epoch]);

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

export function formatPOL(amount: number): string {
  return (amount / 1_000_000).toLocaleString(undefined, {
    minimumFractionDigits: 0,
    maximumFractionDigits: 2,
  });
}

export function shortAddr(addr: string): string {
  if (addr.length <= 16) return addr;
  return `${addr.slice(0, 8)}...${addr.slice(-6)}`;
}

export function formatUptime(seconds: number): string {
  const d = Math.floor(seconds / 86400);
  const h = Math.floor((seconds % 86400) / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (d > 0) return `${d}d ${h}h ${m}m`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}

export function timeAgo(timestamp: number | string): string {
  const ts = typeof timestamp === "number" ? timestamp * 1000 : new Date(timestamp).getTime();
  const diff = Date.now() - ts;
  const secs = Math.floor(diff / 1000);
  if (secs < 60) return `${secs}s ago`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  return `${hours}h ago`;
}
