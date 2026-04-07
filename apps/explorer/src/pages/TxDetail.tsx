import { useParams, Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { fetchTransaction, type TransactionSummary } from "../api";

export default function TxDetail() {
  const { hash } = useParams<{ hash: string }>();

  const { data, isLoading, error } = useQuery<TransactionSummary>({
    queryKey: ["tx", hash],
    queryFn: () => fetchTransaction(hash!),
    enabled: !!hash,
  });

  if (isLoading) return <p className="muted">Loading transaction...</p>;
  if (error) return <p className="error">Failed to load transaction</p>;
  if (!data) return null;

  return (
    <div className="detail-page">
      <h2>Transaction</h2>

      <table className="kv-table">
        <tbody>
          <tr>
            <td className="kv-key">Hash</td>
            <td className="mono">{data.hash}</td>
          </tr>
          <tr>
            <td className="kv-key">Sender</td>
            <td className="mono">
              <Link to={`/account/${data.sender}`} className="link">{data.sender}</Link>
            </td>
          </tr>
          <tr>
            <td className="kv-key">Action Type</td>
            <td>{data.action_type}</td>
          </tr>
          <tr>
            <td className="kv-key">Fee</td>
            <td>{data.fee}</td>
          </tr>
          <tr>
            <td className="kv-key">Block Height</td>
            <td>
              <Link to={`/block/${data.block_height}`} className="link">
                {data.block_height}
              </Link>
            </td>
          </tr>
          <tr>
            <td className="kv-key">Status</td>
            <td>
              <span className={`badge ${data.status === "success" ? "badge-ok" : "badge-err"}`}>
                {data.status}
              </span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  );
}
