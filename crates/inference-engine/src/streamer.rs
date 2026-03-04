use tokio::sync::mpsc;

use crate::backend::Token;

/// Create a token streaming channel with the given buffer size.
pub fn token_channel(buffer: usize) -> (mpsc::Sender<Token>, mpsc::Receiver<Token>) {
    mpsc::channel(buffer)
}

/// Collect all tokens from a receiver into a single string.
pub async fn collect_tokens(mut rx: mpsc::Receiver<Token>) -> String {
    let mut result = String::new();
    while let Some(token) = rx.recv().await {
        result.push_str(&token.text);
    }
    result
}
