pub trait Component: Send + Sync + 'static {}

impl<T: Send + Sync + 'static> Component for T {}
