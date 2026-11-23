pub(crate) fn blocks(alignment: u64, stride: u64) -> u32 {
    assert!(
        alignment > 0 && stride > 0,
        "a span of {stride} bytes at a {alignment} byte alignment names no block",
    );
    u32::try_from(alignment / gcd(alignment, stride))
        .expect("a device alignment fits a record count")
}

pub(crate) fn padded(alignment: u64, bytes: u64) -> u64 {
    bytes.next_multiple_of(alignment.max(1))
}

fn gcd(left: u64, right: u64) -> u64 {
    if right == 0 {
        left
    } else {
        gcd(right, left % right)
    }
}
