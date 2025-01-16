use tokio::sync::mpsc::Receiver;

#[derive(Debug)]
pub struct StreamHandler {
    rx: Receiver<String>,
}

impl StreamHandler {
    pub fn new(rx: Receiver<String>) -> Self { StreamHandler { rx } }

    pub async fn handle_stream_with<F>(&mut self, mut emit_fn: F)
    where F: FnMut(String) + Send + 'static {
        while let Some(data) = self.rx.recv().await {
            emit_fn(data);
        }
    }
}
