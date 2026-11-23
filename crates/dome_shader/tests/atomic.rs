use dome_shader::{AtomicU32, ReadWrite};

const ATOMIC_STORE: u16 = 228;
const CONSTANT: u16 = 43;
const UNIFORM_MEMORY: u32 = 64;

const _: Option<ReadWrite<AtomicU32>> = None;

#[dome_shader::kernel(workgroup_size = 64)]
fn publish(lid: u32, keys: ReadWrite<AtomicU32>) {
    atomic_store(&keys[lid], lid);
}

fn opcode(word: u32) -> u16 {
    (word & 0xffff) as u16
}

fn constant(words: &[u32], id: u32) -> Option<u32> {
    words
        .iter()
        .enumerate()
        .find(|(at, word)| opcode(**word) == CONSTANT && words.get(at + 2).copied() == Some(id))
        .map(|(at, _)| words[at + 3])
}

#[test]
fn an_atomic_store_publishes_through_an_atomic_instruction() {
    let program = publish();
    let words = program.spirv();
    let store = words
        .iter()
        .position(|word| word & 0xffff == u32::from(ATOMIC_STORE))
        .expect("a store to an atomic cell reaches Vulkan as OpAtomicStore");
    let semantics = words[store + 3];
    let semantics = constant(words, semantics)
        .expect("an atomic store names its memory semantics through a constant");
    assert!(
        semantics & UNIFORM_MEMORY != 0,
        "a non relaxed atomic store names its storage class, not only its order",
    );
}
