# cTrader client

The client lives in `src/infra/ctrader/`. It speaks the cTrader Open API over its JSON WebSocket.

## Flow

1. Authorize the application (`ProtoOAApplicationAuthReq`) with the client id and secret.
2. Authorize each trading account (`ProtoOAAccountAuthReq`) with an OAuth access token.
3. Send requests and match answers by `clientMsgId` (a UUID per request).
4. Receive events (spots, executions, margin, disconnects) on a broadcast channel.

## Sign-in (OAuth 2)

- `auth::oauth` builds the consent URL, exchanges the code and refreshes tokens.
- `auth::callback` listens on `127.0.0.1:8765` for the redirect (5 minutes at most). The redirect
  URI must be registered in the cTrader application.
- The access token lasts about 30 days. The refresh token is replaced each time it is used: the
  new pair must be saved before it is used.
- Tokens and the client secret live in the OS keyring through `infra::storage`.

## Session

`session::Session` keeps one account connected:

- Heartbeat every 5 seconds.
- Reconnect with exponential backoff (1 s doubling to 60 s, up to 20 % jitter).
- Spot, live bar and depth subscriptions are restored after each reconnect.
- `AccountDisconnectEvent` re-authorizes the account on the same connection;
  `ClientDisconnectEvent` reconnects; an invalidated token refreshes and reconnects.

## Limits

- The transport spaces requests: 40 per second in general and 4 per second for history, under the
  documented 50 and 5.
- `REQUEST_FREQUENCY_EXCEEDED` and similar answers retry up to 3 times after `retryAfter`.

## Protocol quirks worth knowing

- A live bar's close equals its low (found on a live account): see `market::live`.
- Tick times and prices come delta-encoded, newest first: see `market::ticks`.
- A rejected order is returned as an `Ok` execution event whose kind says it was rejected.

## Known gaps

Tracked in `docs/ROADMAP.md`, phase 13: no silence watchdog on a half-open socket, no scheduled
token refresh while connected, a transient token endpoint failure ends the session, the reconnect
backoff resets after any successful connect.
