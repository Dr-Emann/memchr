use std::io::Write;

use memchr_n::{Backend, Bitset};

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
        ("memchr2", "count-bytes") => memchr2_count(&b, Backend::Auto)?,
        ("memchr2-scalar", "count-bytes") => {
            memchr2_count(&b, Backend::Scalar)?
        }
        ("memchr3", "count-bytes") => memchr3_count(&b, Backend::Auto)?,
        ("memchr3-scalar", "count-bytes") => {
            memchr3_count(&b, Backend::Scalar)?
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
            Bitset::from_bytes(&[n1]).finder().find(h)
        }))
    })
}

fn memchr_prebuilt_count(
    b: &Benchmark,
    backend: Backend,
) -> anyhow::Result<Vec<Sample>> {
    let haystack = &b.haystack;
    let needle = b.one_needle_byte()?;
    let finder = Bitset::from_bytes(&[needle]).finder_with(backend);
    shared::run(b, || Ok(finder.iter(haystack).count_slow()))
}

fn memchr2_count(
    b: &Benchmark,
    backend: Backend,
) -> anyhow::Result<Vec<Sample>> {
    let haystack = &b.haystack;
    let (n1, n2) = b.two_needle_bytes()?;
    let finder = Bitset::from_bytes(&[n1, n2]).finder_with(backend);
    shared::run(b, || Ok(finder.iter(haystack).count_slow()))
}

fn memchr3_count(
    b: &Benchmark,
    backend: Backend,
) -> anyhow::Result<Vec<Sample>> {
    let haystack = &b.haystack;
    let (n1, n2, n3) = b.three_needle_bytes()?;
    let finder = Bitset::from_bytes(&[n1, n2, n3]).finder_with(backend);
    shared::run(b, || Ok(finder.iter(haystack).count_slow()))
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
