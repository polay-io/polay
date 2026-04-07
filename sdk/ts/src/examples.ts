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

// ---------------------------------------------------------------------------
// Example 5: Guild flow
// ---------------------------------------------------------------------------

/**
 * Guild management flow:
 *
 * 1. Create a guild.
 * 2. Invite a member (they join).
 * 3. Deposit to the guild treasury.
 * 4. Promote the member to officer.
 *
 * Assumes both keypairs have funds.
 */
export async function exampleGuildFlow(
  client: PolayClient,
  keypair: PolayKeypair,
): Promise<void> {
  const builder = new TransactionBuilder();

  const account = await client.getAccount(keypair.address);
  let nonce = account?.nonce ?? 0;

  // Step 1: Create a guild.
  const createHash = await sendAction(client, builder, keypair, nonce++, {
    type: "CreateGuild",
    name: "Dragon Slayers",
    description: "Elite PvP guild",
    max_members: 50,
  });
  console.log(`CreateGuild tx       : ${createHash}`);
  const guildId = createHash; // guild ID is typically derived from tx

  // Step 2: A new member joins.
  const member = PolayKeypair.generate();

  // Fund the member first.
  await sendAction(client, builder, keypair, nonce++, {
    type: "Transfer",
    to: member.address,
    amount: "100000",
  });

  const memberAccount = await client.getAccount(member.address);
  const memberNonce = memberAccount?.nonce ?? 0;
  const joinHash = await sendAction(client, builder, member, memberNonce, {
    type: "JoinGuild",
    guild_id: guildId,
  });
  console.log(`JoinGuild tx         : ${joinHash}`);

  // Step 3: Leader deposits to guild treasury.
  const depositHash = await sendAction(client, builder, keypair, nonce++, {
    type: "GuildDeposit",
    guild_id: guildId,
    amount: "50000",
  });
  console.log(`GuildDeposit tx      : ${depositHash}`);

  // Step 4: Promote member to officer.
  const promoteHash = await sendAction(client, builder, keypair, nonce++, {
    type: "GuildPromote",
    guild_id: guildId,
    member: member.address,
    role: "Officer",
  });
  console.log(`GuildPromote tx      : ${promoteHash}`);
}

// ---------------------------------------------------------------------------
// Example 6: Tournament flow
// ---------------------------------------------------------------------------

/**
 * Tournament lifecycle:
 *
 * 1. Create a tournament with entry fee and prize distribution.
 * 2. Players join the tournament.
 * 3. Organizer starts the tournament.
 * 4. Report results and distribute prizes.
 *
 * Assumes organizer keypair has funds.
 */
export async function exampleTournamentFlow(
  client: PolayClient,
  keypair: PolayKeypair,
): Promise<void> {
  const builder = new TransactionBuilder();

  const account = await client.getAccount(keypair.address);
  let nonce = account?.nonce ?? 0;

  // Step 1: Create tournament (70% first, 20% second, 10% third).
  const createHash = await sendAction(client, builder, keypair, nonce++, {
    type: "CreateTournament",
    name: "Arena Championship",
    game_id: "arena-battle-v1",
    entry_fee: "1000",
    max_participants: 32,
    min_participants: 4,
    start_height: 9999999,
    prize_distribution: [7000, 2000, 1000],
  });
  console.log(`CreateTournament tx  : ${createHash}`);
  const tournamentId = createHash;

  // Step 2: Players join.
  const players = [PolayKeypair.generate(), PolayKeypair.generate()];
  for (const player of players) {
    // Fund the player.
    await sendAction(client, builder, keypair, nonce++, {
      type: "Transfer",
      to: player.address,
      amount: "10000",
    });

    const pAccount = await client.getAccount(player.address);
    const pNonce = pAccount?.nonce ?? 0;
    const joinHash = await sendAction(client, builder, player, pNonce, {
      type: "JoinTournament",
      tournament_id: tournamentId,
    });
    console.log(`JoinTournament tx    : ${joinHash}`);
  }

  // Step 3: Organizer starts the tournament.
  const startHash = await sendAction(client, builder, keypair, nonce++, {
    type: "StartTournament",
    tournament_id: tournamentId,
  });
  console.log(`StartTournament tx   : ${startHash}`);

  // Step 4: Report results (player 0 wins, player 1 second).
  const reportHash = await sendAction(client, builder, keypair, nonce++, {
    type: "ReportTournamentResults",
    tournament_id: tournamentId,
    rankings: players.map((p) => p.address),
  });
  console.log(`ReportResults tx     : ${reportHash}`);

  // Step 5: Winners claim prizes.
  for (const player of players) {
    const pAccount = await client.getAccount(player.address);
    const pNonce = pAccount?.nonce ?? 0;
    const claimHash = await sendAction(client, builder, player, pNonce + 1, {
      type: "ClaimTournamentPrize",
      tournament_id: tournamentId,
    });
    console.log(`ClaimPrize tx        : ${claimHash}`);
  }
}

