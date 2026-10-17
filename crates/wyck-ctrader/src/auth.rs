//! OAuth 2: the consent URL, tokens, refresh, and the local redirect listener.
//!
//! | Module | Role |
//! |---|---|
//! | `oauth` | [`OAuthClient`], [`TokenSet`], [`authorization_url`], [`Scope`] |
//! | `callback` | [`CallbackListener`]: the loopback web server that catches the redirect |

mod callback;
mod oauth;

pub use callback::CallbackListener;
#[cfg(test)]
pub(crate) use oauth::parse_token_response;
pub use oauth::{OAuthClient, Scope, TokenSet, authorization_url, new_state};
