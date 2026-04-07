# Incident Response Runbook

This document describes procedures for responding to operational incidents on the POLAY mainnet.

## Severity Levels

| Level | Name | Description | Response Time |
|-------|------|-------------|---------------|
| P0 | Critical | Chain halted, consensus failure, funds at risk | Immediate (< 15 min) |
| P1 | High | Degraded performance, validator down, missed blocks | < 1 hour |
| P2 | Medium | Monitoring alerts, non-critical service down | < 4 hours |
| P3 | Low | Cosmetic issues, documentation gaps | Next business day |

## Communication Channels

- **On-call rotation**: Check the on-call schedule for the current responder
- **Incident channel**: Create a dedicated channel per incident
- **Status page**: Update the public status page within the response time SLA
- **Post-mortem**: Required for all P0 and P1 incidents

---

## P0: Chain Halt

### Symptoms
- Block height stops advancing for > 5 minutes
- Prometheus alert: `HighBlockLatency` firing
- Multiple validators report consensus failure

### Diagnosis

```bash
# Check block height across validators
for port in 9944 9945 9946 9947; do
  curl -s http://validator-$port/rpc -d '{"jsonrpc":"2.0","method":"chain_getLatestBlock","params":[],"id":1}' | jq '.result.height'
done

# Check consensus state
curl -s http://localhost:9944/rpc -d '{"jsonrpc":"2.0","method":"node_info","params":[],"id":1}' | jq .

# Check validator connectivity
curl -s http://localhost:9944/rpc -d '{"jsonrpc":"2.0","method":"net_peers","params":[],"id":1}' | jq '.result | length'
```

### Recovery

1. **Identify how many validators are down.** If < 1/3 are down, the remaining validators should continue producing blocks. Wait for the down validators to recover.

2. **If > 1/3 are down**, consensus has stalled. Coordinate with validator operators:
   ```bash
   # On each validator, check logs for errors
   journalctl -u polay-validator --since "10 minutes ago" | grep -i error
   ```

3. **If caused by a software bug**, prepare an emergency patch:
   ```bash
   git checkout -b hotfix/chain-halt-YYYYMMDD
   # Apply fix
   cargo test --workspace
   cargo build --release
   ```

4. **Coordinate restart** with at least 2/3+1 of validators simultaneously.

5. **Post-incident**: Write post-mortem within 24 hours.

---

## P0: Suspected Exploit / Funds at Risk

### Symptoms
- Unexpected large transfers or token minting
- Balance discrepancies in explorer
- Reports of unauthorized transactions

### Immediate Actions

1. **Do NOT shut down validators** unless actively losing funds. The chain log is evidence.
2. **Capture state snapshot** immediately:
   ```bash
   # Snapshot the state directory
   cp -r /var/lib/polay/state /var/lib/polay/state-snapshot-$(date +%s)
   ```
3. **Identify the suspicious transaction(s)**:
   ```bash
   curl -s http://localhost:9944/rpc -d '{
     "jsonrpc":"2.0","method":"chain_getBlock",
     "params":[BLOCK_HEIGHT],"id":1
   }' | jq '.result.transactions'
   ```
4. **If the exploit is ongoing**, coordinate an emergency validator halt (requires 2/3+1 agreement).
5. **Engage security team** for forensic analysis.

---

## P1: Validator Down

### Symptoms
- Prometheus alert: `ValidatorDown` or `NodeDown`
- Missing blocks from a specific proposer
- Peer count dropped

### Diagnosis

```bash
# Check if process is running
systemctl status polay-validator

# Check resource usage
top -p $(pgrep polay)
df -h /var/lib/polay

# Check logs
journalctl -u polay-validator --since "30 minutes ago" -n 100

# Check if the node is syncing
curl -s http://localhost:9944/rpc -d '{"jsonrpc":"2.0","method":"chain_getLatestBlock","params":[],"id":1}'
```

### Recovery

