import { useState } from "react";

interface DeployConfig {
  hcloudToken: string;
  sshKeyName: string;
  serverType: string;
  dockerImage: string;
  regions: string[];
}

const REGIONS = [
  { id: "nbg1", label: "Nuremberg, EU" },
  { id: "ash", label: "Ashburn, US-East" },
  { id: "hil", label: "Hillsboro, US-West" },
  { id: "sin", label: "Singapore, Asia" },
  { id: "fsn1", label: "Falkenstein, EU" },
  { id: "hel1", label: "Helsinki, EU" },
];

const SERVER_TYPES = [
  { id: "cpx21", label: "CPX21 — 3 vCPU, 4GB, 80GB (~$7.50/mo)" },
  { id: "cpx31", label: "CPX31 — 4 vCPU, 8GB, 160GB (~$14/mo)" },
  { id: "cpx41", label: "CPX41 — 8 vCPU, 16GB, 240GB (~$27/mo)" },
];

export default function Deploy() {
  const [config, setConfig] = useState<DeployConfig>({
    hcloudToken: "",
    sshKeyName: "",
    serverType: "cpx21",
    dockerImage: "ghcr.io/polaychain/polay:main",
    regions: ["nbg1", "ash", "hil", "sin"],
  });

  const [step, setStep] = useState(0);
  const [logs, setLogs] = useState<string[]>([]);

  const update = (field: keyof DeployConfig, value: string | string[]) => {
    setConfig((prev) => ({ ...prev, [field]: value }));
  };

  const toggleRegion = (id: string) => {
    setConfig((prev) => ({
      ...prev,
      regions: prev.regions.includes(id)
        ? prev.regions.filter((r) => r !== id)
        : [...prev.regions, id],
    }));
  };

  const canDeploy = config.hcloudToken.length > 10 && config.sshKeyName && config.regions.length >= 1;

  const generateTfvars = () => {
    const validators = config.regions.map((r, i) => `  { name = "validator-${i + 1}", location = "${r}" }`);
    return [
      `hcloud_token = "${config.hcloudToken}"`,
      `ssh_key_name = "${config.sshKeyName}"`,
      `server_type  = "${config.serverType}"`,
      `docker_image = "${config.dockerImage}"`,
      "",
      "validators = [",
      validators.join(",\n"),
      "]",
    ].join("\n");
  };

  const handleDeploy = () => {
    setStep(1);
    setLogs([
      "Step 1: Writing terraform.tfvars...",
      "",
      generateTfvars(),
      "",
      "Step 2: Run these commands in your terminal:",
      "",
      "  cd ~/Projects/polay/deploy/hetzner",
      `  export HCLOUD_TOKEN="${config.hcloudToken.slice(0, 4)}...redacted"`,
      "  terraform init",
      "  terraform apply -auto-approve",
      "",
      "Step 3: After terraform finishes:",
      "",
      "  ./deploy-validators.sh",
      "",
      "Step 4: Check status:",
      "",
      "  ./deploy-validators.sh --status",
    ]);
  };

  const monthlyCost = (() => {
    const prices: Record<string, number> = { cpx21: 7.5, cpx31: 14, cpx41: 27 };
    return (prices[config.serverType] ?? 7.5) * config.regions.length;
  })();

  return (
    <>
      <div className="page-header">
        <h2>Deploy</h2>
        <span style={{ fontSize: 13, color: "var(--text-dim)" }}>
          Hetzner Cloud Multi-Region Deployment
        </span>
      </div>

      <div className="grid grid-2" style={{ marginBottom: 20 }}>
        {/* Configuration */}
        <div className="card">
          <div className="card-header"><h3>Configuration</h3></div>

          <div className="deploy-field">
            <label>Hetzner API Token</label>
            <input
              type="password"
              placeholder="Enter your Hetzner Cloud API token"
              value={config.hcloudToken}
              onChange={(e) => update("hcloudToken", e.target.value)}
            />
          </div>

          <div className="deploy-field">
            <label>SSH Key Name</label>
            <input
              type="text"
              placeholder="Name of SSH key in Hetzner console"
              value={config.sshKeyName}
              onChange={(e) => update("sshKeyName", e.target.value)}
            />
          </div>

          <div className="deploy-field">
            <label>Server Type</label>
            <select
              value={config.serverType}
              onChange={(e) => update("serverType", e.target.value)}
            >
              {SERVER_TYPES.map((t) => (
                <option key={t.id} value={t.id}>{t.label}</option>
              ))}
            </select>
          </div>

          <div className="deploy-field">
            <label>Docker Image</label>
            <input
              type="text"
              value={config.dockerImage}
              onChange={(e) => update("dockerImage", e.target.value)}
            />
          </div>
        </div>

        {/* Regions */}
        <div className="card">
          <div className="card-header">
            <h3>Regions ({config.regions.length} selected)</h3>
            <span style={{ fontSize: 13, color: "var(--accent)" }}>
              ~${monthlyCost.toFixed(0)}/mo
            </span>
          </div>

          {REGIONS.map((r) => (
            <div
              key={r.id}
              onClick={() => toggleRegion(r.id)}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 10,
                padding: "10px 0",
                borderBottom: "1px solid var(--border)",
                cursor: "pointer",
              }}
            >
              <div style={{
                width: 20,
                height: 20,
                borderRadius: 4,
                border: `2px solid ${config.regions.includes(r.id) ? "var(--accent)" : "var(--border)"}`,
                background: config.regions.includes(r.id) ? "var(--accent)" : "transparent",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                fontSize: 12,
                color: "white",
              }}>
                {config.regions.includes(r.id) && "\u2713"}
              </div>
              <div>
                <div style={{ fontSize: 14 }}>{r.label}</div>
                <div style={{ fontSize: 11, color: "var(--text-dim)" }}>{r.id}</div>
              </div>
            </div>
          ))}

          <div style={{ marginTop: 20 }}>
            <button
              className="btn btn-primary"
              disabled={!canDeploy}
              onClick={handleDeploy}
              style={{ width: "100%" }}
            >
              Generate Deploy Commands
            </button>
          </div>
        </div>
      </div>

      {/* Deploy output */}
      {step > 0 && (
        <div className="card">
          <div className="card-header"><h3>Deployment Instructions</h3></div>
          <pre style={{
            background: "var(--bg)",
            padding: 16,
            borderRadius: 6,
            fontSize: 13,
            fontFamily: "var(--font-mono)",
            overflow: "auto",
            lineHeight: 1.6,
            color: "var(--green)",
            whiteSpace: "pre-wrap",
          }}>
            {logs.join("\n")}
          </pre>
        </div>
      )}

      {/* Quick reference */}
      <div className="card" style={{ marginTop: 20 }}>
        <div className="card-header"><h3>Quick Reference</h3></div>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Command</th>
                <th>Description</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td className="mono">./deploy-validators.sh</td>
                <td>Full deploy: upload genesis + keys, start all validators</td>
              </tr>
              <tr>
                <td className="mono">./deploy-validators.sh --status</td>
                <td>Check health of all validators</td>
              </tr>
              <tr>
                <td className="mono">./deploy-validators.sh --restart</td>
                <td>Restart all validator containers</td>
              </tr>
              <tr>
                <td className="mono">./deploy-validators.sh --logs N</td>
                <td>Stream logs from validator N</td>
              </tr>
              <tr>
                <td className="mono">terraform destroy</td>
                <td>Tear down all infrastructure</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </>
  );
}
