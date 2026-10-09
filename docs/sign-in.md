# Sign-in

This is the sign-in modal (phase 11 of `docs/ROADMAP.md`). The code is in `src/app/sign_in*` (state machine, steps, saving) and `src/ui/shell/sign_in_gate.rs` (the card).

## Principle

At launch the app shows the frame of the dashboard drawn as soft blocks (`ui::shell::locked_frame`,
no data, no behavior), under a veil, with a modal on top that cannot be closed until there is a
session. The real dashboard needs a session to exist, so the frame is a drawing of it. The background is inert: no clicks, no
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

- `app::sign_in::state`: the phases and failures as a pure state machine, with unit tests.
- `app::sign_in`: the `SignIn` entity runs the network steps, saves the profile, and emits
  `SignInEvent::Connected`. A cancel bumps an attempt number and aborts the wait for the redirect,
  so the port is free at once.
- `ui::shell::sign_in_gate`: the card. Escape is the `Dismiss` action of the `SignInGate` context.
- `ui::shell::app_view`: the window root; opens the session and the dashboard, and brings the
  modal back when the session ends. A settings folder that cannot be opened shows a card with a
  Quit button instead of a panic.
- The saved session is read after the first frame. The keyring is read on a background thread
  (`SavedPlan::read`): the profile is looked up first, and only the secret reader and the token
  storage, which are `Send`, move to the other thread.
