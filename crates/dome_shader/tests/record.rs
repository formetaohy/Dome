use dome_shader::Read;
use dome_shader::layout::{FieldLayout, FieldType, Record, RecordLayout};

struct TestRecord {
    value: u32,
}

impl Record for TestRecord {
    const LAYOUT: RecordLayout = RecordLayout {
        name: "Test",
        size: 4,
        fields: &[FieldLayout {
            name: "value",
            offset: 0,
            ty: FieldType::U32,
        }],
    };
}

const _: Option<Read<TestRecord>> = None;

#[dome_shader::kernel(workgroup_size = 64)]
fn run(lid: u32, data: Read<TestRecord>) {
    let value = data[lid].value;
    let _witness = value;
}

#[test]
fn a_record_binding_declares_its_layout() {
    assert_eq!(<TestRecord as Record>::LAYOUT.name, "Test");
    let _ = run();
}