1. **Out of memory**: Increase memory limits, restart.
2. **Disk full**: Prune old data or expand disk, restart.
3. **Corrupted state**: Stop validator, remove state directory, resync from peers.
   ```bash
   systemctl stop polay-validator
   rm -rf /var/lib/polay/state
   systemctl start polay-validator
   # Node will resync from genesis via P2P
   ```
4. **Network partition**: Check firewall rules, verify P2P port (30333) is accessible.

---

## P1: Consensus Fork

### Symptoms
- Different validators report different block hashes at the same height
- Explorer shows conflicting data

### Diagnosis

```bash
# Compare block hashes across validators
for i in 1 2 3 4; do
  echo "Validator $i:"
  curl -s http://validator-$i:9944/rpc -d '{"jsonrpc":"2.0","method":"chain_getBlock","params":[HEIGHT],"id":1}' | jq '.result.header.hash'
done
```

### Recovery

1. Identify which fork has > 2/3 stake backing.
2. Validators on the minority fork should stop, wipe state, and resync.
3. Investigate root cause (likely equivocation or software bug).

---

## P2: High Mempool Backlog

### Symptoms
- Prometheus alert: `MempoolBacklog` > 5000 for > 5 minutes
- Transaction confirmation times increasing

### Diagnosis

```bash
curl -s http://localhost:9944/rpc -d '{"jsonrpc":"2.0","method":"mempool_status","params":[],"id":1}' | jq .
```

### Mitigation

1. **If caused by spam**: Per-IP rate limiting should handle this. Check rate limiter stats.
2. **If caused by legitimate load**: Consider temporarily increasing `max_block_transactions`.
3. **If mempool is full of stale txs**: The TTL eviction (300s) should clear them automatically. If not:
   ```bash
   curl -s http://localhost:9944/rpc -d '{"jsonrpc":"2.0","method":"mempool_flush","params":[],"id":1}'
   ```

---

## P2: Monitoring Stack Down

### Symptoms
- No metrics in Grafana
- Prometheus targets showing as DOWN

### Recovery

```bash
# Check Prometheus
docker logs polay-mainnet-prometheus --tail 50

# Check metrics exporter
docker logs polay-mainnet-metrics --tail 50

# Restart monitoring stack
docker compose -f docker-compose.mainnet.yml restart prometheus metrics-exporter grafana
```

---

## Operational Procedures

### Validator Key Rotation

```bash
# 1. Generate new key
polay-wallet keygen --output new-validator.key

# 2. Register the new key on-chain (governance proposal or direct tx)
# 3. Wait for the epoch boundary
# 4. Update the validator config to use the new key
# 5. Restart the validator
```

### Emergency Chain Halt (Nuclear Option)

**Only use when funds are actively at risk and cannot be stopped otherwise.**

1. Coordinate with > 2/3 of validator operators
2. All participating validators execute: `systemctl stop polay-validator`
3. The chain halts within one block time
4. Apply fix, verify, coordinate restart

### Upgrading Validator Software

```bash
# 1. Build new release
git pull origin main
cargo build --release

# 2. Stop validator (will miss ~1-2 blocks)
systemctl stop polay-validator

# 3. Replace binary
cp target/release/polay /usr/local/bin/polay

# 4. Restart
systemctl start polay-validator

# 5. Verify
curl -s http://localhost:9944/rpc -d '{"jsonrpc":"2.0","method":"node_info","params":[],"id":1}' | jq .
```

---

## Post-Mortem Template

```markdown
# Incident Post-Mortem: [Title]

**Date:** YYYY-MM-DD
**Severity:** P0/P1/P2/P3
**Duration:** X hours Y minutes
**Author:** [Name]

## Summary
One paragraph describing what happened.

## Timeline
- HH:MM — First alert fired
- HH:MM — Responder acknowledged
- HH:MM — Root cause identified
- HH:MM — Fix deployed
- HH:MM — Service restored

## Root Cause
Technical description of the underlying issue.

## Impact
- Blocks missed: N
- Users affected: N
- Funds lost: 0

## Resolution
What was done to fix it.

## Action Items
- [ ] Immediate fix applied
- [ ] Monitoring improved
- [ ] Root cause permanently addressed
- [ ] Documentation updated
```
