import { type ReactNode, useState, type FormEvent } from "react";
import { Link, useNavigate } from "react-router-dom";

export default function Layout({ children }: { children: ReactNode }) {
  const [search, setSearch] = useState("");
  const navigate = useNavigate();

  function handleSearch(e: FormEvent) {
    e.preventDefault();
    const q = search.trim();
    if (!q) return;

    if (/^\d+$/.test(q)) {
      // Numeric -> block height
      navigate(`/block/${q}`);
    } else if (q.length === 64 && /^[0-9a-fA-F]+$/.test(q)) {
      // 64 hex chars -> could be tx hash or address; try tx first
      navigate(`/tx/${q}`);
    } else if (q.length >= 40 && /^[0-9a-fA-F]+$/.test(q)) {
      // Hex -> treat as address
      navigate(`/account/${q}`);
    } else {
      // Fallback: try as tx hash
      navigate(`/tx/${q}`);
    }
    setSearch("");
  }

  return (
    <div className="app-layout">
      <header className="app-header">
        <div className="header-inner">
          <Link to="/" className="logo">
            <span className="logo-icon">&#x26D3;</span>
            <span className="logo-text">POLAY</span>
            <span className="logo-sub">Explorer</span>
          </Link>

          <form className="search-form" onSubmit={handleSearch}>
            <input
              className="search-input"
              type="text"
              placeholder="Search by block height, tx hash, or address..."
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
            <button className="search-btn" type="submit">
              Search
            </button>
          </form>

          <nav className="nav-links">
            <Link to="/" className="nav-link">Dashboard</Link>
          </nav>
        </div>
      </header>

      <main className="app-main">{children}</main>

      <footer className="app-footer">
        <span>POLAY Block Explorer &mdash; Testnet</span>
      </footer>
    </div>
  );
}
