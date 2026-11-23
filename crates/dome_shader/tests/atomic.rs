use dome_shader::{AtomicU32, ReadWrite};

const ATOMIC_STORE: u32 = 228;

const _: Option<ReadWrite<AtomicU32>> = None;

#[dome_shader::kernel(workgroup_size = 64)]
fn publish(lid: u32, keys: ReadWrite<AtomicU32>) {
    atomic_store(&keys[lid], lid);
}

#[test]
fn an_atomic_store_publishes_through_an_atomic_instruction() {
    let program = publish();
    assert!(
        program
            .spirv()
            .iter()
            .any(|word| word & 0xffff == ATOMIC_STORE),
        "a store to an atomic cell reaches Vulkan as OpAtomicStore",
    );
}
