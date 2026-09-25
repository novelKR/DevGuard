//! Bounded workloads for the load interval. Each runs as a managed payload
//! under `devguard exec`, inside the budget its admission reserved, and
//! none creates its own process group or session.

use std::fs::File;
use std::io::{BufRead, Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

const MIB: usize = 1024 * 1024;
/// Touch memory at this stride; it is at most the smallest page size.
const STRIDE: usize = 4096;

/// Keep `threads` threads computing for `duration`. Returns the rounds done.
pub fn cpu(threads: usize, duration: Duration) -> u64 {
    let deadline = Instant::now() + duration;
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads.max(1))
            .map(|seed| scope.spawn(move || spin(seed as u64, deadline)))
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap_or(0))
            .sum()
    })
}

fn spin(seed: u64, deadline: Instant) -> u64 {
    let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    let mut rounds = 0;
    while Instant::now() < deadline {
        for _ in 0..100_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
        }
        rounds += 1;
    }
    std::hint::black_box(state);
    rounds
}

/// Hold `mib` MiB with every page written, rewriting them each second so
/// they stay resident, for `duration`. Returns the passes over the block.
pub fn memory(mib: usize, duration: Duration) -> u64 {
    let deadline = Instant::now() + duration;
    let mut block = vec![0u8; mib * MIB];
    let mut passes = 0;
    loop {
        for page in block.chunks_mut(STRIDE) {
            page[0] = page[0].wrapping_add(1);
        }
        passes += 1;
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        std::thread::sleep((deadline - now).min(Duration::from_secs(1)));
    }
    std::hint::black_box(&block);
    passes
}

/// Write `mib` MiB to a new file in `directory` in 1 MiB blocks, sync it,
/// read it back and check every block, then remove it. Returns the bytes
/// verified.
pub fn io(directory: &Path, mib: usize) -> std::io::Result<u64> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let path = directory.join(format!("io-{}-{nanos}.bin", std::process::id()));
    let result = write_and_verify(&path, mib);
    let removed = std::fs::remove_file(&path);
    let verified = result?;
    removed?;
    Ok(verified)
}

/// Like `io`, then idle until `duration` has passed since it began, so a
/// consumer's I/O stays at a bounded average rate.
pub fn io_paced(directory: &Path, mib: usize, duration: Duration) -> std::io::Result<u64> {
    let started = Instant::now();
    let verified = io(directory, mib)?;
    if let Some(rest) = duration.checked_sub(started.elapsed()) {
        std::thread::sleep(rest);
    }
    Ok(verified)
}

fn block(index: usize) -> Vec<u8> {
    (0..MIB)
        .map(|offset| (offset.wrapping_mul(31) ^ index) as u8)
        .collect()
}

fn write_and_verify(path: &Path, mib: usize) -> std::io::Result<u64> {
    let mut file = File::create_new(path)?;
    for index in 0..mib {
        file.write_all(&block(index))?;
    }
    file.sync_all()?;
    drop(file);
    let mut file = File::open(path)?;
    let mut buffer = vec![0u8; MIB];
    let mut verified = 0;
    for index in 0..mib {
        file.read_exact(&mut buffer)?;
        if buffer != block(index) {
            return Err(std::io::Error::other("a block read back differs"));
        }
        verified += MIB as u64;
    }
    Ok(verified)
}

/// Write `mib` MiB of text lines to `out`, spread evenly over `duration`
/// (as fast as `out` accepts them when it is zero). Returns the bytes written.
pub fn output(mib: usize, duration: Duration, out: &mut impl Write) -> std::io::Result<u64> {
    const LINE: &[u8] = b"devguard output pressure: 0123456789 abcdefghijklmnopqrstuvwxyz\n";
    const CHUNK: u64 = 64 * 1024;
    let started = Instant::now();
    let limit = (mib * MIB) as u64;
    let mut written = 0;
    while written < limit {
        out.write_all(LINE)?;
        written += LINE.len() as u64;
        if written % CHUNK < LINE.len() as u64 {
            out.flush()?;
            // Keep to the average rate: wait until this share of the duration has passed.
            let due = duration.mul_f64(written as f64 / limit as f64);
            if let Some(rest) = due.checked_sub(started.elapsed()) {
                std::thread::sleep(rest);
            }
        }
    }
    out.flush()?;
    Ok(written)
}

/// Read `input` to its end one line at a time. Returns the lines read.
pub fn slow_reader(input: impl BufRead) -> std::io::Result<u64> {
    let mut lines = 0;
    for line in input.lines() {
        line?;
        lines += 1;
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_work_runs_every_thread_until_its_deadline() {
        let started = Instant::now();
        assert!(cpu(2, Duration::from_millis(100)) >= 2);
        assert!(started.elapsed() >= Duration::from_millis(100));
    }

    #[test]
    fn memory_work_writes_every_page_at_least_once() {
        assert!(memory(4, Duration::ZERO) >= 1);
    }

    #[test]
    fn io_work_verifies_what_it_wrote_and_removes_it() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(io(directory.path(), 3).unwrap(), 3 * MIB as u64);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn paced_io_work_lasts_its_duration() {
        let directory = tempfile::tempdir().unwrap();
        let started = Instant::now();
        assert_eq!(
            io_paced(directory.path(), 1, Duration::from_millis(200)).unwrap(),
            MIB as u64
        );
        assert!(started.elapsed() >= Duration::from_millis(200));
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn io_work_refuses_a_missing_directory() {
        let directory = tempfile::tempdir().unwrap();
        assert!(io(&directory.path().join("missing"), 1).is_err());
    }

    #[test]
    fn output_work_writes_at_least_the_requested_size() {
        let mut sink = Vec::new();
        let written = output(1, Duration::ZERO, &mut sink).unwrap();
        assert_eq!(written, sink.len() as u64);
        assert!(written >= MIB as u64);
    }

    #[test]
    fn paced_output_work_spreads_over_its_duration() {
        let mut sink = Vec::new();
        let started = Instant::now();
        output(1, Duration::from_millis(300), &mut sink).unwrap();
        assert!(started.elapsed() >= Duration::from_millis(250));
        assert!(sink.len() >= MIB);
    }

    #[test]
    fn slow_reader_counts_lines_to_the_end() {
        assert_eq!(slow_reader(&b"a\nb\nc\n"[..]).unwrap(), 3);
        assert_eq!(slow_reader(&b""[..]).unwrap(), 0);
    }
}
