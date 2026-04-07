#!/usr/bin/env python3
"""POLAY Blockchain Whitepaper - PDF Generator"""

from reportlab.lib.pagesizes import letter
from reportlab.lib.styles import getSampleStyleSheet, ParagraphStyle
from reportlab.lib.colors import HexColor, black, white
from reportlab.lib.units import inch, mm
from reportlab.lib.enums import TA_CENTER, TA_JUSTIFY, TA_LEFT, TA_RIGHT
from reportlab.platypus import (
    SimpleDocTemplate, Paragraph, Spacer, PageBreak, Table, TableStyle,
    KeepTogether, HRFlowable, ListFlowable, ListItem, Flowable
)
from reportlab.pdfgen import canvas
from reportlab.lib import colors
import os

# ── Colors ──────────────────────────────────────────────
POLAY_DARK    = HexColor("#0D1117")
POLAY_BLUE    = HexColor("#1E3A5F")
POLAY_ACCENT  = HexColor("#3B82F6")
POLAY_LIGHT   = HexColor("#60A5FA")
POLAY_GREEN   = HexColor("#10B981")
POLAY_PURPLE  = HexColor("#8B5CF6")
POLAY_ORANGE  = HexColor("#F59E0B")
POLAY_RED     = HexColor("#EF4444")
POLAY_GRAY    = HexColor("#6B7280")
POLAY_BG      = HexColor("#F8FAFC")
TABLE_HEADER  = HexColor("#1E3A5F")
TABLE_ALT     = HexColor("#EFF6FF")
TABLE_BORDER  = HexColor("#CBD5E1")

OUTPUT_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "POLAY_Whitepaper.pdf")

# ── Styles ──────────────────────────────────────────────
styles = getSampleStyleSheet()

styles.add(ParagraphStyle(
    'WPTitle', parent=styles['Title'],
    fontSize=36, leading=44, textColor=POLAY_BLUE,
    spaceAfter=6, alignment=TA_CENTER, fontName='Helvetica-Bold'
))
styles.add(ParagraphStyle(
    'WPSubtitle', parent=styles['Normal'],
    fontSize=16, leading=22, textColor=POLAY_GRAY,
    spaceAfter=30, alignment=TA_CENTER, fontName='Helvetica'
))
styles.add(ParagraphStyle(
    'WPVersion', parent=styles['Normal'],
    fontSize=11, leading=16, textColor=POLAY_ACCENT,
    spaceAfter=4, alignment=TA_CENTER, fontName='Helvetica-Bold'
))
styles.add(ParagraphStyle(
    'H1', parent=styles['Heading1'],
    fontSize=22, leading=28, textColor=POLAY_BLUE,
    spaceBefore=24, spaceAfter=12, fontName='Helvetica-Bold'
))
styles.add(ParagraphStyle(
    'H2', parent=styles['Heading2'],
    fontSize=16, leading=22, textColor=POLAY_ACCENT,
    spaceBefore=18, spaceAfter=8, fontName='Helvetica-Bold'
))
styles.add(ParagraphStyle(
    'H3', parent=styles['Heading3'],
    fontSize=13, leading=18, textColor=POLAY_BLUE,
    spaceBefore=12, spaceAfter=6, fontName='Helvetica-Bold'
))
styles.add(ParagraphStyle(
    'Body', parent=styles['Normal'],
    fontSize=10.5, leading=16, textColor=black,
    spaceAfter=8, alignment=TA_JUSTIFY, fontName='Helvetica'
))
styles.add(ParagraphStyle(
    'BodyIndent', parent=styles['Normal'],
    fontSize=10.5, leading=16, textColor=black,
    spaceAfter=6, alignment=TA_JUSTIFY, fontName='Helvetica',
    leftIndent=20
))
styles.add(ParagraphStyle(
    'WPBullet', parent=styles['Normal'],
    fontSize=10.5, leading=16, textColor=black,
    spaceAfter=4, fontName='Helvetica', leftIndent=24, firstLineIndent=-12
))
styles.add(ParagraphStyle(
    'WPCode', parent=styles['Normal'],
    fontSize=9, leading=13, textColor=HexColor("#1E293B"),
    fontName='Courier', backColor=HexColor("#F1F5F9"),
    leftIndent=16, rightIndent=16, spaceBefore=4, spaceAfter=8,
    borderPadding=(6, 8, 6, 8)
))
styles.add(ParagraphStyle(
    'Caption', parent=styles['Normal'],
    fontSize=9, leading=13, textColor=POLAY_GRAY,
    spaceAfter=12, alignment=TA_CENTER, fontName='Helvetica-Oblique'
))
styles.add(ParagraphStyle(
    'TOCEntry', parent=styles['Normal'],
    fontSize=12, leading=22, textColor=POLAY_BLUE,
    fontName='Helvetica', leftIndent=0
))
styles.add(ParagraphStyle(
    'TOCSub', parent=styles['Normal'],
    fontSize=10.5, leading=20, textColor=HexColor("#374151"),
    fontName='Helvetica', leftIndent=20
))
styles.add(ParagraphStyle(
    'Footer', parent=styles['Normal'],
    fontSize=8, leading=10, textColor=POLAY_GRAY,
    alignment=TA_CENTER, fontName='Helvetica'
))
styles.add(ParagraphStyle(
    'TableCell', parent=styles['Normal'],
    fontSize=9.5, leading=13, textColor=black,
    fontName='Helvetica', alignment=TA_LEFT
))
styles.add(ParagraphStyle(
    'TableHeader', parent=styles['Normal'],
    fontSize=9.5, leading=13, textColor=white,
    fontName='Helvetica-Bold', alignment=TA_LEFT
))
styles.add(ParagraphStyle(
    'Abstract', parent=styles['Normal'],
    fontSize=11, leading=17, textColor=HexColor("#374151"),
    spaceAfter=8, alignment=TA_JUSTIFY, fontName='Helvetica-Oblique',
    leftIndent=30, rightIndent=30
))

# ── Helpers ─────────────────────────────────────────────
def hr():
    return HRFlowable(width="100%", thickness=1, color=TABLE_BORDER, spaceAfter=12, spaceBefore=6)

def section_hr():
    return HRFlowable(width="100%", thickness=2, color=POLAY_ACCENT, spaceAfter=8, spaceBefore=2)

def bullet(text):
    return Paragraph(f"<bullet>&bull;</bullet> {text}", styles['WPBullet'])

def make_table(headers, rows, col_widths=None):
    """Create a styled table."""
    h = [Paragraph(h, styles['TableHeader']) for h in headers]
    data = [h]
    for row in rows:
        data.append([Paragraph(str(c), styles['TableCell']) for c in row])

    if col_widths is None:
        col_widths = [None] * len(headers)

    t = Table(data, colWidths=col_widths, repeatRows=1)
    style_cmds = [
        ('BACKGROUND', (0, 0), (-1, 0), TABLE_HEADER),
        ('TEXTCOLOR', (0, 0), (-1, 0), white),
        ('FONTNAME', (0, 0), (-1, 0), 'Helvetica-Bold'),
        ('FONTSIZE', (0, 0), (-1, 0), 9.5),
        ('BOTTOMPADDING', (0, 0), (-1, 0), 8),
        ('TOPPADDING', (0, 0), (-1, 0), 8),
        ('GRID', (0, 0), (-1, -1), 0.5, TABLE_BORDER),
        ('VALIGN', (0, 0), (-1, -1), 'TOP'),
        ('LEFTPADDING', (0, 0), (-1, -1), 8),
        ('RIGHTPADDING', (0, 0), (-1, -1), 8),
        ('TOPPADDING', (0, 1), (-1, -1), 5),
        ('BOTTOMPADDING', (0, 1), (-1, -1), 5),
    ]
    for i in range(1, len(data)):
        if i % 2 == 0:
            style_cmds.append(('BACKGROUND', (0, i), (-1, i), TABLE_ALT))
    t.setStyle(TableStyle(style_cmds))
    return t


