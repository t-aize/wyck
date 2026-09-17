# wyck

**A terminal trading panel for cTrader — built for speed, not for staring at it.**

![License](https://img.shields.io/badge/license-Apache--2.0-blue)
![Status](https://img.shields.io/badge/status-early%20development-orange)
![Rust](https://img.shields.io/badge/rust-2024%20edition-orange)

> 🚧 **Early development.** wyck is not yet functional end-to-end. This README describes
> the design and the intended feature set — see [Roadmap](#roadmap) for what's actually done.

> 🌐 **Domain claim, for the record:** `wyck.sh`. First-come-first-served does not apply
> retroactively — future me, don't let someone else grab it just because past me was busy
> writing a README instead of buying a domain.

## Table of contents

- [Why wyck](#why-wyck)
- [Features](#features)
- [Status](#status)
- [Installation](#installation)
- [Configuration](#configuration)
- [Usage](#usage)
- [Architecture](#architecture)
- [Roadmap](#roadmap)
- [Disclaimer](#disclaimer)
- [License](#license)

## Why wyck

cTrader already has fast market-order buttons and hotkeys — pressing "buy" isn't the
problem. What it doesn't have natively is risk-based position sizing: turning a stop-loss
distance and a risk percentage into the correct lot size. That gap is real enough that a
whole marketplace of third-party cBots exists just to patch it inside cTrader's own charts.

wyck exists to close that gap in one hotkey: set a stop loss and a risk amount, it
calculates the correct lot size and sends the order — no manual math under pressure while
scalping, and (eventually) the same flow across more than one broker instead of being stuck
inside cTrader's own chart the way a cBot is.

Two design decisions follow directly from that goal:

- **No LLM in the execution path.** wyck talks to [cTrader's MCP server](https://mcp.spotware.com/)
  with structured, direct tool calls — buy, sell, close, modify. A language model is a great
  tool for analysis and automation, but it has no place between a hotkey and a live order;
  the added latency and interpretation risk aren't worth it for scalping.
- **Keyboard first.** Every trading action is a hotkey. The mouse is for TradingView.

## Features

- Risk-based position sizing — set a stop loss and a risk %/amount, wyck computes the lot
  size and fires the order in one hotkey
- Terminal UI (`ratatui` + `crossterm`) — lightweight, always-on-top of your workflow,
  no context switch to place a trade
- Direct connection to cTrader's MCP server over streamable HTTP + SSE (`rmcp`)
- Live account, position and P&L view
- Hotkey-driven market/limit/stop orders and position management
- Async core (`tokio`) so market data streaming never blocks order execution
- File-based logging (`tracing`) — stdout is reserved for the terminal UI itself
- OS-standard config/credential locations (`directories`)

## Status

wyck is a from-scratch project, currently at the scaffolding stage (dependencies and
project layout in place; no working client yet). It's being built to a "trade with it in a
few months" timeline first, with a more ambitious multi-client architecture planned as a
longer-running effort once the basics are proven by actual use.

## Installation

**Prerequisites:** Rust 1.98+ (2024 edition).

```bash
git clone https://github.com/t-aize/wyck.git
cd wyck
cargo build --release
```

The binary is produced at `target/release/wyck`.

## Configuration

wyck connects to cTrader through its official MCP server. You'll need:

- A cTrader account with [AI Agent Connect](https://mcp.spotware.com/) enabled
- MCP connection details (remote MCP endpoint, or a local MCP server running alongside
  cTrader Windows/Mac), copied from cTrader into wyck's config

Config and credentials are stored in the platform's standard application directory
(via the `directories` crate) rather than in the repo or the working directory.

> Exact config file format and CLI flags are still in flux — this section will be filled in
> as the first working version lands.

## Usage

```bash
wyck
```

Planned default keybindings (subject to change):

| Key       | Action                          |
| --------- | -------------------------------- |
| `b`       | Buy at market                    |
| `s`       | Sell at market                   |
| `c`       | Close current position           |
| `↑` / `↓` | Adjust position size             |
| `[` / `]` | Adjust stop loss / take profit   |
| `Tab`     | Cycle instrument                 |
| `q`       | Quit                             |

## Architecture

Today, wyck is a single binary that talks to the cTrader MCP directly — simple on purpose,
so there's something usable to trade with soon.

The long-term target (v1) is a zeron-style split: pull the MCP client, order execution and
position state into a small headless **engine**, talk to it over a typed RPC, and let the
terminal UI become just one client among possibly several (a future richer GUI, a headless
mode for an always-on box, etc.) without touching the trading logic. Nothing about the
current version needs to change for this to happen later — it's an incremental split, not a
rewrite.

Full customizability is part of that vision too — themes, layout, panels, the works, the way
zeron treats its whole UI as its own rather than inheriting someone else's chrome. That
extends to charting itself: multiple customizable charts laid out however you want, custom
indicators, and saved templates/settings you can switch between, rather than one fixed chart
view. Scope-wise, v1 isn't meant to stop at a single cTrader token: a real account-management
panel (multiple cTrader accounts, and eventually other brokers/platforms), futures and
order-flow support, aiming at something that covers the ground TradingView, a depth/order-flow
tool, Tradovate and Quantower each cover separately — modern, fully customizable, open source
end to end. That's the ambitious version of this project; the TUI above is step one toward it,
not a separate thing.

The final goal also folds in the parts most terminals leave as separate apps: backtesting
against historical data, a trade journal (entries, exits, notes, running stats over time),
and an economic calendar with real filtering — by impact, currency, custom watchlist —
instead of a wall of every release on earth. The point is genuinely to take what the biggest
trading terminals and apps each do well and bring it under one roof, instead of juggling five
different tools for five different jobs.

## Roadmap

### TUI (current)

- [ ] Connect and authenticate to the cTrader MCP server
- [ ] Risk-based lot size calculation (stop loss + risk % → lot size)
- [ ] Live account, position and P&L view
- [ ] Hotkey market order execution
- [ ] Limit/stop orders and position modification
- [ ] Config file + credential storage
- [ ] Structured logging to file
- [ ] TradingView alert webhook ingestion (optional, for semi-systematic setups)

### v1 (Cargo workspace, engine/UI split)

- [ ] Engine / UI split (headless engine + RPC, TUI as first client)
- [ ] Multi-account (token) management — hold and switch between several cTrader accounts,
      not just the single token the TUI targets today
- [ ] Multi-order management — track and act on multiple concurrent positions/orders across
      accounts, not just one at a time
- [ ] Backtesting engine — run a strategy or ruleset against historical data
- [ ] Trade journal — entries, exits, notes, and running stats over time
- [ ] Economic calendar with real filters (impact, currency, custom watchlist)
- [ ] Everything that follows from the above (per-account state, routing orders to the right
      account, aggregated multi-account P&L, etc.)

## Disclaimer

wyck places real orders on a real trading account. It is a personal tool, provided as-is,
with no guarantee of correctness. It does not provide financial, investment, legal or tax
advice. You are solely responsible for verifying its behavior, securing your credentials,
and any trading decisions and losses that result from using it.

The MCP token wyck connects with is scoped to a single cTrader account — demo or live,
prop-firm or not. It can be used against a demo account for testing, but wyck has no
awareness of which kind of account it's pointed at, so double-check your token before
running anything against a funded account.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
