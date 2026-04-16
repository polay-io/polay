import { useQuery } from "@tanstack/react-query";
import { useState, useCallback } from "react";
import {
  fetchNodeInfo,
  fetchHealth,
  fetchMempoolSize,
  fetchChainInfo,
  fetchNetworkStats,
  formatUptime,
  type BlockResponse,
} from "../api";
import { useBlockSubscription } from "../hooks/useWebSocket";

export default function Network() {
  const health = useQuery({
    queryKey: ["health"],
    queryFn: fetchHealth,
    refetchInterval: 5000,
  });

  const nodeInfo = useQuery({
    queryKey: ["nodeInfo"],
    queryFn: fetchNodeInfo,
    refetchInterval: 5000,
  });

  const chainInfo = useQuery({
    queryKey: ["chainInfo"],
    queryFn: fetchChainInfo,
    refetchInterval: 5000,
  });

  const mempool = useQuery({
    queryKey: ["mempoolSize"],
    queryFn: fetchMempoolSize,
    refetchInterval: 3000,
  });

  const stats = useQuery({
    queryKey: ["networkStats"],
    queryFn: fetchNetworkStats,
    refetchInterval: 5000,
  });

  // Track block times
  const [blockTimes, setBlockTimes] = useState<number[]>([]);
  const [lastBlockTime, setLastBlockTime] = useState<number | null>(null);

  const wsConnected = useBlockSubscription(
    useCallback((block: unknown) => {
      const b = block as BlockResponse;
      const now = Date.now();
      if (lastBlockTime) {
        setBlockTimes((prev) => [...prev.slice(-29), now - lastBlockTime]);
      }
      setLastBlockTime(now);
    }, [lastBlockTime])
  );

  const ni = nodeInfo.data;
  const ci = chainInfo.data;
  const h = health.data;
  const isHealthy = h?.status === "ok" || h?.status === "healthy";

  const avgBlockTime =
    blockTimes.length > 0
      ? blockTimes.reduce((a, b) => a + b, 0) / blockTimes.length
      : null;

  return (
    <>
      <div className="page-header">
        <h2>Network</h2>
        <div className="live-indicator">
          {wsConnected && <div className="pulse" />}
          {wsConnected ? "WebSocket Connected" : "Polling"}
        </div>
      </div>

      {/* Node status */}
      <div className="grid grid-4" style={{ marginBottom: 20 }}>
        <div className="card">
          <div className="card-header"><h3>Node Status</h3></div>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span className={`status-dot ${isHealthy ? "green" : "red"}`} />
            <span className="stat-value" style={{ fontSize: 20 }}>
              {isHealthy ? "Healthy" : "Offline"}
            </span>
          </div>
          <div className="stat-label">
            {h?.syncing ? "Syncing..." : "Fully synced"}
          </div>
        </div>
        <div className="card stat-sm">
          <div className="card-header"><h3>Peers</h3></div>
          <div className="stat-value">{ni?.peer_count ?? "..."}</div>
          <div className="stat-label">Connected P2P peers</div>
        </div>
        <div className="card stat-sm">
          <div className="card-header"><h3>Mempool</h3></div>
          <div className="stat-value">{mempool.data ?? "..."}</div>
          <div className="stat-label">Pending transactions</div>
        </div>
        <div className="card stat-sm">
          <div className="card-header"><h3>Uptime</h3></div>
          <div className="stat-value">{ni ? formatUptime(ni.uptime_seconds) : "..."}</div>
          <div className="stat-label">Since last restart</div>
        </div>
      </div>

      {/* Chain details */}
      <div className="grid grid-2" style={{ marginBottom: 20 }}>
        <div className="card">
          <div className="card-header"><h3>Chain Info</h3></div>
          <div className="table-wrap">
            <table>
              <tbody>
                <tr>
                  <td style={{ color: "var(--text-label)", width: 160 }}>Chain ID</td>
                  <td className="mono">{ci?.chain_id ?? "..."}</td>
                </tr>
                <tr>
                  <td style={{ color: "var(--text-label)" }}>Height</td>
                  <td className="mono">{ci?.height?.toLocaleString() ?? "..."}</td>
                </tr>
                <tr>
                  <td style={{ color: "var(--text-label)" }}>Latest Hash</td>
                  <td className="mono" style={{ fontSize: 11, wordBreak: "break-all" }}>
                    {ci?.latest_hash ?? "..."}
                  </td>
                </tr>
                <tr>
                  <td style={{ color: "var(--text-label)" }}>State Root</td>
                  <td className="mono" style={{ fontSize: 11, wordBreak: "break-all" }}>
                    {ci?.state_root ?? "..."}
                  </td>
                </tr>
                <tr>
                  <td style={{ color: "var(--text-label)" }}>Node Version</td>
                  <td className="mono">{ni?.node_version ?? "..."}</td>
                </tr>
                <tr>
                  <td style={{ color: "var(--text-label)" }}>Epoch</td>
                  <td className="mono">{stats.data?.epoch ?? "..."}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>

        <div className="card">
          <div className="card-header"><h3>Block Production</h3></div>
          <div className="grid grid-2" style={{ gap: 16 }}>
            <div className="stat-sm">
              <div className="stat-value">
                {ni ? `${(ni.block_time_ms / 1000).toFixed(1)}s` : "..."}
              </div>
              <div className="stat-label">Target block time</div>
            </div>
            <div className="stat-sm">
              <div className="stat-value">
                {avgBlockTime ? `${(avgBlockTime / 1000).toFixed(2)}s` : "..."}
              </div>
              <div className="stat-label">
                Actual avg ({blockTimes.length} samples)
              </div>
            </div>
          </div>

          {/* Visual block time bars */}
          {blockTimes.length > 0 && (
            <div style={{ marginTop: 20 }}>
              <div style={{ fontSize: 11, color: "var(--text-dim)", marginBottom: 8 }}>
                Recent block intervals (ms)
              </div>
              <div style={{ display: "flex", alignItems: "end", gap: 2, height: 60 }}>
                {blockTimes.slice(-30).map((t, i) => {
                  const target = ni?.block_time_ms ?? 2000;
                  const ratio = t / target;
                  const height = Math.min(100, ratio * 50);
                  const color =
                    ratio < 0.8 ? "var(--blue)" :
                    ratio < 1.2 ? "var(--green)" :
                    ratio < 1.5 ? "var(--yellow)" : "var(--red)";
                  return (
                    <div
                      key={i}
                      title={`${t}ms`}
                      style={{
                        flex: 1,
                        height: `${height}%`,
                        background: color,
                        borderRadius: 2,
                        minWidth: 4,
                      }}
                    />
                  );
                })}
              </div>
            </div>
          )}
        </div>
      </div>

      {/* RPC endpoints */}
      <div className="card">
        <div className="card-header"><h3>RPC Endpoint</h3></div>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Endpoint</th>
                <th>Type</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td className="mono">http://178.104.202.101:9944</td>
                <td>JSON-RPC</td>
                <td><span className={`badge ${isHealthy ? "badge-green" : "badge-red"}`}>{isHealthy ? "Online" : "Offline"}</span></td>
              </tr>
              <tr>
                <td className="mono">ws://178.104.202.101:9944</td>
                <td>WebSocket</td>
                <td><span className={`badge ${wsConnected ? "badge-green" : "badge-red"}`}>{wsConnected ? "Connected" : "Disconnected"}</span></td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </>
  );
}
