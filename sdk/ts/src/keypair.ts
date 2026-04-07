import * as ed from "@noble/ed25519";
import { sha512 } from "@noble/hashes/sha512";
import { sha256 } from "@noble/hashes/sha256";

// noble/ed25519 v2 requires setting the sha512 hash function.
ed.etc.sha512Sync = (...m: Uint8Array[]) => {
  const h = sha512.create();
  for (const msg of m) h.update(msg);
  return h.digest();
};

/**
 * An Ed25519 keypair for the POLAY blockchain.
 *
 * Secret keys are 32-byte Ed25519 seeds. Public keys are the corresponding
 * 32-byte Ed25519 public keys. The POLAY address is the SHA-256 hash of the
 * public key, hex-encoded (64 characters).
 */
export class PolayKeypair {
  private readonly _secretKey: Uint8Array;
  private readonly _publicKey: Uint8Array;

  private constructor(secretKey: Uint8Array) {
    if (secretKey.length !== 32) {
      throw new Error(
        `Invalid secret key length: expected 32 bytes, got ${secretKey.length}`,
      );
    }
    this._secretKey = new Uint8Array(secretKey);
    this._publicKey = ed.getPublicKey(this._secretKey);
  }

  /**
   * Generate a new random keypair.
   *
   * Uses crypto.getRandomValues for the 32-byte seed.
   */
  static generate(): PolayKeypair {
    const seed = ed.utils.randomPrivateKey();
    return new PolayKeypair(seed);
  }

  /**
   * Restore a keypair from an existing 32-byte Ed25519 secret key (seed).
   */
  static fromSecretKey(secretKey: Uint8Array): PolayKeypair {
    return new PolayKeypair(secretKey);
  }

  /**
   * Restore a keypair from a hex-encoded 32-byte secret key.
   */
  static fromSecretKeyHex(hex: string): PolayKeypair {
    return new PolayKeypair(hexToBytes(hex));
  }

  /** The raw 32-byte Ed25519 public key. */
  get publicKey(): Uint8Array {
    return new Uint8Array(this._publicKey);
  }

  /** The raw 32-byte Ed25519 secret key (seed). */
  get secretKey(): Uint8Array {
    return new Uint8Array(this._secretKey);
  }

  /**
   * The POLAY address: SHA-256 of the public key, hex-encoded.
   *
   * This matches the Rust `Address` type which is a 32-byte hash of the
   * Ed25519 public key.
   */
  get address(): string {
    const hash = sha256(this._publicKey);
    return bytesToHex(hash);
  }

  /**
   * Sign an arbitrary message using Ed25519.
   *
   * Returns the 64-byte signature.
   */
  async sign(message: Uint8Array): Promise<Uint8Array> {
    return ed.signAsync(message, this._secretKey);
  }

  /**
   * Synchronous signing variant.
   */
  signSync(message: Uint8Array): Uint8Array {
    return ed.sign(message, this._secretKey);
  }
}

// ---------------------------------------------------------------------------
// Hex utilities
// ---------------------------------------------------------------------------

/** Convert a byte array to a lowercase hex string. */
export function bytesToHex(bytes: Uint8Array): string {
  let hex = "";
  for (let i = 0; i < bytes.length; i++) {
    hex += bytes[i].toString(16).padStart(2, "0");
  }
  return hex;
}

/** Convert a hex string to a Uint8Array. */
export function hexToBytes(hex: string): Uint8Array {
  if (hex.length % 2 !== 0) {
    throw new Error("Hex string must have an even number of characters");
  }
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = parseInt(hex.substring(i * 2, i * 2 + 2), 16);
  }
  return bytes;
}
