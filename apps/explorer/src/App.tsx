import { Routes, Route } from "react-router-dom";
import Layout from "./components/Layout";
import Dashboard from "./pages/Dashboard";
import BlockDetail from "./pages/BlockDetail";
import TxDetail from "./pages/TxDetail";
import AccountDetail from "./pages/AccountDetail";

export default function App() {
  return (
    <Layout>
      <Routes>
        <Route path="/" element={<Dashboard />} />
        <Route path="/block/:height" element={<BlockDetail />} />
        <Route path="/tx/:hash" element={<TxDetail />} />
        <Route path="/account/:address" element={<AccountDetail />} />
      </Routes>
    </Layout>
  );
}
