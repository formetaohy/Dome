pub trait Clipboard {
    fn read(&mut self) -> Option<String>;
    fn write(&mut self, text: &str);
}

pub struct NullClipboard;

impl Clipboard for NullClipboard {
    fn read(&mut self) -> Option<String> {
        None
    }

    fn write(&mut self, _text: &str) {}
}

#[derive(Default)]
pub struct MemoryClipboard {
    text: Option<String>,
}

impl Clipboard for MemoryClipboard {
    fn read(&mut self) -> Option<String> {
        self.text.clone()
    }

    fn write(&mut self, text: &str) {
        self.text = Some(String::from(text));
    }
}
