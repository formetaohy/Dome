#[derive(Clone, Debug)]
pub(crate) enum Action {
    Open(usize),
    Forget(usize),
    Choose,
    EditPath(String),
    SubmitPath(String),
    Submit,
    CloseDialog,
    DismissError,
}