class CoverPage(Flowable):
    """Custom cover page flowable."""
    def __init__(self, width, height):
        Flowable.__init__(self)
        self.width = width
        self.height = height

    def draw(self):
        c = self.canv
        h = self.height
        # Background
        c.setFillColor(POLAY_BLUE)
        c.rect(0, h - 220, self.width, 220, fill=1, stroke=0)

        # Title
        c.setFillColor(white)
        c.setFont('Helvetica-Bold', 42)
        c.drawCentredString(self.width / 2, h - 80, "POLAY")

        c.setFont('Helvetica', 18)
        c.drawCentredString(self.width / 2, h - 115, "A Gaming-Native Layer 1 Blockchain")

        c.setFont('Helvetica', 12)
        c.setFillColor(POLAY_LIGHT)
        c.drawCentredString(self.width / 2, h - 145, "Technical Whitepaper")

        c.setFont('Helvetica', 10)
        c.drawCentredString(self.width / 2, h - 170, "Version 1.0  |  April 2026")

        # Decorative line
        c.setStrokeColor(POLAY_ACCENT)
        c.setLineWidth(3)
        c.line(self.width/2 - 80, h - 195, self.width/2 + 80, h - 195)


def add_page_number(canvas_obj, doc):
    """Footer with page numbers."""
    canvas_obj.saveState()
    canvas_obj.setFont('Helvetica', 8)
    canvas_obj.setFillColor(POLAY_GRAY)
    canvas_obj.drawCentredString(letter[0] / 2, 30, f"POLAY Whitepaper  |  Page {doc.page}")
    canvas_obj.drawRightString(letter[0] - 50, 30, "Confidential")
    canvas_obj.restoreState()


