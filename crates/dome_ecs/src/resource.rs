pub trait Resource: Send + Sync + 'static {}

impl<T: Send + Sync + 'static> Resource for T {}
