import { BrowserRouter, Routes, Route } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import Layout from "./components/Layout";
import Dashboard from "./pages/Dashboard";
import Validators from "./pages/Validators";
import Network from "./pages/Network";
import Governance from "./pages/Governance";
import Deploy from "./pages/Deploy";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      staleTime: 2000,
      throwOnError: false,
    },
  },
});

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <Routes>
          <Route element={<Layout />}>
            <Route index element={<Dashboard />} />
            <Route path="validators" element={<Validators />} />
            <Route path="network" element={<Network />} />
            <Route path="governance" element={<Governance />} />
            <Route path="deploy" element={<Deploy />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </QueryClientProvider>
  );
}