// ---------------------------------------------------------------------------
// Example 7: Asset rental flow
// ---------------------------------------------------------------------------

/**
 * Asset rental flow:
 *
 * 1. Owner creates an asset and lists it for rent.
 * 2. Renter rents the asset.
 * 3. Renter returns the asset.
 *
 * Assumes both keypairs have funds and assets.
 */
export async function exampleRentalFlow(
  client: PolayClient,
  keypair: PolayKeypair,
): Promise<void> {
  const builder = new TransactionBuilder();

  const account = await client.getAccount(keypair.address);
  let nonce = account?.nonce ?? 0;

  // Create an asset class to rent out.
  const createHash = await sendAction(client, builder, keypair, nonce++, {
    type: "CreateAssetClass",
    name: "Rare Mount",
    symbol: "RMNT",
    asset_type: "NonFungible",
    max_supply: "100",
    metadata_uri: "https://polay.io/assets/rare-mount.json",
  });
  const assetClassId = createHash;

  // Mint one.
  await sendAction(client, builder, keypair, nonce++, {
    type: "MintAsset",
    asset_class_id: assetClassId,
    to: keypair.address,
    amount: "1",
    metadata: null,
  });

  // Step 1: List for rent.
  const assetId = "0".repeat(64); // first minted instance
  const listHash = await sendAction(client, builder, keypair, nonce++, {
    type: "ListForRent",
    asset_class_id: assetClassId,
    asset_id: assetId,
    price_per_block: "5",
    deposit: "500",
    min_duration: 10,
    max_duration: 1000,
  });
  console.log(`ListForRent tx       : ${listHash}`);
  const rentalId = listHash;

  // Step 2: Renter rents for 100 blocks.
  const renter = PolayKeypair.generate();
  await sendAction(client, builder, keypair, nonce++, {
    type: "Transfer",
    to: renter.address,
    amount: "10000",
  });

  const renterAccount = await client.getAccount(renter.address);
  let renterNonce = renterAccount?.nonce ?? 0;
  const rentHash = await sendAction(client, builder, renter, renterNonce++, {
    type: "RentAsset",
    rental_id: rentalId,
    duration: 100,
  });
  console.log(`RentAsset tx         : ${rentHash}`);

  // Step 3: Renter returns early.
  const returnHash = await sendAction(client, builder, renter, renterNonce++, {
    type: "ReturnRental",
    rental_id: rentalId,
  });
  console.log(`ReturnRental tx      : ${returnHash}`);
}

// ---------------------------------------------------------------------------
// Example 8: Session key flow
// ---------------------------------------------------------------------------

/**
 * Session key delegation for gasless gaming:
 *
 * 1. Create a temporary session key with limited permissions.
 * 2. Use the session key for game transactions.
 * 3. Revoke the session key when done.
 *
 * Assumes the keypair has funds.
 */
export async function exampleSessionKeyFlow(
  client: PolayClient,
  keypair: PolayKeypair,
): Promise<void> {
  const builder = new TransactionBuilder();

  const account = await client.getAccount(keypair.address);
  let nonce = account?.nonce ?? 0;

  // Generate a temporary session keypair.
  const sessionKey = PolayKeypair.generate();
  const sessionPubkeyHex = Buffer.from(sessionKey.publicKey).toString("hex");

  // Step 1: Create session key (valid for 1 hour, 1M spending limit).
  const expiresAt = Math.floor(Date.now() / 1000) + 3600;
  const createHash = await sendAction(client, builder, keypair, nonce++, {
    type: "CreateSession",
    session_pubkey: sessionPubkeyHex,
    permissions: "Gaming",
    expires_at: expiresAt,
    spending_limit: "1000000",
  });
  console.log(`CreateSession tx     : ${createHash}`);

  // Step 2: Use session key for a game action (transfer on behalf of granter).
  const tx = builder.build({
    signer: keypair.address,
    nonce: nonce++,
    action: { type: "Transfer", to: PolayKeypair.generate().address, amount: "100" },
    session: sessionKey.address, // session key signs instead of master key
  });
  const signed = await TransactionBuilder.sign(tx, sessionKey);
  const txHash = await client.submitTransaction(signed);
  console.log(`Session tx           : ${txHash}`);

  // Step 3: Revoke session key.
  const revokeHash = await sendAction(client, builder, keypair, nonce++, {
    type: "RevokeSession",
    session_address: sessionKey.address,
  });
  console.log(`RevokeSession tx     : ${revokeHash}`);

  // Verify revocation.
  const sessions = await client.getActiveSessions(keypair.address);
  console.log(`Active sessions      : ${sessions.length}`);
}
