pub async fn select_first<T: Clone>(items: &[T]) -> Option<T> {
    items.first().cloned()
}