# ── Content Builder ─────────────────────────────────────
def build_whitepaper():
    doc = SimpleDocTemplate(
        OUTPUT_PATH,
        pagesize=letter,
        leftMargin=60, rightMargin=60,
        topMargin=50, bottomMargin=60,
        title="POLAY: A Gaming-Native Layer 1 Blockchain - Technical Whitepaper",
        author="POLAY Foundation",
        subject="Blockchain Whitepaper",
    )

    W = letter[0] - 120  # usable width
    story = []

    # ════════════════════════════════════════════════════
    # COVER PAGE
    # ════════════════════════════════════════════════════
    story.append(CoverPage(W, 300))
    story.append(Spacer(1, 30))

    story.append(Paragraph(
        "POLAY is a purpose-built Layer 1 blockchain engineered from the ground up for the gaming industry. "
        "It provides native primitives for asset management, player identity, marketplace operations, "
        "tournaments, guilds, and anti-cheat attestation -- all at the protocol level, not as smart contract afterthoughts.",
        styles['Abstract']
    ))
    story.append(Spacer(1, 12))
    story.append(Paragraph(
        "Built in Rust with 16 crates, 767 tests, and 40 transaction types, POLAY delivers sub-second finality, "
        "parallel transaction execution, session keys for seamless gaming UX, and gas sponsorship so new players "
        "never need tokens to start playing.",
        styles['Abstract']
    ))

    story.append(PageBreak())

    # ════════════════════════════════════════════════════
    # TABLE OF CONTENTS
    # ════════════════════════════════════════════════════
    story.append(Paragraph("Table of Contents", styles['H1']))
    story.append(section_hr())

    toc_items = [
        ("1", "Abstract"),
        ("2", "Introduction & Motivation"),
        ("3", "Architecture Overview"),
        ("4", "Consensus: Delegated Proof-of-Stake BFT"),
        ("5", "State Model & Storage"),
        ("6", "Execution Engine"),
        ("7", "Transaction Types & Gas Schedule"),
        ("8", "Gaming Primitives"),
        ("9", "Tokenomics & Economics"),
        ("10", "Networking & P2P Protocol"),
        ("11", "Security Model"),
        ("12", "Developer Experience"),
        ("13", "Performance & Benchmarks"),
        ("14", "Governance"),
        ("15", "Roadmap"),
        ("16", "Conclusion"),
    ]
    for num, title in toc_items:
        story.append(Paragraph(f"<b>{num}.</b>&nbsp;&nbsp;&nbsp;{title}", styles['TOCEntry']))

    story.append(PageBreak())

    # ════════════════════════════════════════════════════
    # 1. ABSTRACT
    # ════════════════════════════════════════════════════
    story.append(Paragraph("1. Abstract", styles['H1']))
    story.append(section_hr())
    story.append(Paragraph(
        "The global gaming market generates over $200 billion in annual revenue, yet blockchain adoption in gaming "
        "remains marginal. The core problem is not a lack of interest -- it is a lack of infrastructure. Existing "
        "Layer 1 blockchains were designed for financial transactions and DeFi, forcing game developers to build "
        "complex smart contracts to replicate basic gaming operations: asset management, player identity, marketplace "
        "listings, tournament brackets, and guild systems.",
        styles['Body']
    ))
    story.append(Paragraph(
        "POLAY solves this by building gaming primitives directly into the protocol layer. Instead of deploying "
        "a smart contract to create an NFT marketplace, a game developer sends a single <b>CreateListing</b> transaction. "
        "Instead of writing Solidity for tournament prize distribution, they use the native <b>CreateTournament</b>, "
        "<b>JoinTournament</b>, and <b>ClaimTournamentPrize</b> actions. Every gaming operation is a first-class citizen "
        "in the POLAY protocol.",
        styles['Body']
    ))
    story.append(Paragraph(
        "This paper presents the complete technical design of POLAY: a Rust-based Layer 1 blockchain with "
        "Delegated Proof-of-Stake BFT consensus, parallel transaction execution, 40 native transaction types, "
        "session keys for frictionless gameplay, gas sponsorship for zero-cost onboarding, and a complete "
        "tokenomics model with inflation, fee burning, and treasury management.",
        styles['Body']
    ))

    story.append(PageBreak())

    # ════════════════════════════════════════════════════
    # 2. INTRODUCTION & MOTIVATION
    # ════════════════════════════════════════════════════
    story.append(Paragraph("2. Introduction & Motivation", styles['H1']))
    story.append(section_hr())

    story.append(Paragraph("2.1 The Problem with General-Purpose Chains", styles['H2']))
    story.append(Paragraph(
        "General-purpose blockchains like Ethereum, Solana, and Avalanche provide a Turing-complete execution "
        "environment where any logic can be deployed as a smart contract. While powerful, this approach has "
        "fundamental limitations for gaming applications:",
        styles['Body']
    ))
    story.append(bullet("<b>UX Friction</b> -- Every in-game action that touches the blockchain requires a wallet popup, signature confirmation, and gas payment. A player cannot swing a sword without approving a MetaMask transaction."))
    story.append(bullet("<b>Performance Overhead</b> -- Smart contract execution involves EVM/WASM interpretation, storage slot lookups, and gas metering overhead. Native protocol operations are orders of magnitude faster."))
    story.append(bullet("<b>Fragmented Standards</b> -- Each game must implement its own asset standard, marketplace logic, and identity system. There is no shared infrastructure for cross-game interoperability."))
    story.append(bullet("<b>Onboarding Barrier</b> -- New players must acquire native tokens to pay gas before they can perform any action, creating a chicken-and-egg problem that kills conversion rates."))
    story.append(bullet("<b>Cost Unpredictability</b> -- Gas prices on shared chains spike during network congestion, making game economics unpredictable and budget-breaking for studios."))
    story.append(Spacer(1, 8))

    story.append(Paragraph("2.2 The POLAY Approach", styles['H2']))
    story.append(Paragraph(
        "POLAY takes a fundamentally different approach: instead of providing a general-purpose virtual machine "
        "and expecting developers to build gaming infrastructure as smart contracts, POLAY embeds gaming primitives "
        "directly into the protocol layer. This design philosophy -- <b>native over interpreted, protocol over contract</b> -- "
        "delivers three key advantages:",
        styles['Body']
    ))
    story.append(bullet("<b>Performance</b> -- Native Rust execution with no interpretation overhead. Parallel transaction processing via rayon. Sub-second block finality."))
    story.append(bullet("<b>UX</b> -- Session keys eliminate wallet popups during gameplay. Gas sponsorship enables free onboarding. Deterministic gas costs enable predictable game economics."))
    story.append(bullet("<b>Interoperability</b> -- All games on POLAY share the same asset standards, identity system, and marketplace. A sword earned in Game A can be traded in Game B's marketplace with zero integration work."))

    story.append(PageBreak())

    # ════════════════════════════════════════════════════
    # 3. ARCHITECTURE OVERVIEW
    # ════════════════════════════════════════════════════
    story.append(Paragraph("3. Architecture Overview", styles['H1']))
    story.append(section_hr())

    story.append(Paragraph(
        "POLAY is implemented as a Rust monorepo comprising 16 crates organized into four layers: "
        "types, state, execution, and infrastructure.",
        styles['Body']
    ))

    story.append(Paragraph("3.1 Crate Architecture", styles['H2']))

    crate_table = make_table(
        ["Layer", "Crate", "Responsibility"],
        [
            ["Types", "polay-types", "Core data structures: Block, Transaction, Address, Hash, 40 action types, gaming types (Rental, Guild, Tournament), economics types"],
            ["Types", "polay-crypto", "Ed25519 signing/verification via ed25519-dalek v2, SHA-256 hashing, Merkle tree computation"],
            ["State", "polay-state", "StateStore trait (RocksDB + in-memory), key namespacing with 33 prefix bytes, state view/writer, Merkle commitments, snapshots, sync protocol"],
            ["State", "polay-config", "ChainConfig (50+ parameters), NodeConfig, network profiles (devnet/testnet/mainnet), config validation"],
            ["State", "polay-genesis", "Genesis document generation, validation, devnet/testnet/mainnet ceremony"],
            ["Execution", "polay-execution", "Transaction validation (stateless + stateful), executor with fee distribution, 12 execution modules, parallel executor, gas metering, state invariant checker"],
            ["Execution", "polay-consensus", "BFT state machine: propose, prevote, precommit, commit with 2/3+ quorum"],
            ["Execution", "polay-mempool", "Priority mempool with nonce ordering, duplicate detection, size limits"],
            ["Execution", "polay-staking", "Validator management, delegation, slashing, epoch rewards, inflation"],
            ["Execution", "polay-market", "Marketplace listing, buying, protocol fees, royalties"],
            ["Execution", "polay-identity", "Player profiles, achievements, reputation scoring"],
            ["Execution", "polay-attestation", "Anti-cheat match result verification, attestor management"],
            ["Infra", "polay-validator", "Validator node: block production, BFT loop, epoch transitions, chain state management"],
            ["Infra", "polay-network", "libp2p P2P networking: gossipsub, mDNS, peer scoring, rate limiting, protocol versioning"],
            ["Infra", "polay-rpc", "JSON-RPC server (HTTP + WebSocket): 32 methods, 3 subscriptions, rate limiting, event bus"],
            ["Infra", "polay-node", "CLI binary: run, init, keygen, bench subcommands"],
        ],
        col_widths=[50, 90, W - 140]
    )
    story.append(crate_table)
    story.append(Spacer(1, 8))
    story.append(Paragraph("Table 1: POLAY crate architecture organized by layer.", styles['Caption']))

    story.append(Paragraph("3.2 Data Flow", styles['H2']))
    story.append(Paragraph(
        "A transaction flows through the system as follows: (1) The client builds and signs a Transaction using the "
        "TypeScript SDK or wallet CLI. (2) The signed transaction is submitted via JSON-RPC to a validator node. "
        "(3) The RPC server performs Ed25519 signature verification, rate limit checking, and forwards valid "
        "transactions to the mempool. (4) The block producer selects transactions from the mempool, executes them "
        "(potentially in parallel via rayon), computes the Merkle state root, and proposes a block. (5) In BFT mode, "
        "validators exchange prevote and precommit messages; once 2/3+ stake quorum is reached, the block is committed. "
        "(6) The committed block is applied to state: receipts, events, and transaction locations are persisted. "
        "(7) The event bus broadcasts NewBlock and TransactionConfirmed events to WebSocket subscribers.",
        styles['Body']
    ))

    story.append(Paragraph("3.3 Technology Stack", styles['H2']))
    tech_table = make_table(
        ["Component", "Technology", "Purpose"],
        [
            ["Language", "Rust (2021 edition)", "Memory safety, performance, fearless concurrency"],
            ["Cryptography", "ed25519-dalek v2, sha2", "Ed25519 signatures, SHA-256 hashing"],
            ["Serialization", "Borsh (state), serde/JSON (RPC)", "Binary for storage, JSON for API"],
            ["Storage", "RocksDB", "Persistent key-value state with prefix iteration"],
            ["Networking", "libp2p v0.54", "TCP/Noise/Yamux, gossipsub, mDNS"],
            ["RPC", "jsonrpsee v0.24", "JSON-RPC 2.0 over HTTP + WebSocket"],
            ["Parallelism", "rayon", "Work-stealing thread pool for parallel tx execution"],
            ["Async Runtime", "tokio", "Event-driven validator loop, RPC server"],
            ["Client SDK", "TypeScript + @noble/ed25519", "Browser and Node.js client library"],
        ],
        col_widths=[80, 130, W - 210]
    )
    story.append(tech_table)

    story.append(PageBreak())

    # ════════════════════════════════════════════════════
    # 4. CONSENSUS
    # ════════════════════════════════════════════════════
    story.append(Paragraph("4. Consensus: Delegated Proof-of-Stake BFT", styles['H1']))
    story.append(section_hr())

    story.append(Paragraph(
        "POLAY uses a Delegated Proof-of-Stake (DPoS) consensus mechanism combined with Byzantine Fault Tolerant "
        "(BFT) block finality. This provides instant finality (no probabilistic confirmations), energy efficiency, "
        "and governance participation through stake delegation.",
        styles['Body']
    ))

    story.append(Paragraph("4.1 BFT Protocol", styles['H2']))
    story.append(Paragraph(
        "The consensus protocol follows a four-phase commit cycle inspired by Tendermint:",
        styles['Body']
    ))
    story.append(bullet("<b>Propose</b> -- The designated proposer (round-robin weighted by stake) assembles a block from the mempool, executes all transactions, computes the state root, and broadcasts the proposal."))
    story.append(bullet("<b>Prevote</b> -- Each validator receives the proposal, runs pre-consensus validation (8 integrity checks including signature verification, parent hash, transactions root, and state root), and broadcasts a signed prevote."))
    story.append(bullet("<b>Precommit</b> -- Once a validator observes 2/3+ stake-weighted prevotes for the same block, it broadcasts a signed precommit."))
    story.append(bullet("<b>Commit</b> -- Once 2/3+ stake-weighted precommits are collected, the block is committed to state. A CommitProof containing all precommit signatures is stored alongside the block."))
    story.append(Spacer(1, 6))
    story.append(Paragraph(
        "The quorum threshold is configurable (default: 6667 bps = 66.67%), ensuring BFT safety as long as "
        "fewer than 1/3 of stake-weighted validators are Byzantine.",
        styles['Body']
    ))

    story.append(Paragraph("4.2 Validator Set Management", styles['H2']))
    story.append(Paragraph(
        "The active validator set is updated at each epoch boundary. The EpochManager performs the following steps:",
        styles['Body']
    ))
    story.append(bullet("Unjail validators whose jail period has elapsed (1,800 blocks at default settings)."))
    story.append(bullet("Filter to Active validators with stake >= min_stake."))
    story.append(bullet("Sort by total stake descending, truncate to max_validators."))
    story.append(bullet("Build a new ValidatorSet with stake-proportional voting weights."))
    story.append(bullet("Distribute epoch rewards to validators and their delegators."))
    story.append(bullet("Store EpochInfo (epoch number, validator set, total staked, rewards distributed)."))

    story.append(Paragraph("4.3 Slashing", styles['H2']))
    story.append(Paragraph(
        "Validators that misbehave are penalized through slashing. POLAY implements proportional delegation "
        "slashing -- when a validator is slashed, all delegators to that validator also lose a proportional "
        "fraction of their delegated stake. This aligns incentives: delegators must choose validators carefully.",
        styles['Body']
    ))
    slash_table = make_table(
        ["Offense", "Devnet Penalty", "Mainnet Penalty", "Jail Duration"],
        [
            ["Downtime", "1% of stake", "1% of stake", "1,800 blocks (~1 hour)"],
            ["Double Sign", "5% of stake", "10% of stake", "1,800 blocks (~1 hour)"],
        ],
        col_widths=[100, 100, 100, W - 300]
    )
    story.append(slash_table)

    story.append(PageBreak())

    # ════════════════════════════════════════════════════
    # 5. STATE MODEL
    # ════════════════════════════════════════════════════
    story.append(Paragraph("5. State Model & Storage", styles['H1']))
    story.append(section_hr())

    story.append(Paragraph("5.1 Key-Value Architecture", styles['H2']))
    story.append(Paragraph(
        "POLAY uses a flat key-value state model backed by RocksDB for persistence and an in-memory BTreeMap "
        "for testing. All state entries are namespaced using single-byte prefixes, enabling efficient range "
        "queries and logical separation of domains.",
        styles['Body']
    ))

    prefix_table = make_table(
        ["Prefix", "Domain", "Example Key Pattern"],
        [
            ["0x00", "Account State", "0x00 || address (32 bytes)"],
            ["0x01", "Asset Classes", "0x01 || class_id (32 bytes)"],
            ["0x02", "Asset Balances", "0x02 || class_id || owner_address"],
            ["0x03", "Marketplace Listings", "0x03 || listing_id"],
            ["0x04-0x06", "Identity & Social", "Profiles, achievements, reputation"],
            ["0x07-0x08", "Staking", "Validator info, delegations"],
            ["0x09-0x0B", "Attestation", "Attestors, match results"],
            ["0x0C-0x0E", "Chain Metadata", "Height, hash, blocks"],
            ["0x0F-0x11", "Governance", "Proposals, votes"],
            ["0x12-0x14", "Receipts & Events", "Transaction receipts, events, locations"],
            ["0x15-0x16", "Sessions", "Session key grants"],
            ["0x17-0x18", "Epochs & Supply", "Epoch info, supply tracking"],
            ["0x19-0x1B", "Rentals", "Rental listings, by-owner, by-renter indexes"],
            ["0x1C-0x1E", "Guilds", "Guild state, membership, member index"],
            ["0x1F-0x20", "Tournaments", "Tournament state, participant index"],
        ],
        col_widths=[60, 120, W - 180]
    )
    story.append(prefix_table)
    story.append(Paragraph("Table 2: State key prefix allocation across 33 namespaces.", styles['Caption']))

    story.append(Paragraph("5.2 Merkle State Commitments", styles['H2']))
    story.append(Paragraph(
        "After executing each block, POLAY computes a Merkle state root over all key-value entries. The algorithm "
        "sorts all entries lexicographically by key, builds a binary Merkle tree using SHA-256 as the hash function, "
        "and stores the root in the block header. This enables lightweight state verification: a client can verify "
        "any state entry exists by checking a logarithmic-size Merkle proof against the block header's state root.",
        styles['Body']
    ))

    story.append(Paragraph("5.3 State Sync Protocol", styles['H2']))
    story.append(Paragraph(
        "New nodes joining the network do not need to replay every block from genesis. POLAY implements a "
        "chunk-based state sync protocol:",
        styles['Body']
    ))
    story.append(bullet("<b>SnapshotCreator</b> takes a consistent snapshot of all state at a given block height, splits it into ~1 MB chunks, and computes a SHA-256 hash per chunk."))
    story.append(bullet("<b>StateSyncManager</b> drives a state machine: Idle -> RequestingSnapshot -> DownloadingChunks -> Verifying -> Complete. It handles out-of-order delivery and rejects tampered chunks."))
    story.append(bullet("<b>SnapshotRestorer</b> verifies each chunk against the snapshot metadata, applies verified chunks to the target store, and verifies the final state root matches."))
    story.append(Paragraph(
        "The sync protocol uses four dedicated P2P message types: RequestSnapshot, SnapshotMetadata, "
        "RequestChunk, and SnapshotChunkData.",
        styles['Body']
    ))

    story.append(PageBreak())

    # ════════════════════════════════════════════════════
    # 6. EXECUTION ENGINE
    # ════════════════════════════════════════════════════
    story.append(Paragraph("6. Execution Engine", styles['H1']))
    story.append(section_hr())

    story.append(Paragraph("6.1 Transaction Lifecycle", styles['H2']))
    story.append(Paragraph(
        "Each transaction passes through a rigorous validation and execution pipeline:",
        styles['Body']
    ))
    story.append(bullet("<b>Stateless Validation</b> -- Chain ID match, max_fee > 0, signer not zero address, Ed25519 signature verification (including session key path), sponsor validation (not self, not zero)."))
    story.append(bullet("<b>Input Validation</b> -- Per-action field validation: string lengths, amount > 0, valid addresses/hashes, domain-specific rules (e.g., prize_distribution sums to 100)."))
    story.append(bullet("<b>Stateful Validation</b> -- Account existence, nonce match, balance covers max_fee (or sponsor balance for sponsored txs), action-specific preconditions."))
    story.append(bullet("<b>Fee Deduction</b> -- Gas calculated, fee deducted from payer (sponsor or signer), distributed: 50% burned, 20% treasury, 30% block producer."))
    story.append(bullet("<b>Action Dispatch</b> -- Routed to one of 12 execution modules based on action type."))
    story.append(bullet("<b>Receipt Generation</b> -- TransactionReceipt with success/failure, gas_used, fee_payer, events, and optional error message."))
    story.append(Paragraph(
        "Critically, fees are deducted <b>even if the action fails</b>. This prevents free state access via "
        "deliberately failing transactions and ensures spam always has a cost.",
        styles['Body']
    ))

    story.append(Paragraph("6.2 Parallel Execution", styles['H2']))
    story.append(Paragraph(
        "POLAY supports parallel transaction execution via a rayon-powered scheduler. The system analyzes each "
        "transaction's read and write sets (account keys, asset keys, rental IDs, guild IDs, etc.) to identify "
        "non-conflicting transactions that can safely execute concurrently.",
        styles['Body']
    ))
    story.append(bullet("<b>AccessSet</b> predicts each transaction's read/write keys before execution."))
    story.append(bullet("<b>Scheduler</b> uses greedy first-fit bin packing to group non-conflicting transactions into parallel batches."))
    story.append(bullet("<b>OverlayStore</b> gives each transaction its own write-through cache over the base state. After execution, overlays are merged sequentially to maintain determinism."))
    story.append(Paragraph(
        "This design ensures that parallel execution produces identical results to sequential execution, "
        "maintaining consensus determinism while improving throughput on multi-core hardware.",
        styles['Body']
    ))

    story.append(Paragraph("6.3 Session Keys", styles['H2']))
    story.append(Paragraph(
        "Session keys solve the fundamental UX problem of blockchain gaming: wallet popup fatigue. A player "
        "creates a temporary session key with scoped permissions, allowing the game client to sign transactions "
        "on the player's behalf without requiring wallet interaction for every action.",
        styles['Body']
    ))
    story.append(bullet("<b>SessionGrant</b> specifies: session public key, permitted actions (e.g., only TransferAsset and SubmitMatchResult), expiration height, and cumulative spending limit."))
    story.append(bullet("The session key signs transactions, but the <b>signer's account</b> pays fees and owns the state changes."))
    story.append(bullet("Sessions can be revoked at any time by the granting account."))
    story.append(bullet("Spending limits are tracked cumulatively -- once the limit is reached, the session becomes inoperative."))

    story.append(Paragraph("6.4 Gas Sponsorship", styles['H2']))
    story.append(Paragraph(
        "Gas sponsorship enables zero-cost onboarding for new players. A game studio or guild can sponsor "
        "transactions for their users by setting the <b>sponsor</b> field on a Transaction. The sponsor's account "
        "pays the gas fee while the signer executes the action.",
        styles['Body']
    ))
    story.append(bullet("Sponsor's balance is checked during stateful validation (not the signer's for fee purposes)."))
    story.append(bullet("Sponsor's nonce is NOT incremented -- a single sponsor can sponsor many concurrent users."))
    story.append(bullet("Self-sponsorship (sponsor == signer) is rejected to prevent confusion."))
    story.append(bullet("Sponsored transactions are conflict-aware: two transactions sharing a sponsor are serialized (not parallelized) to prevent balance corruption."))

    story.append(PageBreak())

    # ════════════════════════════════════════════════════
    # 7. TRANSACTION TYPES & GAS
    # ════════════════════════════════════════════════════
    story.append(Paragraph("7. Transaction Types & Gas Schedule", styles['H1']))
    story.append(section_hr())

    story.append(Paragraph(
        "POLAY defines 40 native transaction types across 8 domains. Each type has a fixed gas cost "
        "determined by the GasSchedule. The total fee is calculated as: "
        "<b>fee = (21,000 + action_gas + tx_bytes x 16) x gas_price</b>.",
        styles['Body']
    ))

    story.append(Paragraph("7.1 Core Operations", styles['H2']))
    gas_core = make_table(
        ["Action", "Gas", "Description"],
        [
            ["Transfer", "5,000", "Send POL between accounts"],
            ["CreateAssetClass", "50,000", "Define a new asset type (fungible, NFT, semi-fungible)"],
            ["MintAsset", "30,000", "Mint new units of an asset class"],
            ["TransferAsset", "10,000", "Transfer assets between players"],
            ["BurnAsset", "10,000", "Permanently destroy asset units"],
            ["CreateListing", "40,000", "List assets on the marketplace"],
            ["CancelListing", "20,000", "Remove a marketplace listing"],
            ["BuyListing", "60,000", "Purchase from marketplace (with protocol fee + royalties)"],
        ],
        col_widths=[110, 55, W - 165]
    )
    story.append(gas_core)
    story.append(Spacer(1, 8))

    story.append(Paragraph("7.2 Identity & Social", styles['H2']))
    gas_id = make_table(
        ["Action", "Gas", "Description"],
        [
            ["CreateProfile", "30,000", "Create a player profile with username"],
            ["AddAchievement", "20,000", "Award an achievement to a player"],
            ["UpdateReputation", "15,000", "Adjust player reputation score"],
        ],
        col_widths=[110, 55, W - 165]
    )
    story.append(gas_id)
    story.append(Spacer(1, 8))

    story.append(Paragraph("7.3 Staking & Governance", styles['H2']))
    gas_stake = make_table(
        ["Action", "Gas", "Description"],
        [
            ["RegisterValidator", "100,000", "Register as a validator node"],
            ["DelegateStake", "30,000", "Delegate POL to a validator"],
            ["UndelegateStake", "30,000", "Begin unbonding delegation"],
            ["SubmitProposal", "100,000", "Submit a governance proposal"],
            ["VoteProposal", "30,000", "Cast a stake-weighted vote"],
            ["ExecuteProposal", "50,000", "Execute a passed proposal"],
        ],
        col_widths=[110, 55, W - 165]
    )
    story.append(gas_stake)
    story.append(Spacer(1, 8))

    story.append(Paragraph("7.4 Gaming Features", styles['H2']))
    gas_gaming = make_table(
        ["Action", "Gas", "Description"],
        [
            ["RegisterAttestor", "50,000", "Register a game attestor for anti-cheat"],
            ["SubmitMatchResult", "80,000", "Submit verified match results"],
            ["DistributeReward", "40K + 5K/recipient", "Distribute match rewards"],
            ["CreateSession", "50,000", "Create a scoped session key"],
            ["RevokeSession", "20,000", "Revoke a session key"],
            ["ListForRent", "30,000", "List a game asset for rental"],
            ["RentAsset", "40,000", "Rent an asset with deposit"],
            ["ReturnRental", "25,000", "Return a rented asset early"],
            ["ClaimExpiredRental", "25,000", "Claim deposit from expired rental"],
            ["CancelRentalListing", "20,000", "Cancel a rental listing"],
            ["CreateGuild", "50,000", "Create an on-chain guild"],
            ["JoinGuild", "20,000", "Join an existing guild"],
            ["LeaveGuild", "20,000", "Leave a guild"],
            ["GuildDeposit", "25,000", "Deposit POL to guild treasury"],
            ["GuildWithdraw", "25,000", "Withdraw from guild treasury"],
            ["GuildPromote", "15,000", "Promote a guild member"],
            ["GuildKick", "20,000", "Remove a guild member"],
            ["CreateTournament", "50,000", "Create a tournament with prize pool"],
            ["JoinTournament", "25,000", "Enter a tournament (pays entry fee)"],
            ["StartTournament", "30,000", "Trigger tournament start"],
            ["ReportTournamentResults", "40K + 5K/rank", "Submit final rankings"],
            ["ClaimTournamentPrize", "25,000", "Claim tournament winnings"],
            ["CancelTournament", "30,000", "Cancel and refund all participants"],
        ],
        col_widths=[130, 80, W - 210]
    )
    story.append(gas_gaming)

    story.append(PageBreak())

    # ════════════════════════════════════════════════════
    # 8. GAMING PRIMITIVES
    # ════════════════════════════════════════════════════
    story.append(Paragraph("8. Gaming Primitives", styles['H1']))
    story.append(section_hr())

    story.append(Paragraph(
        "POLAY's core differentiation is that gaming operations are protocol-native. This section details "
        "the four gaming systems built into the chain.",
        styles['Body']
    ))

    story.append(Paragraph("8.1 Asset Rental System", styles['H2']))
    story.append(Paragraph(
        "The rental system enables players to rent game assets (NFTs, items, characters) to other players "
        "for a specified duration with an upfront deposit. This is critical for gaming economies where rare "
        "items should be accessible without permanent purchase.",
        styles['Body']
    ))
    story.append(bullet("<b>Listing</b>: Asset owner sets price_per_block, deposit amount, and min/max duration constraints."))
    story.append(bullet("<b>Renting</b>: Renter pays (price_per_block x duration) + deposit upfront. Payment is credited to the owner immediately."))
    story.append(bullet("<b>Early Return</b>: Renter returns the asset before expiry. Remaining blocks are refunded: refund = remaining_blocks x price_per_block + deposit."))
    story.append(bullet("<b>Expiry</b>: Anyone can trigger ClaimExpiredRental after the end_height. Deposit is returned to the renter; asset reverts to the owner."))
    story.append(bullet("<b>Cancellation</b>: Owner can cancel an unrented listing at any time."))

    story.append(Paragraph("8.2 Guild System", styles['H2']))
    story.append(Paragraph(
        "Guilds are on-chain organizations with a hierarchical role system (Leader, Officer, Member) and "
        "a shared treasury. This enables collective economic activity: guilds can pool resources, sponsor "
        "members' gas fees, and manage shared asset portfolios.",
        styles['Body']
    ))
    story.append(bullet("<b>Hierarchy</b>: Leader (creator) > Officers (promoted by leader) > Members. Only leaders can promote; officers+ can withdraw from treasury; officers can kick members but not other officers."))
    story.append(bullet("<b>Treasury</b>: Any member can deposit POL. Officers and leaders can withdraw. The treasury balance is tracked on the Guild state object."))
    story.append(bullet("<b>Auto-Dissolve</b>: When the last member leaves, the guild is automatically deleted from state."))
    story.append(bullet("<b>Size Limits</b>: Configurable max_members (1-10,000) set at creation."))

    story.append(Paragraph("8.3 Tournament System", styles['H2']))
    story.append(Paragraph(
        "Tournaments provide on-chain bracket management with entry fees, prize pools, and automated prize "
        "distribution. This eliminates the trust problem in competitive gaming: prize pools are held by the "
        "protocol, not a centralized organizer.",
        styles['Body']
    ))
    story.append(bullet("<b>Registration Phase</b>: Organizer creates a tournament specifying entry fee, participant limits, start height, and prize distribution (e.g., [50, 30, 20] for top 3). Players join by paying the entry fee, which accumulates in the prize pool."))
    story.append(bullet("<b>Active Phase</b>: Anyone can trigger StartTournament once the start_height is reached and minimum participants are met. Gameplay happens off-chain (verified by attestors)."))
    story.append(bullet("<b>Completion</b>: Organizer reports final rankings. Each ranked player's prize = prize_pool x distribution[rank] / 100."))
    story.append(bullet("<b>Claiming</b>: Players individually claim their prizes. Double-claiming is prevented by a per-rank claimed flag."))
    story.append(bullet("<b>Cancellation</b>: Organizer can cancel during registration, triggering a full refund to all participants."))
    story.append(bullet("<b>Free Tournaments</b>: entry_fee = 0 is supported for promotional events."))

    story.append(Paragraph("8.4 Anti-Cheat Attestation", styles['H2']))
    story.append(Paragraph(
        "POLAY's attestation system provides cryptographic verification of game match results. Registered "
        "attestors (game servers or trusted third parties) submit match results on-chain. Multiple attestors "
        "can verify the same match, and the system includes a quarantine mechanism for suspicious results.",
        styles['Body']
    ))
    story.append(bullet("<b>Attestors</b> register per game_id with an endpoint and metadata. They sign match results containing player addresses, scores, and game-specific data."))
    story.append(bullet("<b>Quarantine</b>: If a result's consistency score falls below the attestation_quarantine_threshold (default: 30), it is flagged for review."))
    story.append(bullet("<b>Rewards</b>: DistributeReward distributes POL to match participants based on results, with variable gas cost per recipient."))

    story.append(PageBreak())

    # ════════════════════════════════════════════════════
    # 9. TOKENOMICS
    # ════════════════════════════════════════════════════
    story.append(Paragraph("9. Tokenomics & Economics", styles['H1']))
    story.append(section_hr())

    story.append(Paragraph("9.1 Token: POL", styles['H2']))
    story.append(Paragraph(
        "POL is the native token of the POLAY blockchain, used for gas fees, staking, governance deposits, "
        "marketplace transactions, tournament entry fees, and guild treasuries. All amounts are denominated "
        "in the smallest indivisible unit of POL.",
        styles['Body']
    ))

    token_table = make_table(
        ["Parameter", "Value"],
        [
            ["Token Name", "POL"],
            ["Genesis Supply", "100,000,000 POL"],
            ["Initial Inflation Rate", "8% annual"],
            ["Inflation Floor", "2% annual"],
            ["Inflation Decay", "5% per year"],
            ["Target Staking Ratio", "67%"],
        ],
        col_widths=[160, W - 160]
    )
    story.append(token_table)
    story.append(Spacer(1, 8))

    story.append(Paragraph("9.2 Fee Distribution", styles['H2']))
    story.append(Paragraph(
        "Every gas fee collected is split three ways, creating a balanced economic model that rewards "
        "validators, funds protocol development, and creates deflationary pressure:",
        styles['Body']
    ))

    fee_table = make_table(
        ["Destination", "Share", "Purpose"],
        [
            ["Burn", "50%", "Permanently destroyed, reducing total supply. Creates deflationary pressure that counteracts inflation."],
            ["Treasury", "20%", "Protocol treasury at a designated address. Funds ecosystem development, grants, and protocol improvements."],
            ["Block Producer", "30%", "Direct reward to the validator that produced the block. Incentivizes participation and uptime."],
        ],
        col_widths=[90, 50, W - 140]
    )
    story.append(fee_table)
    story.append(Spacer(1, 8))

    story.append(Paragraph("9.3 Inflation & Block Rewards", styles['H2']))
    story.append(Paragraph(
        "Validators earn block rewards through inflation. The annual inflation rate starts at 8% and decays "
        "by 5% each year, with a floor of 2%. Rewards are distributed at each epoch boundary proportionally "
        "to validator stake, with delegators receiving their share minus the validator's commission.",
        styles['Body']
    ))
    story.append(Paragraph(
        "The effective inflation rate is responsive to the staking ratio. When staking participation is below "
        "the 67% target, higher rewards incentivize more staking. As the network matures, the combination of "
        "decaying inflation and fee burning is designed to reach equilibrium or even net deflation at high "
        "transaction volumes.",
        styles['Body']
    ))
    story.append(Paragraph(
        "Reward gaming is prevented by a <b>last_reward_epoch</b> field on each Delegation. Delegators must be "
        "staked for at least one full epoch before earning rewards, preventing the deposit-before-epoch-end attack.",
        styles['Body']
    ))

    story.append(Paragraph("9.4 Supply Tracking", styles['H2']))
    story.append(Paragraph(
        "POLAY maintains a real-time SupplyInfo object in state that tracks seven metrics: total_supply, "
        "circulating_supply, total_staked, total_burned, treasury_balance, total_minted, and "
        "total_fees_collected. This data is queryable via the <b>polay_getSupplyInfo</b> RPC method and is "
        "updated with every block.",
        styles['Body']
    ))

    story.append(PageBreak())

    # ════════════════════════════════════════════════════
    # 10. NETWORKING
    # ════════════════════════════════════════════════════
    story.append(Paragraph("10. Networking & P2P Protocol", styles['H1']))
    story.append(section_hr())

    story.append(Paragraph(
        "POLAY's networking layer is built on libp2p v0.54 with production hardening for peer management, "
        "rate limiting, and protocol versioning.",
        styles['Body']
    ))

    story.append(Paragraph("10.1 Transport & Discovery", styles['H2']))
    story.append(bullet("<b>Transport</b>: TCP with Noise encryption and Yamux multiplexing. All peer communication is encrypted."))
    story.append(bullet("<b>Discovery</b>: mDNS for local/devnet peer discovery. Static boot node multiaddrs for testnet/mainnet."))
    story.append(bullet("<b>Message Propagation</b>: gossipsub with signed message authentication and custom hash-based message IDs."))
    story.append(bullet("<b>Topics</b>: Three gossipsub topics -- <i>polay/txs/1</i> (transactions), <i>polay/blocks/1</i> (blocks), <i>polay/consensus/1</i> (votes)."))

    story.append(Paragraph("10.2 Peer Management", styles['H2']))
    story.append(bullet("<b>Peer Scoring</b>: Each peer starts at score 100. +1 for valid messages, -20 for invalid messages. Auto-ban at score <= -100."))
    story.append(bullet("<b>Connection Limits</b>: max_peers = 50, min_peers = 4. Excess connections trigger eviction of the lowest-scoring peer."))
    story.append(bullet("<b>Ban System</b>: Timed bans (default 1 hour) with automatic expiration. Banned peers are rejected on connection attempts."))
    story.append(bullet("<b>Rate Limiting</b>: Per-peer limits of 100 messages/second and 20 MB/second with 1-second sliding windows."))

    story.append(Paragraph("10.3 Protocol Versioning", styles['H2']))
    story.append(Paragraph(
        "All network messages are wrapped in a MessageEnvelope containing a protocol version number. "
        "Messages with incompatible versions are rejected, the sending peer's score is decremented, and "
        "a bad-message event is recorded. This ensures clean upgrades across protocol versions.",
        styles['Body']
    ))

    story.append(PageBreak())

    # ════════════════════════════════════════════════════
    # 11. SECURITY
    # ════════════════════════════════════════════════════
    story.append(Paragraph("11. Security Model", styles['H1']))
    story.append(section_hr())

    story.append(Paragraph("11.1 Cryptographic Foundation", styles['H2']))
    story.append(bullet("<b>Ed25519</b> for all signatures (transaction signing, validator votes, session keys). 128-bit security level."))
    story.append(bullet("<b>SHA-256</b> for all hashing (block hashes, transaction hashes, Merkle trees, key derivation)."))
    story.append(bullet("<b>Borsh</b> binary serialization for deterministic state encoding. JSON only for RPC interface."))

    story.append(Paragraph("11.2 Execution Safety", styles['H2']))
    story.append(bullet("<b>Checked Arithmetic</b>: All balance operations use checked_sub/saturating_add. No direct subtraction on financial values."))
    story.append(bullet("<b>Panic Safety</b>: Transaction execution is wrapped in catch_unwind. A panicking transaction produces a failure receipt without crashing the node."))
    story.append(bullet("<b>Fee-on-Failure</b>: Gas fees are deducted before action execution. Failed actions still cost gas, preventing free state probing."))
    story.append(bullet("<b>Nonce Protection</b>: Strict nonce ordering with max_nonce_gap = 16 prevents replay attacks and excessive nonce jumping."))
    story.append(bullet("<b>Transaction Expiration</b>: Transactions older than tx_max_age_seconds (300s) are rejected."))
    story.append(bullet("<b>Block Gas Limit</b>: max_block_gas = 100,000,000 prevents block stuffing attacks."))

    story.append(Paragraph("11.3 State Invariant Checker", styles['H2']))
    story.append(Paragraph(
        "POLAY includes a StateInvariantChecker diagnostic tool that verifies three properties:",
        styles['Body']
    ))
    story.append(bullet("<b>Supply Invariant</b>: Sum of all account balances + staked amounts + treasury equals total_supply."))
    story.append(bullet("<b>Account Invariants</b>: No account has inconsistent state (e.g., delegations reference non-existent validators)."))
    story.append(bullet("<b>Staking Invariants</b>: Sum of all delegations to a validator matches the validator's recorded total stake."))
    story.append(Paragraph(
        "This tool is designed for audit use and can be invoked at any block height to verify state consistency.",
        styles['Body']
    ))

    story.append(Paragraph("11.4 Network Security", styles['H2']))
    story.append(bullet("<b>Strict Gossipsub Validation</b>: Messages must be properly signed, correctly sized, and match their topic (tx on tx topic, block on block topic)."))
    story.append(bullet("<b>RPC Rate Limiting</b>: Transaction submission throttled to rpc_max_submissions_per_second (default: 100, mainnet: 50)."))
    story.append(bullet("<b>Message Size Limits</b>: TX messages capped at 128 KB, block messages at 10 MB, consensus messages at 4 KB."))
    story.append(bullet("<b>CORS Policy</b>: Configurable for browser-based clients."))

    story.append(PageBreak())

    # ════════════════════════════════════════════════════
    # 12. DEVELOPER EXPERIENCE
    # ════════════════════════════════════════════════════
    story.append(Paragraph("12. Developer Experience", styles['H1']))
    story.append(section_hr())

    story.append(Paragraph("12.1 JSON-RPC API", styles['H2']))
    story.append(Paragraph(
        "POLAY exposes 32 RPC methods over HTTP and WebSocket, plus 3 real-time subscription channels. "
        "All methods use the <b>polay_</b> prefix. The API enables full interaction with the chain: "
        "querying state, submitting transactions, monitoring events, and checking node health.",
        styles['Body']
    ))
    story.append(Paragraph(
        "Key endpoints include: <b>polay_health</b> (load balancer probes), <b>polay_getNodeInfo</b> "
        "(version, uptime, peers), <b>polay_getSupplyInfo</b> (tokenomics dashboard), <b>polay_estimateGas</b> "
        "(pre-flight fee estimation), and <b>polay_getNetworkStats</b> (network overview).",
        styles['Body']
    ))

    story.append(Paragraph("12.2 TypeScript SDK", styles['H2']))
    story.append(Paragraph(
        "The official TypeScript SDK provides a high-level client for browser and Node.js environments. It includes:",
        styles['Body']
    ))
    story.append(bullet("<b>PolayClient</b>: RPC wrapper with typed methods for all 32 endpoints."))
    story.append(bullet("<b>TransactionBuilder</b>: Fluent API for constructing and signing transactions with canonical JSON serialization."))
    story.append(bullet("<b>PolayKeypair</b>: Ed25519 key generation and management using @noble/ed25519."))
    story.append(bullet("Full support for session keys and gas sponsorship via optional <b>session</b> and <b>sponsor</b> parameters."))

    story.append(Paragraph("12.3 Wallet CLI", styles['H2']))
    story.append(Paragraph(
        "The <b>polay-wallet</b> binary provides 20+ subcommands for interacting with the chain from the terminal: "
        "key generation, balance queries, POL transfers, asset management (create, mint, transfer, burn), "
        "marketplace operations (list, buy, cancel), profile management, staking, and governance.",
        styles['Body']
    ))

    story.append(Paragraph("12.4 PostgreSQL Indexer", styles['H2']))
    story.append(Paragraph(
        "A standalone indexer binary polls the node RPC, decodes all 40 transaction types, and populates "
        "17 PostgreSQL tables with structured, queryable data. It supports resumable indexing from any height "
        "and automatic schema migration.",
        styles['Body']
    ))

    story.append(Paragraph("12.5 Explorer REST API", styles['H2']))
    story.append(Paragraph(
        "The explorer API provides 20+ REST endpoints under <b>/api/v1/</b> for building block explorers and "
        "dashboards: block listing, transaction search, account lookup, validator status, marketplace browsing, "
        "and a unified search endpoint that resolves block heights, tx hashes, addresses, and match IDs.",
        styles['Body']
    ))

    story.append(PageBreak())

    # ════════════════════════════════════════════════════
    # 13. PERFORMANCE
    # ════════════════════════════════════════════════════
    story.append(Paragraph("13. Performance & Benchmarks", styles['H1']))
    story.append(section_hr())

    story.append(Paragraph(
        "POLAY is designed for high throughput with deterministic performance. Key metrics:",
        styles['Body']
    ))

    perf_table = make_table(
        ["Metric", "Value", "Conditions"],
        [
            ["Sequential TPS (in-memory)", "735,000+", "MemoryStore, single-thread, Transfer actions"],
            ["Block Time", "2-4 seconds", "Configurable per network profile"],
            ["Finality", "Instant (1 block)", "BFT commit = final, no reorgs"],
            ["Max Block Transactions", "10,000 (devnet)", "5,000 on mainnet for safety"],
            ["Max Block Gas", "100,000,000", "Prevents block stuffing"],
            ["State Sync", "~1 MB chunks", "Parallel chunk download"],
            ["Merkle Proof", "O(log n)", "Logarithmic verification"],
        ],
        col_widths=[130, 100, W - 230]
    )
    story.append(perf_table)
    story.append(Spacer(1, 8))
    story.append(Paragraph(
        "The built-in benchmark suite (<b>polay bench --txs 10000</b>) measures sequential vs. parallel execution "
        "throughput in memory, providing a baseline for hardware sizing. A separate network load test script "
        "measures end-to-end throughput including RPC, mempool, and block production.",
        styles['Body']
    ))

    story.append(PageBreak())

    # ════════════════════════════════════════════════════
    # 14. GOVERNANCE
    # ════════════════════════════════════════════════════
    story.append(Paragraph("14. Governance", styles['H1']))
    story.append(section_hr())

    story.append(Paragraph(
        "POLAY implements on-chain governance allowing stakeholders to propose and vote on protocol changes. "
        "Governance is stake-weighted: each voter's influence is proportional to their staked POL.",
        styles['Body']
    ))

    story.append(Paragraph("14.1 Proposal Lifecycle", styles['H2']))
    story.append(bullet("<b>Submission</b>: Any account can submit a proposal by depositing min_proposal_deposit POL. The proposal specifies a title, description, and one of six ProposalActions."))
    story.append(bullet("<b>Voting Period</b>: Lasts voting_period_blocks (8h devnet, 3.5d testnet, 14d mainnet). Voters choose Yes, No, or Abstain."))
    story.append(bullet("<b>Quorum</b>: governance_quorum_bps of total staked POL must participate (33% devnet, 40% mainnet)."))
    story.append(bullet("<b>Pass Threshold</b>: pass_threshold_bps of non-abstain votes must be Yes (50% devnet, 60% mainnet)."))
    story.append(bullet("<b>Execution</b>: Passed proposals can be executed by anyone, triggering the specified ProposalAction."))

    story.append(Paragraph("14.2 Proposal Actions", styles['H2']))
    gov_table = make_table(
        ["Action", "Description"],
        [
            ["UpdateConfig", "Modify chain configuration parameters (block time, gas limits, fees, etc.)"],
            ["SpendTreasury", "Transfer POL from the protocol treasury to a specified address"],
            ["SlashValidator", "Slash a specific validator's stake by a specified fraction"],
            ["UpdateValidatorSet", "Force-update the active validator set"],
            ["EmergencyHalt", "Halt block production (circuit breaker)"],
            ["TextProposal", "Non-binding signaling proposal for community direction"],
        ],
        col_widths=[120, W - 120]
    )
    story.append(gov_table)

    story.append(PageBreak())

    # ════════════════════════════════════════════════════
    # 15. ROADMAP
    # ════════════════════════════════════════════════════
    story.append(Paragraph("15. Roadmap", styles['H1']))
    story.append(section_hr())

    road_table = make_table(
        ["Phase", "Status", "Deliverables"],
        [
            ["Phase 0: MVP Foundation", "COMPLETE", "Monorepo structure, 16 crates, core types, Ed25519 crypto, RocksDB state, BFT consensus, 22 initial transaction types, JSON-RPC server, mempool, devnet scripts, TypeScript SDK"],
            ["Phase 1: Production Hardening", "COMPLETE", "Pre-consensus block validation, receipt/event storage, epoch transitions, parallel execution (rayon), session keys, complete indexer (17 tables)"],
            ["Phase 2: Infrastructure", "COMPLETE", "State sync protocol (snapshot + chunk download), gas sponsorship (meta-transactions), network hardening (peer scoring, rate limiting, protocol versioning, ban lists)"],
            ["Phase 3: Gaming Features", "COMPLETE", "Asset rental system (5 tx types), guild system (7 tx types), tournament system (6 tx types) -- all with full test coverage"],
            ["Phase 4: Mainnet Ready", "COMPLETE", "Tokenomics engine (inflation, fee burn, supply tracking), security hardening (checked arithmetic, delegation slashing, invariant checker), mainnet configuration profiles, health endpoints, genesis ceremony"],
            ["Phase 5: Public Testnet", "NEXT", "Public testnet launch, faucet, block explorer, documentation site, bug bounty program, third-party security audit"],
            ["Phase 6: Mainnet", "PLANNED", "Mainnet genesis ceremony, mainnet launch, exchange listings, SDK ecosystem expansion, game studio partnerships"],
        ],
        col_widths=[120, 70, W - 190]
    )
    story.append(road_table)

    story.append(PageBreak())

    # ════════════════════════════════════════════════════
    # 16. CONCLUSION
    # ════════════════════════════════════════════════════
    story.append(Paragraph("16. Conclusion", styles['H1']))
    story.append(section_hr())

    story.append(Paragraph(
        "POLAY represents a paradigm shift in blockchain design for gaming. Rather than forcing game developers "
        "to build atop general-purpose smart contract platforms, POLAY provides the entire gaming infrastructure "
        "stack as native protocol operations. The result is a blockchain where creating a tournament, renting an "
        "in-game item, or managing a guild treasury is as natural as transferring tokens.",
        styles['Body']
    ))
    story.append(Paragraph(
        "The implementation demonstrates this vision concretely: 16 Rust crates, 40 native transaction types "
        "spanning 8 gaming domains, 767 passing tests, and three network profiles ready for devnet, testnet, "
        "and mainnet deployment. The architecture -- DPoS BFT consensus, parallel execution, session keys, "
        "gas sponsorship, and a complete tokenomics model -- addresses every major barrier to blockchain gaming "
        "adoption.",
        styles['Body']
    ))
    story.append(Paragraph(
        "POLAY is not just another blockchain claiming to support gaming. It is a blockchain engineered, from its "
        "type system to its consensus mechanism to its fee distribution model, with gaming as the primary use case. "
        "Every design decision -- from the 40 transaction types to the session key permission model to the rental "
        "deposit mechanism -- was made with game developers and players in mind.",
        styles['Body']
    ))
    story.append(Spacer(1, 16))
    story.append(hr())
    story.append(Spacer(1, 8))
    story.append(Paragraph(
        "<b>POLAY Foundation</b> | polay.io | Version 1.0 | April 2026",
        styles['Caption']
    ))
    story.append(Paragraph(
        "This document is the technical whitepaper for the POLAY blockchain. "
        "All specifications are subject to change as the protocol evolves through governance.",
        styles['Caption']
    ))

    # ── Build PDF ──
    doc.build(story, onFirstPage=add_page_number, onLaterPages=add_page_number)
    print(f"\nWhitepaper generated: {OUTPUT_PATH}")
    print(f"Pages: ~30")


if __name__ == "__main__":
    build_whitepaper()
