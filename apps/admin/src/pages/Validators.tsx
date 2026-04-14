import { useQuery } from "@tanstack/react-query";
import {
  fetchValidators,
  fetchNetworkStats,
  fetchCurrentEpoch,
  formatPOL,
  shortAddr,
} from "../api";

export default function Validators() {
  const validators = useQuery({
    queryKey: ["validators"],
    queryFn: fetchValidators,
    refetchInterval: 10000,
  });

  const stats = useQuery({
    queryKey: ["networkStats"],
    queryFn: fetchNetworkStats,
    refetchInterval: 10000,
  });

  const epoch = useQuery({
    queryKey: ["currentEpoch"],
    queryFn: fetchCurrentEpoch,
    refetchInterval: 10000,
  });

  const vs = validators.data ?? [];
  const totalStake = vs.reduce((sum, v) => sum + v.stake, 0);

  return (
    <>
      <div className="page-header">
        <h2>Validators</h2>
        <span style={{ fontSize: 13, color: "var(--text-dim)" }}>
          Epoch {epoch.data ?? "..."} | {vs.length} active | {formatPOL(stats.data?.total_staked ?? 0)} POL staked
        </span>
      </div>

      {/* Summary cards */}
      <div className="grid grid-4" style={{ marginBottom: 20 }}>
        <div className="card stat-sm">
          <div className="card-header"><h3>Active Validators</h3></div>
          <div className="stat-value">{vs.length}</div>
        </div>
        <div className="card stat-sm">
          <div className="card-header"><h3>Total Staked</h3></div>
          <div className="stat-value">{formatPOL(totalStake)}</div>
          <div className="stat-label">POL</div>
        </div>
        <div className="card stat-sm">
          <div className="card-header"><h3>Avg Commission</h3></div>
          <div className="stat-value">
            {vs.length > 0
              ? `${(vs.reduce((s, v) => s + v.commission_bps, 0) / vs.length / 100).toFixed(1)}%`
              : "..."}
          </div>
        </div>
        <div className="card stat-sm">
          <div className="card-header"><h3>Blocks Produced</h3></div>
          <div className="stat-value">
            {vs.reduce((s, v) => s + v.blocks_produced, 0).toLocaleString()}
          </div>
          <div className="stat-label">Total across all validators</div>
        </div>
      </div>

      {/* Validator table */}
      <div className="card">
        <div className="card-header"><h3>Validator Set</h3></div>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>#</th>
                <th>Address</th>
                <th>Status</th>
                <th style={{ textAlign: "right" }}>Stake (POL)</th>
                <th style={{ textAlign: "right" }}>Share</th>
                <th style={{ textAlign: "right" }}>Commission</th>
                <th style={{ textAlign: "right" }}>Blocks</th>
              </tr>
            </thead>
            <tbody>
              {vs
                .sort((a, b) => b.stake - a.stake)
                .map((v, i) => (
                  <tr key={v.address}>
                    <td>{i + 1}</td>
                    <td className="mono">{shortAddr(v.address)}</td>
                    <td>
                      {v.jailed_until ? (
                        <span className="badge badge-red">Jailed</span>
                      ) : v.status === "active" || v.status === "Active" ? (
                        <span className="badge badge-green">Active</span>
                      ) : (
                        <span className="badge badge-yellow">{v.status}</span>
                      )}
                    </td>
                    <td className="mono" style={{ textAlign: "right" }}>
                      {formatPOL(v.stake)}
                    </td>
                    <td style={{ textAlign: "right" }}>
                      {totalStake > 0 ? `${((v.stake / totalStake) * 100).toFixed(1)}%` : "..."}
                    </td>
                    <td style={{ textAlign: "right" }}>
                      {(v.commission_bps / 100).toFixed(1)}%
                    </td>
                    <td className="mono" style={{ textAlign: "right" }}>
                      {v.blocks_produced.toLocaleString()}
                    </td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>

        {/* Stake concentration warning */}
        {vs.length > 0 && (() => {
          const top = vs.sort((a, b) => b.stake - a.stake)[0];
          const pct = (top.stake / totalStake) * 100;
          if (pct > 33) {
            return (
              <div style={{
                marginTop: 16,
                padding: "10px 14px",
                background: "var(--yellow-dim)",
                borderRadius: 6,
                fontSize: 13,
                color: "var(--yellow)",
              }}>
                Warning: Top validator holds {pct.toFixed(1)}% of stake ({">"}33%).
                This exceeds the safety threshold for BFT consensus.
              </div>
            );
          }
          return null;
        })()}
      </div>
    </>
  );
}
