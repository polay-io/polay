import { NavLink, Outlet } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { fetchHealth } from "../api";

export default function Layout() {
  const health = useQuery({
    queryKey: ["health"],
    queryFn: fetchHealth,
    refetchInterval: 5000,
    retry: 2,
    throwOnError: false,
  });

  const isHealthy = health.data?.status === "ok" || health.data?.status === "healthy";

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="sidebar-logo">
          <h1>POLAY</h1>
          <span>Admin Panel</span>
        </div>
        <nav>
          <NavLink to="/" end>
            <span className="icon">&#9632;</span>
            <span>Dashboard</span>
          </NavLink>
          <NavLink to="/validators">
            <span className="icon">&#9733;</span>
            <span>Validators</span>
          </NavLink>
          <NavLink to="/network">
            <span className="icon">&#9778;</span>
            <span>Network</span>
          </NavLink>
          <NavLink to="/governance">
            <span className="icon">&#9878;</span>
            <span>Governance</span>
          </NavLink>
          <NavLink to="/deploy">
            <span className="icon">&#9729;</span>
            <span>Deploy</span>
          </NavLink>
        </nav>
        <div className="sidebar-footer">
          <span className={`status-dot ${isHealthy ? "green" : "red"}`} />
          {isHealthy ? "Node Online" : "Node Offline"}
          {health.data?.height != null && (
            <div style={{ marginTop: 4 }}>
              Block #{health.data.height}
            </div>
          )}
        </div>
      </aside>
      <main className="main">
        <Outlet />
      </main>
    </div>
  );
}
