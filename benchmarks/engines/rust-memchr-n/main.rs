use std::io::Write;

use memchr_n::{Backend, Bytes};

use shared::{Benchmark, Sample};

fn main() -> anyhow::Result<()> {
    let mut args = vec![];
    for osarg in std::env::args_os().skip(1) {
        let Ok(arg) = osarg.into_string() else {
            anyhow::bail!("all arguments must be valid UTF-8")
        };
        args.push(arg);
    }
    anyhow::ensure!(
        !args.is_empty(),
        "Usage: runner [--quiet] (<engine-name> | --version)"
    );
    if args.iter().any(|a| a == "--version") {
        writeln!(std::io::stdout(), env!("CARGO_PKG_VERSION"))?;
        return Ok(());
    }
    let quiet = args.iter().any(|a| a == "--quiet");
    let engine = &**args.last().unwrap();
    let b = Benchmark::from_stdin()?;
    let samples = match (&*engine, &*b.model) {
        ("memchr-oneshot", "count-bytes") => memchr_oneshot_count(&b)?,
        ("memchr-prebuilt", "count-bytes") => {
            memchr_prebuilt_count(&b, Backend::Auto)?
        }
        ("memchr-scalar", "count-bytes") => {
            memchr_prebuilt_count(&b, Backend::Scalar)?
        }
        ("memchr-onlycount", "count-bytes") => {
            memchr_onlycount(&b, Backend::Auto)?
        }
        ("memchr2", "count-bytes") => memchr2_count(&b, Backend::Auto)?,
        ("memchr2-scalar", "count-bytes") => {
            memchr2_count(&b, Backend::Scalar)?
        }
        ("memchr3", "count-bytes") => memchr3_count(&b, Backend::Auto)?,
        ("memchr3-scalar", "count-bytes") => {
            memchr3_count(&b, Backend::Scalar)?
        }
        ("byteset", "count-bytes") => byteset_count(&b, Backend::Auto)?,
        ("byteset-scalar", "count-bytes") => {
            byteset_count(&b, Backend::Scalar)?
        }
        ("byteset-build", "count-bytes") => byteset_build_count(&b)?,
        ("byteset-onlycount", "count-bytes") => {
            byteset_onlycount(&b, Backend::Auto)?
        }
        ("range", "count-bytes") => range_count(&b, Backend::Auto)?,
        ("range-scalar", "count-bytes") => range_count(&b, Backend::Scalar)?,
        ("range-onlycount", "count-bytes") => {
            range_onlycount(&b, Backend::Auto)?
        }
        (engine, model) => {
            anyhow::bail!("unrecognized engine '{engine}' and model '{model}'")
        }
    };
    if !quiet {
        let mut stdout = std::io::stdout().lock();
        for s in samples.iter() {
            writeln!(stdout, "{},{}", s.duration.as_nanos(), s.count)?;
        }
    }
    Ok(())
}

/// Rebuilds the searcher on every call, which is how `memchr::memchr` and the
/// other oneshot engines behave.
fn memchr_oneshot_count(b: &Benchmark) -> anyhow::Result<Vec<Sample>> {
    let haystack = &b.haystack;
    let needle = b.one_needle_byte()?;
    shared::run(b, || {
        Ok(shared::count_memchr(haystack, needle, |h, n1| {
            Bytes::from_bytes(&[n1]).finder().find(h)
        }))
    })
}

fn memchr_prebuilt_count(
    b: &Benchmark,
    backend: Backend,
) -> anyhow::Result<Vec<Sample>> {
    let haystack = &b.haystack;
    let needle = b.one_needle_byte()?;
    let finder = Bytes::from_bytes(&[needle]).finder_with(backend);
    shared::run(b, || Ok(finder.iter(haystack).count_slow()))
}

/// Uses the specialized counting kernel, which tallies matches without
/// reporting where they are. This is the counterpart of the
/// `rust/memchr/memchr/onlycount` and `rust/bytecount/*` engines.
fn memchr_onlycount(
    b: &Benchmark,
    backend: Backend,
) -> anyhow::Result<Vec<Sample>> {
    let haystack = &b.haystack;
    let needle = b.one_needle_byte()?;
    let finder = Bytes::from_bytes(&[needle]).finder_with(backend);
    shared::run(b, || Ok(finder.iter(haystack).count()))
}

