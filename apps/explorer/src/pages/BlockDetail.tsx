import { useParams, Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { fetchBlock, fetchBlockReward, type BlockDetail as BlockDetailT } from "../api";

function truncHash(h: string, len = 12): string {
  if (!h) return "";
  return h.length > len * 2 ? `${h.slice(0, len)}...${h.slice(-len)}` : h;
}

function formatTime(ts: number): string {
  return new Date(ts * 1000).toLocaleString();
}

function formatPOL(raw: number): string {
  return (raw / 1_000_000).toLocaleString(undefined, { maximumFractionDigits: 6 });
}

export default function BlockDetail() {
  const { height } = useParams<{ height: string }>();

  const { data, isLoading, error } = useQuery<BlockDetailT>({
    queryKey: ["block", height],
    queryFn: () => fetchBlock(height!),
    enabled: !!height,
    retry: 2,
  });

  const reward = useQuery<number>({
    queryKey: ["blockReward"],
    queryFn: fetchBlockReward,
    retry: 2,
  });

  if (isLoading) return <p className="muted">Loading block...</p>;
  if (error) return <p className="error">Failed to load block {height}</p>;
  if (!data) return null;

  return (
    <div className="detail-page">
      <h2>Block #{data.height}</h2>

      <table className="kv-table">
        <tbody>
          <tr><td className="kv-key">Height</td><td>{data.height.toLocaleString()}</td></tr>
          <tr><td className="kv-key">Hash</td><td className="mono">{data.hash}</td></tr>
          <tr><td className="kv-key">Parent Hash</td><td className="mono">{data.parent_hash}</td></tr>
          <tr><td className="kv-key">State Root</td><td className="mono">{data.state_root}</td></tr>
          <tr>
            <td className="kv-key">Proposer</td>
            <td className="mono">
              <Link to={`/account/${data.proposer}`} className="link">{data.proposer}</Link>
            </td>
          </tr>
          <tr>
            <td className="kv-key">Block Reward</td>
            <td>{reward.data != null ? `${formatPOL(reward.data)} POL` : "..."}</td>
          </tr>
          <tr><td className="kv-key">Timestamp</td><td>{formatTime(data.timestamp)}</td></tr>
          <tr><td className="kv-key">Transactions</td><td>{data.tx_count}</td></tr>
          <tr><td className="kv-key">Chain ID</td><td>{data.chain_id}</td></tr>
        </tbody>
      </table>

      {data.transactions && data.transactions.length > 0 && (
        <section className="section">
          <h3>Transactions</h3>
          <table className="data-table">
            <thead>
              <tr>
                <th>Hash</th>
                <th>Sender</th>
                <th>Action</th>
                <th>Fee</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {data.transactions.map((tx: any) => (
                <tr key={tx.hash || tx.tx_hash}>
                  <td className="mono">
                    <Link to={`/tx/${tx.hash || tx.tx_hash}`} className="link">
                      {truncHash(tx.hash || tx.tx_hash)}
                    </Link>
                  </td>
                  <td className="mono">
                    <Link to={`/account/${tx.sender}`} className="link">
                      {truncHash(tx.sender, 8)}
                    </Link>
                  </td>
                  <td>{tx.action_type}</td>
                  <td>{tx.fee}</td>
                  <td>
                    <span className={`badge ${tx.status === "success" ? "badge-ok" : "badge-err"}`}>
                      {tx.status}
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      )}

      {(!data.transactions || data.transactions.length === 0) && (
        <p className="muted" style={{ marginTop: "1.5rem" }}>
          No transactions in this block.
        </p>
      )}
    </div>
  );
}
