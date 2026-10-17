---
paths:
  - "crates/wyck-ctrader/**"
---

# cTrader client

- This is the only crate that talks to the network. Tokens come in as arguments and go out as
  values; nothing here reads a file or the keyring.
- Payload type numbers and field names follow the official Open API. A test pins each one; do not
  change a number to make something compile.
- A trading request needs the proof the guard made for it and for the connection's `Mode`.
- A timeout is not a refusal: do not resend an order after one.
- Add a call only when the app needs it: a request type, a constant, a client method, a test.
