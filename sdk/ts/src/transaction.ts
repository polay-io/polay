import { sha256 } from "@noble/hashes/sha256";

import { PolayKeypair, bytesToHex } from "./keypair.js";
import type { Transaction, TransactionAction, SignedTransaction } from "./types.js";

// ---------------------------------------------------------------------------
// Canonical JSON serialization
// ---------------------------------------------------------------------------

/**
 * Produce a deterministic JSON string for a value.
 *
 * Object keys are sorted lexicographically at every level so that the byte
 * representation is fully deterministic -- a requirement for signing.
 */
function canonicalJson(value: unknown): string {
  if (value === null || value === undefined) {
    return "null";
  }
  if (typeof value === "boolean" || typeof value === "number") {
    return JSON.stringify(value);
  }
  if (typeof value === "string") {
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) {
    const items = value.map((v) => canonicalJson(v));
    return "[" + items.join(",") + "]";
  }
  if (typeof value === "object") {
    const keys = Object.keys(value as Record<string, unknown>).sort();
    const entries = keys.map(
      (k) => JSON.stringify(k) + ":" + canonicalJson((value as Record<string, unknown>)[k]),
    );
    return "{" + entries.join(",") + "}";
  }
  throw new Error(`canonicalJson: unsupported type ${typeof value}`);
}

/**
 * Serialize a transaction to its canonical signing bytes.
 *
 * The signing payload is the SHA-256 hash of the deterministic JSON encoding
 * of the transaction.
 */
export function transactionSigningBytes(tx: Transaction): Uint8Array {
  const json = canonicalJson(tx);
  return new TextEncoder().encode(json);
}

// ---------------------------------------------------------------------------
// TransactionBuilder
// ---------------------------------------------------------------------------

/**
 * Builds unsigned POLAY transactions.
 *
 * ```ts
 * const builder = new TransactionBuilder("polay-devnet-1");
 * const tx = builder.build({
 *   signer: keypair.address,
 *   nonce: 0,
 *   action: { type: "Transfer", to: recipient, amount: "1000000" },
 * });
 * ```
 */
export class TransactionBuilder {
  private readonly chainId: string;

  constructor(chainId: string = "polay-devnet-1") {
    this.chainId = chainId;
  }

  /**
   * Build an unsigned transaction.
   *
   * @param params.signer  Hex-encoded address of the transaction signer.
   * @param params.nonce   The signer's current nonce.
   * @param params.action  The on-chain operation to perform.
   * @param params.maxFee  Maximum fee in native tokens (default: "1000").
   */
  build(params: {
    signer: string;
    nonce: number;
    action: TransactionAction;
    maxFee?: string;
    session?: string;
    sponsor?: string;
  }): Transaction {
    const tx: Transaction = {
      chain_id: this.chainId,
      nonce: params.nonce,
      signer: params.signer,
      action: params.action,
      max_fee: params.maxFee ?? "1000",
      timestamp: Math.floor(Date.now() / 1000),
    };
    if (params.session) {
      tx.session = params.session;
    }
    if (params.sponsor) {
      tx.sponsor = params.sponsor;
    }
    return tx;
  }

  /**
   * Sign a transaction with the given keypair.
   *
   * 1. Serialize the transaction to deterministic JSON bytes.
   * 2. SHA-256 hash those bytes to produce the signing message.
   * 3. Ed25519 sign the hash with the keypair.
   * 4. Compute the tx_hash as SHA-256(signature || signing_message).
   */
  static async sign(
    tx: Transaction,
    keypair: PolayKeypair,
  ): Promise<SignedTransaction> {
    const msgBytes = transactionSigningBytes(tx);
    const msgHash = sha256(msgBytes);

    // Ed25519 signature over the hash.
    const sigBytes = await keypair.sign(msgHash);
    const signature = bytesToHex(sigBytes);

    // Transaction hash: SHA-256(signature_bytes || message_hash).
    const txHashInput = new Uint8Array(sigBytes.length + msgHash.length);
    txHashInput.set(sigBytes, 0);
    txHashInput.set(msgHash, sigBytes.length);
    const txHash = bytesToHex(sha256(txHashInput));

    return {
      transaction: tx,
      signature,
      tx_hash: txHash,
      signer_pubkey: bytesToHex(keypair.publicKey),
    };
  }

  /**
   * Synchronous signing variant.
   */
  static signSync(
    tx: Transaction,
    keypair: PolayKeypair,
  ): SignedTransaction {
    const msgBytes = transactionSigningBytes(tx);
    const msgHash = sha256(msgBytes);

    const sigBytes = keypair.signSync(msgHash);
    const signature = bytesToHex(sigBytes);

    const txHashInput = new Uint8Array(sigBytes.length + msgHash.length);
    txHashInput.set(sigBytes, 0);
    txHashInput.set(msgHash, sigBytes.length);
    const txHash = bytesToHex(sha256(txHashInput));

    return {
      transaction: tx,
      signature,
      tx_hash: txHash,
      signer_pubkey: bytesToHex(keypair.publicKey),
    };
  }
}
