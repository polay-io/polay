import { useQuery } from "@tanstack/react-query";
import { fetchProposals, fetchNetworkStats, shortAddr } from "../api";

export default function Governance() {
  const proposals = useQuery({
    queryKey: ["proposals"],
    queryFn: fetchProposals,
    refetchInterval: 15000,
  });

  const stats = useQuery({
    queryKey: ["networkStats"],
    queryFn: fetchNetworkStats,
    refetchInterval: 10000,
  });

  const ps = proposals.data ?? [];
  const currentHeight = stats.data?.height ?? 0;

  const active = ps.filter((p) => p.status === "active" || p.status === "Active" || p.status === "voting");
  const passed = ps.filter((p) => p.status === "passed" || p.status === "Passed");
  const rejected = ps.filter((p) => p.status === "rejected" || p.status === "Rejected" || p.status === "failed");

  return (
    <>
      <div className="page-header">
        <h2>Governance</h2>
        <span style={{ fontSize: 13, color: "var(--text-dim)" }}>
          {ps.length} total proposals
        </span>
      </div>

      <div className="grid grid-3" style={{ marginBottom: 20 }}>
        <div className="card stat-sm">
          <div className="card-header"><h3>Active</h3></div>
          <div className="stat-value" style={{ color: "var(--blue)" }}>{active.length}</div>
          <div className="stat-label">Currently voting</div>
        </div>
        <div className="card stat-sm">
          <div className="card-header"><h3>Passed</h3></div>
          <div className="stat-value" style={{ color: "var(--green)" }}>{passed.length}</div>
        </div>
        <div className="card stat-sm">
          <div className="card-header"><h3>Rejected</h3></div>
          <div className="stat-value" style={{ color: "var(--red)" }}>{rejected.length}</div>
        </div>
      </div>

      <div className="card">
        <div className="card-header"><h3>All Proposals</h3></div>
        {ps.length === 0 ? (
          <div style={{ color: "var(--text-dim)", padding: 30, textAlign: "center" }}>
            No governance proposals yet. Proposals can be submitted via the SDK.
          </div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>ID</th>
                  <th>Title</th>
                  <th>Proposer</th>
                  <th>Status</th>
                  <th style={{ textAlign: "right" }}>Yes</th>
                  <th style={{ textAlign: "right" }}>No</th>
                  <th>Progress</th>
                </tr>
              </thead>
              <tbody>
                {ps.map((p) => {
                  const totalVotes = p.yes_votes + p.no_votes;
                  const yesPct = totalVotes > 0 ? (p.yes_votes / totalVotes) * 100 : 0;
                  const isActive = p.status === "active" || p.status === "Active" || p.status === "voting";
                  const blocksLeft = isActive ? Math.max(0, p.voting_end_height - currentHeight) : 0;

                  return (
                    <tr key={p.id}>
                      <td className="mono">{shortAddr(p.id)}</td>
                      <td style={{ fontWeight: 500, maxWidth: 250 }}>{p.title}</td>
                      <td className="mono">{shortAddr(p.proposer)}</td>
                      <td>
                        <span className={`badge ${
                          p.status === "passed" || p.status === "Passed" ? "badge-green" :
                          p.status === "rejected" || p.status === "Rejected" || p.status === "failed" ? "badge-red" :
                          isActive ? "badge-blue" : "badge-yellow"
                        }`}>
                          {p.status}
                        </span>
                      </td>
                      <td className="mono" style={{ textAlign: "right" }}>{p.yes_votes.toLocaleString()}</td>
                      <td className="mono" style={{ textAlign: "right" }}>{p.no_votes.toLocaleString()}</td>
                      <td style={{ minWidth: 140 }}>
                        {totalVotes > 0 ? (
                          <div>
                            <div style={{
                              display: "flex",
                              height: 6,
                              borderRadius: 3,
                              overflow: "hidden",
                              background: "var(--border)",
                            }}>
                              <div style={{ width: `${yesPct}%`, background: "var(--green)" }} />
                              <div style={{ width: `${100 - yesPct}%`, background: "var(--red)" }} />
                            </div>
                            <div style={{ fontSize: 11, color: "var(--text-dim)", marginTop: 2 }}>
                              {yesPct.toFixed(0)}% yes
                              {isActive && ` | ${blocksLeft} blocks left`}
                            </div>
                          </div>
                        ) : (
                          <span style={{ fontSize: 12, color: "var(--text-dim)" }}>No votes</span>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </>
  );
}
