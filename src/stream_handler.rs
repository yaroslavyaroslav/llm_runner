use std::sync::Arc;

use tokio::sync::mpsc::Receiver;

#[derive(Debug)]
pub struct StreamHandler {}

impl StreamHandler {
    pub async fn handle_stream_with(
        mut rx: Receiver<String>,
        emit_fn: Arc<dyn Fn(String) + Send + Sync + 'static>,
    ) {
        while let Some(data) = rx.recv().await {
            emit_fn(data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_sync_and_send() {
        fn is_sync<T: Sync>() {}
        fn is_send<T: Send>() {}

        is_sync::<StreamHandler>();
        is_send::<StreamHandler>();
    }
}
