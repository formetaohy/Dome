use dome_shader::{AtomicU32, ReadWrite};

const ATOMIC_I_ADD: u16 = 234;
const ATOMIC_STORE: u16 = 228;
const CONSTANT: u16 = 43;
const UNIFORM_MEMORY: u32 = 64;

const _: Option<ReadWrite<AtomicU32>> = None;

#[dome_shader::kernel(workgroup_size = 64)]
fn publish(lid: u32, sources: Read<f32>, counts: ReadWrite<AtomicU32>, keys: ReadWrite<AtomicU32>) {
    let held = sources[0u32];
    let mut first = 0u32;
    if held > 0.0 {
        first = 1u32;
    }
    let last = atomic_add(&counts[0u32], 1u32);
    let first = min(first, last);
    let second = min(first, last);
    if second < 32u32 {
        atomic_store(&keys[second], lid);
    }
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

fn semantics(words: &[u32], op: u16) -> Vec<u32> {
    words
        .iter()
        .enumerate()
        .filter(|(_, word)| opcode(**word) == op)
        .map(|(at, _)| {
            constant(words, words[at + 3]).unwrap_or_else(|| {
                panic!("an atomic instruction names its semantics through a constant")
            })
        })
        .collect()
}

#[test]
fn an_atomic_store_publishes_through_an_atomic_instruction() {
    let program = publish();
    let words = program.spirv();
    assert!(
        words.iter().any(|word| opcode(*word) == ATOMIC_I_ADD),
        "a counter reaches Vulkan as OpAtomicIAdd",
    );
    let stores = semantics(words, ATOMIC_STORE);
    assert_eq!(
        stores.len(),
        1,
        "a store to an atomic cell reaches Vulkan as one OpAtomicStore",
    );
    for store in stores {
        assert!(
            store & UNIFORM_MEMORY != 0,
            "a non relaxed atomic store names its storage class, not only its order",
        );
    }
}
