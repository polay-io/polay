import { useParams, Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { fetchBlock, type BlockDetail as BlockDetailT } from "../api";

function truncHash(h: string, len = 12): string {
  if (!h) return "";
  return h.length > len * 2 ? `${h.slice(0, len)}...${h.slice(-len)}` : h;
}

export default function BlockDetail() {
  const { height } = useParams<{ height: string }>();

  const { data, isLoading, error } = useQuery<BlockDetailT>({
    queryKey: ["block", height],
    queryFn: () => fetchBlock(height!),
    enabled: !!height,
  });

  if (isLoading) return <p className="muted">Loading block...</p>;
  if (error) return <p className="error">Failed to load block {height}</p>;
  if (!data) return null;

  return (
    <div className="detail-page">
      <h2>Block #{data.height}</h2>

      <table className="kv-table">
        <tbody>
          <tr><td className="kv-key">Height</td><td>{data.height}</td></tr>
          <tr><td className="kv-key">Hash</td><td className="mono">{data.hash}</td></tr>
          <tr><td className="kv-key">Parent Hash</td><td className="mono">{data.parent_hash}</td></tr>
          <tr><td className="kv-key">State Root</td><td className="mono">{data.state_root}</td></tr>
          <tr><td className="kv-key">Epoch</td><td>{data.epoch}</td></tr>
          <tr><td className="kv-key">Validator</td>
            <td className="mono">
              <Link to={`/account/${data.validator}`} className="link">{data.validator}</Link>
            </td>
          </tr>
          <tr><td className="kv-key">Timestamp</td><td>{new Date(data.timestamp).toLocaleString()}</td></tr>
          <tr><td className="kv-key">Transactions</td><td>{data.tx_count}</td></tr>
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
              {data.transactions.map((tx) => (
                <tr key={tx.hash}>
                  <td className="mono">
                    <Link to={`/tx/${tx.hash}`} className="link">
                      {truncHash(tx.hash)}
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