fn memchr2_count(
    b: &Benchmark,
    backend: Backend,
) -> anyhow::Result<Vec<Sample>> {
    let haystack = &b.haystack;
    let (n1, n2) = b.two_needle_bytes()?;
    let finder = Bytes::from_bytes(&[n1, n2]).finder_with(backend);
    shared::run(b, || Ok(finder.iter(haystack).count_slow()))
}

fn memchr3_count(
    b: &Benchmark,
    backend: Backend,
) -> anyhow::Result<Vec<Sample>> {
    let haystack = &b.haystack;
    let (n1, n2, n3) = b.three_needle_bytes()?;
    let finder = Bytes::from_bytes(&[n1, n2, n3]).finder_with(backend);
    shared::run(b, || Ok(finder.iter(haystack).count_slow()))
}

/// Searches for any byte in a set of arbitrary size.
///
/// Which kernel this runs is up to `memchr_n`: it picks between a shuffle over
/// one nibble, a pair of nibble lookups and a bitset probe based on the set,
/// so the benchmark definition's needles are what select the kernel.
fn byteset_count(
    b: &Benchmark,
    backend: Backend,
) -> anyhow::Result<Vec<Sample>> {
    let haystack = &b.haystack;
    let finder = needle_bytes(b)?.finder_with(backend);
    shared::run(b, || Ok(finder.iter(haystack).count_slow()))
}

/// Like [`byteset_count`], but builds the finder inside the timed region.
///
/// The byte set API has no oneshot form, so this measures one build amortized
/// over one full scan rather than a build per match. Building is where a large
/// set pays for its nibble tables, and the sets big enough to have them are
/// also the ones with too many matches for a per-match rebuild to say anything.
fn byteset_build_count(b: &Benchmark) -> anyhow::Result<Vec<Sample>> {
    let haystack = &b.haystack;
    let bytes = needle_bytes(b)?;
    shared::run(b, || Ok(bytes.finder().iter(haystack).count_slow()))
}

fn byteset_onlycount(
    b: &Benchmark,
    backend: Backend,
) -> anyhow::Result<Vec<Sample>> {
    let haystack = &b.haystack;
    let finder = needle_bytes(b)?.finder_with(backend);
    shared::run(b, || Ok(finder.iter(haystack).count()))
}

fn range_count(
    b: &Benchmark,
    backend: Backend,
) -> anyhow::Result<Vec<Sample>> {
    let haystack = &b.haystack;
    let finder = needle_range(b)?.finder_with(backend);
    shared::run(b, || Ok(finder.iter(haystack).count_slow()))
}

fn range_onlycount(
    b: &Benchmark,
    backend: Backend,
) -> anyhow::Result<Vec<Sample>> {
    let haystack = &b.haystack;
    let finder = needle_range(b)?.finder_with(backend);
    shared::run(b, || Ok(finder.iter(haystack).count()))
}

/// The set of every needle in the benchmark, each of which must be one byte.
fn needle_bytes(b: &Benchmark) -> anyhow::Result<Bytes> {
    anyhow::ensure!(!b.needles.is_empty(), "benchmark has no needles");
    let mut bytes = Bytes::new();
    for needle in b.needles.iter() {
        anyhow::ensure!(
            needle.len() == 1,
            "every needle must have length 1 (in bytes) but one has length {}",
            needle.len(),
        );
        bytes.add(needle[0]);
    }
    Ok(bytes)
}

/// The inclusive range spelled by the benchmark's single two byte needle, as
/// `<first><last>`.
fn needle_range(b: &Benchmark) -> anyhow::Result<Bytes> {
    let needle = b.one_needle()?;
    let &[first, last] = needle else {
        anyhow::bail!(
            "a range needle must have length 2 (in bytes) but it has length {}",
            needle.len(),
        )
    };
    anyhow::ensure!(
        first <= last,
        "range needle {first:#04x}..={last:#04x} is empty",
    );
    Ok(Bytes::from_range(first..=last))
}

trait IteratorExt: Iterator {
    /// Like `Iterator::count`, but guarantees that it gets the count by
    /// iterating over each element without taking any specialized shortcuts.
    ///
    /// We do this because `memchr_n` specializes `count` to a kernel that only
    /// tallies matches, and we'd generally like to measure how long it takes to
    /// find all occurrences of a needle and not just the number of them.
    fn count_slow(mut self) -> usize
    where
        Self: Sized,
    {
        let mut count = 0;
        while let Some(_) = self.next() {
            count += 1;
        }
        count
    }
}

impl<I: Iterator> IteratorExt for I {}
