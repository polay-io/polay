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

function formatTime(ts: string): string {
  const d = new Date(ts);
  return d.toLocaleTimeString();
}

function truncHash(h: string, len = 10): string {
  if (!h) return "";
  return h.length > len * 2 ? `${h.slice(0, len)}...${h.slice(-len)}` : h;
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
  });

  const supply = useQuery<SupplyInfo>({
    queryKey: ["supplyInfo"],
    queryFn: fetchSupplyInfo,
  });

  const blocks = useQuery<BlockSummary[]>({
    queryKey: ["recentBlocks"],
    queryFn: () => fetchRecentBlocks(10),
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
            <StatCard label="Block Height" value={chain.data.height} />
            <StatCard label="Finalized" value={chain.data.finalized_height} />
            <StatCard label="Epoch" value={chain.data.epoch} />
            <StatCard label="Validators" value={chain.data.active_validators} />
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
            <StatCard label="Total Supply" value={supply.data.total_supply} />
            <StatCard label="Circulating" value={supply.data.circulating_supply} />
            <StatCard label="Staked" value={supply.data.staked} />
            <StatCard label="Burned" value={supply.data.burned} />
            <StatCard label="Treasury" value={supply.data.treasury} />
          </div>
        )}
      </section>

      {/* Latest blocks */}
      <section className="section">
        <h3>Latest Blocks</h3>
        {blocks.isLoading && <p className="muted">Loading...</p>}
        {blocks.error && <p className="error">Failed to load blocks</p>}
        {blocks.data && (
          <table className="data-table">
            <thead>
              <tr>
                <th>Height</th>
                <th>Hash</th>
                <th>Txns</th>
                <th>Validator</th>
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
                  <td className="mono">{truncHash(b.validator, 8)}</td>
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
