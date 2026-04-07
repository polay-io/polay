import { useParams, Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { fetchAccount, type AccountInfo } from "../api";

function truncHash(h: string, len = 12): string {
  if (!h) return "";
  return h.length > len * 2 ? `${h.slice(0, len)}...${h.slice(-len)}` : h;
}

export default function AccountDetail() {
  const { address } = useParams<{ address: string }>();

  const { data, isLoading, error } = useQuery<AccountInfo>({
    queryKey: ["account", address],
    queryFn: () => fetchAccount(address!),
    enabled: !!address,
  });

  if (isLoading) return <p className="muted">Loading account...</p>;
  if (error) return <p className="error">Failed to load account</p>;
  if (!data) return null;

  return (
    <div className="detail-page">
      <h2>Account</h2>

      <table className="kv-table">
        <tbody>
          <tr>
            <td className="kv-key">Address</td>
            <td className="mono">{data.address}</td>
          </tr>
          <tr>
            <td className="kv-key">Balance</td>
            <td>{data.balance} POL</td>
          </tr>
          <tr>
            <td className="kv-key">Staked</td>
            <td>{data.staked} POL</td>
          </tr>
          <tr>
            <td className="kv-key">Nonce</td>
            <td>{data.nonce}</td>
          </tr>
        </tbody>
      </table>
    </div>
  );
}
