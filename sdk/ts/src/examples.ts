import { PolayClient } from "./client.js";
import { PolayKeypair } from "./keypair.js";
import { TransactionBuilder } from "./transaction.js";
import type { TransactionAction } from "./types.js";

// ---------------------------------------------------------------------------
// Helpers shared across examples
// ---------------------------------------------------------------------------

/**
 * Build, sign, and submit a transaction in one step.
 * Returns the transaction hash.
 */
async function sendAction(
  client: PolayClient,
  builder: TransactionBuilder,
  keypair: PolayKeypair,
  nonce: number,
  action: TransactionAction,
): Promise<string> {
  const tx = builder.build({
    signer: keypair.address,
    nonce,
    action,
  });
  const signed = await TransactionBuilder.sign(tx, keypair);
  return client.submitTransaction(signed);
}

// ---------------------------------------------------------------------------
// Example 1: Token lifecycle
// ---------------------------------------------------------------------------

/**
 * Demonstrates a basic token lifecycle on POLAY devnet:
 *
 * 1. Check the signer's balance.
 * 2. Transfer native tokens to a newly generated recipient.
 * 3. Check the recipient's balance.
 *
 * Assumes the `keypair` account is funded (e.g. via genesis or faucet).
 */
export async function exampleTokenLifecycle(
  client: PolayClient,
  keypair: PolayKeypair,
): Promise<void> {
  const builder = new TransactionBuilder();
  const recipient = PolayKeypair.generate();

  // Fetch the current nonce for the signer.
  const account = await client.getAccount(keypair.address);
  let nonce = account?.nonce ?? 0;

  console.log(`Signer address : ${keypair.address}`);
  console.log(`Recipient addr : ${recipient.address}`);
  console.log(`Signer balance : ${account?.balance ?? "0"}`);

  // Transfer 1,000,000 native tokens.
  const txHash = await sendAction(client, builder, keypair, nonce++, {
    type: "Transfer",
    to: recipient.address,
    amount: "1000000",
  });
  console.log(`Transfer tx     : ${txHash}`);

  // Query balances (may need to wait for the block to be produced).
  const recipientBalance = await client.getBalance(recipient.address);
  console.log(`Recipient bal   : ${recipientBalance}`);
}

// ---------------------------------------------------------------------------
// Example 2: Marketplace flow
// ---------------------------------------------------------------------------

/**
 * End-to-end marketplace flow:
 *
 * 1. Create a fungible asset class ("Gold Coins").
 * 2. Mint 100 units to the creator.
 * 3. List 10 units for sale on the marketplace.
 * 4. A buyer purchases the listing.
 *
 * Assumes both `keypair` (seller/creator) and a second generated keypair
 * (buyer) have funds.
 */
export async function exampleMarketplaceFlow(
  client: PolayClient,
  keypair: PolayKeypair,
): Promise<void> {
  const builder = new TransactionBuilder();

  // Fetch signer nonce.
  const account = await client.getAccount(keypair.address);
  let nonce = account?.nonce ?? 0;

  // Step 1: Create an asset class.
  const createAssetHash = await sendAction(client, builder, keypair, nonce++, {
    type: "CreateAssetClass",
    name: "Gold Coins",
    symbol: "GLD",
    asset_type: "Fungible",
    max_supply: "1000000",
    metadata_uri: "https://polay.io/assets/gold-coins.json",
  });
  console.log(`CreateAssetClass tx : ${createAssetHash}`);

  // The asset_class_id is typically the tx hash of the creation transaction.
  // In practice you would read this from the transaction receipt events.
  const assetClassId = createAssetHash;

  // Step 2: Mint 100 units to ourselves.
  const mintHash = await sendAction(client, builder, keypair, nonce++, {
    type: "MintAsset",
    asset_class_id: assetClassId,
    to: keypair.address,
    amount: "100",
    metadata: null,
  });
  console.log(`MintAsset tx        : ${mintHash}`);

  // Step 3: List 10 units for sale.
  // Use a zero-hash as the native token currency.
  const nativeCurrency = "0".repeat(64);
  const listHash = await sendAction(client, builder, keypair, nonce++, {
    type: "CreateListing",
    asset_class_id: assetClassId,
    amount: "10",
    price_per_unit: "500",
    currency: nativeCurrency,
  });
  console.log(`CreateListing tx    : ${listHash}`);

  // The listing_id is typically the tx hash of the listing transaction.
  const listingId = listHash;

  // Step 4: Buyer purchases the listing.
  const buyer = PolayKeypair.generate();
  console.log(`Buyer address       : ${buyer.address}`);

  // Fund the buyer first (transfer from creator).
  const fundHash = await sendAction(client, builder, keypair, nonce++, {
    type: "Transfer",
    to: buyer.address,
    amount: "10000",
  });
  console.log(`Fund buyer tx       : ${fundHash}`);

  // Buyer purchases.
  const buyerAccount = await client.getAccount(buyer.address);
  const buyerNonce = buyerAccount?.nonce ?? 0;
  const buyHash = await sendAction(client, builder, buyer, buyerNonce, {
    type: "BuyListing",
    listing_id: listingId,
  });
  console.log(`BuyListing tx       : ${buyHash}`);
}

