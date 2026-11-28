use crate::value::Value;
use std::cell::RefCell;
use std::rc::Rc;

enum Edit {
    Pushed(Rc<RefCell<Vec<Value>>>),
    Replaced {
        list: Rc<RefCell<Vec<Value>>>,
        index: usize,
        previous: Value,
    },
}

#[derive(Default)]
pub(crate) struct Journal {
    edits: Vec<Edit>,
}

impl Journal {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    pub(crate) fn record_push(&mut self, list: &Rc<RefCell<Vec<Value>>>) {
        self.edits.push(Edit::Pushed(Rc::clone(list)));
    }

    pub(crate) fn record_replace(
        &mut self,
        list: &Rc<RefCell<Vec<Value>>>,
        index: usize,
        previous: Value,
    ) {
        self.edits.push(Edit::Replaced {
            list: Rc::clone(list),
            index,
            previous,
        });
    }

    pub(crate) fn rewind(&mut self) {
        for edit in self.edits.drain(..).rev() {
            match edit {
                Edit::Pushed(list) => {
                    let taken = list.borrow_mut().pop();
                    assert!(
                        taken.is_some(),
                        "a rewound push takes back the item it recorded",
                    );
                }
                Edit::Replaced {
                    list,
                    index,
                    previous,
                } => {
                    let mut items = list.borrow_mut();
                    assert!(
                        index < items.len(),
                        "a rewound replace writes the item it recorded",
                    );
                    items[index] = previous;
                }
            }
        }
    }

    pub(crate) fn commit(&mut self) {
        self.edits.clear();
    }
}
