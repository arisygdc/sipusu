/// implementation for cleaning memory for unsafe pointer
pub trait Cleanup {
    async fn clear(self);
}