// ---------------------------------------------------------------------------
// Example 3: Attestation flow
// ---------------------------------------------------------------------------

/**
 * Game attestation flow:
 *
 * 1. Register an attestor for a game.
 * 2. Submit a match result.
 * 3. Distribute rewards to match winners.
 *
 * Assumes the `keypair` account has authority to attest and distribute.
 */
export async function exampleAttestationFlow(
  client: PolayClient,
  keypair: PolayKeypair,
): Promise<void> {
  const builder = new TransactionBuilder();

  const account = await client.getAccount(keypair.address);
  let nonce = account?.nonce ?? 0;

  const gameId = "arena-battle-v1";

  // Step 1: Register as an attestor.
  const registerHash = await sendAction(client, builder, keypair, nonce++, {
    type: "RegisterAttestor",
    game_id: gameId,
    endpoint: "https://attestor.mygame.io/v1",
    metadata: JSON.stringify({
      version: "1.0.0",
      supported_modes: ["1v1", "team-deathmatch"],
    }),
  });
  console.log(`RegisterAttestor tx  : ${registerHash}`);

  // Create two players for the match.
  const player1 = PolayKeypair.generate();
  const player2 = PolayKeypair.generate();

  // Generate a deterministic match ID (in production this comes from the game server).
  const matchId = "a".repeat(64);

  // Step 2: Submit a match result.
  const submitHash = await sendAction(client, builder, keypair, nonce++, {
    type: "SubmitMatchResult",
    match_result: {
      match_id: matchId,
      game_id: gameId,
      timestamp: Math.floor(Date.now() / 1000),
      players: [player1.address, player2.address],
      scores: [1500, 1200],
      winners: [player1.address],
      reward_pool: "5000",
      server_signature: [],
      anti_cheat_score: 98,
      replay_ref: "ipfs://QmExampleReplayHash",
    },
  });
  console.log(`SubmitMatchResult tx : ${submitHash}`);

  // Step 3: Distribute rewards.
  const distributeHash = await sendAction(client, builder, keypair, nonce++, {
    type: "DistributeReward",
    match_id: matchId,
    rewards: [
      [player1.address, "3500"],
      [player2.address, "1500"],
    ],
  });
  console.log(`DistributeReward tx  : ${distributeHash}`);

  // Verify the match result was recorded.
  const result = await client.getMatchResult(matchId);
  if (result) {
    console.log(`Match recorded       : ${result.game_id}, winners: ${result.winners.length}`);
  }
}

// ---------------------------------------------------------------------------
// Example 4: Player identity and staking
// ---------------------------------------------------------------------------

/**
 * Player identity and staking flow:
 *
 * 1. Create a player profile.
 * 2. Register as a validator.
 * 3. Self-delegate stake.
 *
 * Assumes the `keypair` account has sufficient balance.
 */
export async function exampleIdentityAndStaking(
  client: PolayClient,
  keypair: PolayKeypair,
): Promise<void> {
  const builder = new TransactionBuilder();

  const account = await client.getAccount(keypair.address);
  let nonce = account?.nonce ?? 0;

  // Step 1: Create a profile.
  const profileHash = await sendAction(client, builder, keypair, nonce++, {
    type: "CreateProfile",
    username: "crypto_gamer_42",
    display_name: "Crypto Gamer",
    metadata: JSON.stringify({
      avatar: "https://polay.io/avatars/42.png",
      bio: "Competitive gamer and validator operator",
    }),
  });
  console.log(`CreateProfile tx      : ${profileHash}`);

  // Step 2: Register as a validator (5% commission = 500 bps).
  const registerHash = await sendAction(client, builder, keypair, nonce++, {
    type: "RegisterValidator",
    commission_bps: 500,
  });
  console.log(`RegisterValidator tx  : ${registerHash}`);

  // Step 3: Self-delegate 100,000 tokens.
  const delegateHash = await sendAction(client, builder, keypair, nonce++, {
    type: "DelegateStake",
    validator: keypair.address,
    amount: "100000",
  });
  console.log(`DelegateStake tx      : ${delegateHash}`);

  // Query the validator info.
  const validator = await client.getValidator(keypair.address);
  if (validator) {
    console.log(`Validator stake       : ${validator.stake}`);
    console.log(`Validator status      : ${validator.status}`);
  }

  // Query the profile.
  const profile = await client.getProfile(keypair.address);
  if (profile) {
    console.log(`Profile username      : ${profile.username}`);
    console.log(`Profile reputation    : ${profile.reputation}`);
  }
}
