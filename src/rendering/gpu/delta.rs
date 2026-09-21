//! Delta uploads for per-frame GPU buffers.
//!
//! The graph renderer rebuilds its instance/vertex arrays every frame, but most
//! frames (idle, hover, animation of a single preview) leave nearly all of that
//! data unchanged. `delta_write` keeps a CPU shadow of what the GPU buffer
//! holds and issues `write_buffer` only for the chunks that differ.

/// Comparison granularity. Multiple of `wgpu::COPY_BUFFER_ALIGNMENT`.
const CHUNK: usize = 64;

/// Byte ranges of `bytes` that differ from `shadow`, coalesced. Anything past the
/// end of `shadow` counts as changed.
fn changed_ranges(shadow: &[u8], bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut off = 0;
    while off < bytes.len() {
        let end = (off + CHUNK).min(bytes.len());
        let unchanged = shadow.get(off..end).is_some_and(|s| s == &bytes[off..end]);
        if !unchanged {
            match ranges.last_mut() {
                Some(last) if last.1 == off => last.1 = end,
                _ => ranges.push((off, end)),
            }
        }
        off = end;
    }
    ranges
}

/// Upload `bytes` into `buf`, writing only the regions that differ from
/// `shadow` (the last uploaded contents). Pass `reset = true` when the buffer
/// was just (re)allocated so everything is written.
pub fn delta_write(
    queue: &wgpu::Queue,
    buf: &wgpu::Buffer,
    shadow: &mut Vec<u8>,
    bytes: &[u8],
    reset: bool,
) {
    if reset {
        shadow.clear();
    }
    let align = wgpu::COPY_BUFFER_ALIGNMENT as usize;
    for (start, end) in changed_ranges(shadow, bytes) {
        if (end - start) % align == 0 {
            queue.write_buffer(buf, start as u64, &bytes[start..end]);
        } else {
            // Unaligned tail (never produced by the 4-byte-multiple structs
            // uploaded here): fall back to a full write rather than a bad size.
            queue.write_buffer(buf, 0, bytes);
            break;
        }
    }
    shadow.clear();
    shadow.extend_from_slice(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_data_writes_nothing() {
        let a = vec![7u8; 300];
        assert!(changed_ranges(&a, &a).is_empty());
    }

    #[test]
    fn empty_shadow_writes_everything_as_one_range() {
        let a = vec![1u8; 200];
        assert_eq!(changed_ranges(&[], &a), vec![(0, 200)]);
    }

    #[test]
    fn single_changed_chunk_is_isolated() {
        let old = vec![0u8; 256];
        let mut new = old.clone();
        new[130] = 9; // chunk 2 (128..192)
        assert_eq!(changed_ranges(&old, &new), vec![(128, 192)]);
    }

    #[test]
    fn adjacent_changes_coalesce_and_gaps_split() {
        let old = vec![0u8; 320];
        let mut new = old.clone();
        new[0] = 1;
        new[70] = 1; // chunks 0 and 1 -> one range
        new[300] = 1; // chunk 4 -> separate range
        assert_eq!(changed_ranges(&old, &new), vec![(0, 128), (256, 320)]);
    }

    #[test]
    fn growth_writes_only_the_new_tail() {
        let old = vec![5u8; 128];
        let mut new = old.clone();
        new.extend_from_slice(&[6u8; 64]);
        assert_eq!(changed_ranges(&old, &new), vec![(128, 192)]);
    }
}
