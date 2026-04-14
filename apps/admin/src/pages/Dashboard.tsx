import { useQuery } from "@tanstack/react-query";
import {
  fetchNetworkStats,
  fetchSupplyInfo,
  fetchNodeInfo,
  fetchInflationRate,
  fetchLatestBlock,
  formatPOL,
  formatUptime,
  shortAddr,
  timeAgo,
} from "../api";

export default function Dashboard() {
  const stats = useQuery({
    queryKey: ["networkStats"],
    queryFn: fetchNetworkStats,
    refetchInterval: 4000,
    retry: 2,
    throwOnError: false,
  });

  const supply = useQuery({
    queryKey: ["supplyInfo"],
    queryFn: fetchSupplyInfo,
    refetchInterval: 10000,
    retry: 2,
    throwOnError: false,
  });

  const nodeInfo = useQuery({
    queryKey: ["nodeInfo"],
    queryFn: fetchNodeInfo,
    refetchInterval: 5000,
    retry: 2,
    throwOnError: false,
  });

  const inflation = useQuery({
    queryKey: ["inflationRate"],
    queryFn: fetchInflationRate,
    refetchInterval: 30000,
    retry: 2,
    throwOnError: false,
  });

  const latestBlock = useQuery({
    queryKey: ["latestBlock"],
    queryFn: fetchLatestBlock,
    refetchInterval: 4000,
    retry: 2,
    throwOnError: false,
  });

  const s = stats.data;
  const sup = supply.data;
  const ni = nodeInfo.data;
  const inf = inflation.data;
  const lb = latestBlock.data;

  return (
    <>
      <div className="page-header">
        <h2>Dashboard</h2>
        {stats.isError && (
          <span style={{ fontSize: 13, color: "var(--red)" }}>
            Node unreachable — retrying...
          </span>
        )}
      </div>

      {/* Key metrics */}
      <div className="grid grid-4" style={{ marginBottom: 20 }}>
        <div className="card">
          <div className="card-header"><h3>Block Height</h3></div>
          <div className="stat-value">{s?.height?.toLocaleString() ?? "..."}</div>
          <div className="stat-label">Epoch {s?.epoch ?? "..."}</div>
        </div>
        <div className="card">
          <div className="card-header"><h3>Transactions</h3></div>
          <div className="stat-value">{s?.total_transactions?.toLocaleString() ?? "..."}</div>
          <div className="stat-label">Total confirmed</div>
        </div>
        <div className="card">
          <div className="card-header"><h3>Validators</h3></div>
          <div className="stat-value">{s?.active_validators ?? "..."}</div>
          <div className="stat-label">Active in current epoch</div>
        </div>
        <div className="card">
          <div className="card-header"><h3>Uptime</h3></div>
          <div className="stat-value">{ni ? formatUptime(ni.uptime_seconds) : "..."}</div>
          <div className="stat-label">
            {ni ? `${ni.peer_count} peers | mempool: ${ni.mempool_size}` : "..."}
          </div>
        </div>
      </div>

      {/* Supply + Economics */}
      <div className="grid grid-2" style={{ marginBottom: 20 }}>
        <div className="card">
          <div className="card-header"><h3>Token Supply</h3></div>
          {sup ? (
            <>
              <div className="stat-value">{formatPOL(sup.total_supply)} POL</div>
              <div className="supply-bar">
                <div
                  className="circulating"
                  style={{ width: `${sup.total_supply > 0 ? ((sup.circulating_supply - sup.total_staked) / sup.total_supply) * 100 : 0}%` }}
                />
                <div
                  className="staked"
                  style={{ width: `${sup.total_supply > 0 ? (sup.total_staked / sup.total_supply) * 100 : 0}%` }}
                />
                <div
                  className="treasury"
                  style={{ width: `${sup.total_supply > 0 ? (sup.treasury_balance / sup.total_supply) * 100 : 0}%` }}
                />
                <div
                  className="burned"
                  style={{ width: `${sup.total_supply > 0 ? (sup.total_burned / sup.total_supply) * 100 : 0}%` }}
                />
              </div>
              <div className="supply-legend">
                <span><span className="dot" style={{ background: "var(--green)" }} /> Circulating: {formatPOL(sup.circulating_supply - sup.total_staked)}</span>
                <span><span className="dot" style={{ background: "var(--accent)" }} /> Staked: {formatPOL(sup.total_staked)}</span>
                <span><span className="dot" style={{ background: "var(--blue)" }} /> Treasury: {formatPOL(sup.treasury_balance)}</span>
                <span><span className="dot" style={{ background: "var(--red)" }} /> Burned: {formatPOL(sup.total_burned)}</span>
              </div>
            </>
          ) : (
            <div className="stat-value">...</div>
          )}
        </div>
        <div className="card">
          <div className="card-header"><h3>Economics</h3></div>
          <div className="grid grid-2" style={{ gap: 12 }}>
            <div className="stat-sm">
              <div className="stat-value">
                {inf ? `${(inf.rate_bps / 100).toFixed(1)}%` : "..."}
              </div>
              <div className="stat-label">Inflation rate</div>
            </div>
            <div className="stat-sm">
              <div className="stat-value">
                {inf ? formatPOL(inf.epoch_reward) : "..."}
              </div>
              <div className="stat-label">Epoch reward (POL)</div>
            </div>
            <div className="stat-sm">
              <div className="stat-value">
                {s ? `${(s.block_time_ms / 1000).toFixed(1)}s` : "..."}
              </div>
              <div className="stat-label">Block time</div>
            </div>
            <div className="stat-sm">
              <div className="stat-value">
                {sup ? formatPOL(sup.total_fees_collected) : "..."}
              </div>
              <div className="stat-label">Total fees collected</div>
            </div>
          </div>
        </div>
      </div>

      {/* Latest block */}
      <div className="card">
        <div className="card-header">
          <h3>Latest Block</h3>
        </div>
        {lb ? (
          <div className="block-row">
            <div className="block-height">#{lb.height}</div>
            <div className="block-hash">{lb.hash}</div>
            <div className="block-meta">
              {lb.tx_count} tx{lb.tx_count !== 1 ? "s" : ""}
            </div>
            <div className="block-meta">{shortAddr(lb.proposer)}</div>
            <div className="block-meta">{timeAgo(lb.timestamp)}</div>
          </div>
        ) : (
          <div style={{ color: "var(--text-dim)", padding: 20, textAlign: "center" }}>
            {stats.isError ? "Waiting for node connection..." : "Loading..."}
          </div>
        )}
      </div>
    </>
  );
}
