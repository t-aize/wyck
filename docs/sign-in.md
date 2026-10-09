# Sign-in

This is the design of the sign-in modal (phase 11 of `docs/ROADMAP.md`).

## Principle

At launch the app shows the real dashboard frame without data, softened by a veil, with a modal
on top that cannot be closed until there is a session. The background is inert: no clicks, no
shortcuts, no focus. One modal with states replaces the old welcome, credentials, browser,
account and authorizing screens. The same modal returns, prefilled, when a session is no longer
valid.

## Blur

GPUI has no blur of the app's own content. Its scene has eight primitives (shadow, quad, path,
underline, three sprite kinds, surface) and `blur_radius` exists only on shadows.
`WindowBackgroundAppearance::Blurred` blurs the desktop behind the window, not the content. The
substitute: the dashboard frame with skeleton blocks, a veil at 70 to 80 percent of the
background color, a wide shadow on the modal, and a fade in and out.

## Form

- Environment: Demo or Live (Live shows a warning badge).
- Client ID, focused on open.
- Client Secret: masked, reveal toggle, no copy or cut of the real text.
- "Stay signed in on this device" (on by default; off keeps tokens in memory only).
- A collapsed help block with the redirect URI to register and a copy button.
- Footer: Quit Wyck (secondary), Sign in (primary, large). Enter submits.

## States

| State | What the user sees | Next |
|---|---|---|
| Restoring | "Restoring your session", keyring read off the UI thread | Connected, or Editing |
| Editing | the form, prefilled when a profile exists | Verifying |
| Verifying | fields disabled, button loading, Cancel | WaitingBrowser or Failed |
| WaitingBrowser | explanation, reopen and copy link, a 5 minute countdown, Cancel (frees the port at once) | ChoosingAccount, Authorizing, Editing |
| ChoosingAccount | accounts of the chosen environment only | Authorizing |
| Authorizing | short wait | Connected or Failed |
| Failed | a typed banner above the form, fields kept, Retry | Editing |
| Connected | check mark, then the modal and the veil fade out, focus goes to the app | |

Failure kinds, each with its own message: credentials rejected, network unreachable, consent
denied in the browser, timeout, port 8765 busy, no account for this environment, keyring
unavailable (fallback: session in memory with a warning).

## Closing

- Escape and a click on the veil do nothing but a short shake. During verification Escape cancels
  the verification, not the modal.
- No close button. Quit Wyck or the system window button close the app without confirmation.
- Global shortcuts are off while locked.
- A lost connection is not a sign-out: a quiet banner and automatic reconnection. The modal comes
  back only when the session is invalid (revoked or refused refresh).

## Code

The state machine is a pure enum in `app` with unit tests. The view is in `ui::shell`.
