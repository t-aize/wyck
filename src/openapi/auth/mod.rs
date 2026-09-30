//! OAuth 2: the consent URL, tokens, refresh, and the local redirect listener.

pub mod callback;
pub mod oauth;

pub use callback::{AuthorizationCode, CallbackListener};
pub use oauth::{
    AUTHORIZE_URL, OAuthClient, Scope, TOKEN_URL, TokenSet, authorization_url, new_state,
    parse_token_response,
};
