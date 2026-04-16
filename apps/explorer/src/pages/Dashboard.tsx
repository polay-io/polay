import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import {
  fetchChainInfo,
  fetchSupplyInfo,
  fetchRecentBlocks,
  type ChainInfo,
  type SupplyInfo,
  type BlockSummary,
} from "../api";

function formatTime(ts: number): string {
  const d = new Date(ts * 1000);
  return d.toLocaleTimeString();
}

function truncHash(h: string, len = 10): string {
  if (!h) return "";
  return h.length > len * 2 ? `${h.slice(0, len)}...${h.slice(-len)}` : h;
}

function formatPOL(raw: number): string {
  return (raw / 1_000_000).toLocaleString(undefined, { maximumFractionDigits: 2 });
}

function StatCard({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="stat-card">
      <span className="stat-label">{label}</span>
      <span className="stat-value">{String(value)}</span>
    </div>
  );
}

export default function Dashboard() {
  const chain = useQuery<ChainInfo>({
    queryKey: ["chainInfo"],
    queryFn: fetchChainInfo,
    retry: 2,
  });

  const supply = useQuery<SupplyInfo>({
    queryKey: ["supplyInfo"],
    queryFn: fetchSupplyInfo,
    retry: 2,
  });

  const blocks = useQuery<BlockSummary[]>({
    queryKey: ["recentBlocks"],
    queryFn: () => fetchRecentBlocks(10),
    retry: 2,
  });

  return (
    <div className="dashboard">
      <h2>Dashboard</h2>

      {/* Chain info */}
      <section className="section">
        <h3>Chain Info</h3>
        {chain.isLoading && <p className="muted">Loading...</p>}
        {chain.error && <p className="error">Failed to load chain info</p>}
        {chain.data && (
          <div className="stats-grid">
            <StatCard label="Block Height" value={chain.data.height.toLocaleString()} />
            <StatCard label="Chain ID" value={chain.data.chain_id} />
          </div>
        )}
      </section>

      {/* Supply info */}
      <section className="section">
        <h3>Supply</h3>
        {supply.isLoading && <p className="muted">Loading...</p>}
        {supply.error && <p className="error">Failed to load supply info</p>}
        {supply.data && (
          <div className="stats-grid">
            <StatCard label="Total Supply" value={`${formatPOL(supply.data.total_supply)} POL`} />
            <StatCard label="Circulating" value={`${formatPOL(supply.data.circulating_supply)} POL`} />
            <StatCard label="Staked" value={`${formatPOL(supply.data.total_staked)} POL`} />
            <StatCard label="Burned" value={`${formatPOL(supply.data.total_burned)} POL`} />
            <StatCard label="Treasury" value={`${formatPOL(supply.data.treasury_balance)} POL`} />
          </div>
        )}
      </section>

      {/* Latest blocks */}
      <section className="section">
        <h3>Latest Blocks</h3>
        {blocks.isLoading && <p className="muted">Loading...</p>}
        {blocks.error && <p className="error">Failed to load blocks</p>}
        {blocks.data && blocks.data.length > 0 && (
          <table className="data-table">
            <thead>
              <tr>
                <th>Height</th>
                <th>Hash</th>
                <th>Txns</th>
                <th>Proposer</th>
                <th>Time</th>
              </tr>
            </thead>
            <tbody>
              {blocks.data.map((b) => (
                <tr key={b.height}>
                  <td>
                    <Link to={`/block/${b.height}`} className="link">
                      {b.height}
                    </Link>
                  </td>
                  <td className="mono">
                    <Link to={`/block/${b.height}`} className="link">
                      {truncHash(b.hash)}
                    </Link>
                  </td>
                  <td>{b.tx_count}</td>
                  <td className="mono">{truncHash(b.proposer, 8)}</td>
                  <td>{formatTime(b.timestamp)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </div>
  );
}
