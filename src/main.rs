use std::{
    error::Error,
    iter::{empty, once},
};

use clap::Parser as ClapParser;

use regex_syntax::{
    Parser,
    hir::{Class::*, Hir, HirKind::*},
};

struct MultiCartesianProduct<I, F>
where
    I: Iterator,
    F: Fn() -> I,
{
    factories: Vec<F>,
    iters: Vec<I>,
    heads: Vec<I::Item>,
    done: bool,
}

impl<I, F> MultiCartesianProduct<I, F>
where
    I: Iterator,
    F: Fn() -> I,
{
    fn new(factories: Vec<F>) -> Self {
        let mut iters: Vec<I> = factories.iter().map(|f| (f)()).collect();
        let mut heads = Vec::new();
        let mut done = false;
        for iter in &mut iters {
            if let Some(head) = iter.next() {
                heads.push(head);
            } else {
                done = true;
                break;
            }
        }
        Self {
            factories,
            iters,
            heads,
            done,
        }
    }
}

impl<I, F> Iterator for MultiCartesianProduct<I, F>
where
    I: Iterator,
    I::Item: Clone,
    F: Fn() -> I,
{
    type Item = Vec<I::Item>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        if self.factories.is_empty() {
            self.done = true;
            return Some(Vec::new());
        }
        let result = self.heads.clone();
        for ((head, iter), factory) in self
            .heads
            .iter_mut()
            .zip(&mut self.iters)
            .zip(&self.factories)
        {
            if let Some(next) = iter.next() {
                *head = next;
                return Some(result);
            } else {
                *iter = (factory)();
                *head = iter.next().unwrap();
            }
        }
        self.done = true;
        Some(result)
    }
}

#[test]
fn test_cartesian() {
    for item in MultiCartesianProduct::new(vec![
        || ['a', 'b'].into_iter(),
        || ['f', 'g'].into_iter(),
        || ['y', 'z'].into_iter(),
    ]) {
        println!("{:?}", item);
    }
}

#[test]
fn test_cartesian_2() {
    for item in MultiCartesianProduct::new(vec![|| ['a', 'b', 'c'].into_iter(), || {
        ['f', 'g', 'h'].into_iter()
    }]) {
        println!("{:?}", item);
    }
}

fn iterate_all(hir: &Hir, max_length: Option<usize>) -> Box<dyn Iterator<Item = Vec<u8>> + '_> {
    let result: Box<dyn Iterator<Item = Vec<u8>>> = match hir.kind() {
        Empty | Look(_) => Box::new(empty()),
        Literal(literal) => Box::new(once(literal.0.clone().into())),
        Class(class) => match class {
            Unicode(class_unicode) => Box::new(
                class_unicode
                    .iter()
                    .map(|r| r.start()..=r.end())
                    .flatten()
                    .map(|c| c.encode_utf8(&mut [0; 4]).as_bytes().to_vec()),
            ),
            Bytes(class_bytes) => Box::new(
                class_bytes
                    .iter()
                    .map(|r| r.start()..=r.end())
                    .flatten()
                    .map(|x| vec![x]),
            ),
        },
        Repetition(repetition) => {
            let mapper = move |repeats| {
                MultiCartesianProduct::new(
                    (0..repeats)
                        .map(move |_| move || iterate_all(&repetition.sub, max_length))
                        .collect(),
                )
                .map(|x| x.join(&[][..]))
            };
            match (repetition.max, max_length) {
                (Some(max), Some(max_length)) => Box::new(
                    (repetition.min as usize..=max as usize)
                        .flat_map(mapper)
                        .take_while(move |x| x.len() <= max_length),
                ),
                (Some(max), None) => {
                    Box::new((repetition.min as usize..=max as usize).flat_map(mapper))
                }
                (None, Some(max_length)) => Box::new(
                    (repetition.min as usize..)
                        .flat_map(mapper)
                        .take_while(move |x| x.len() <= max_length),
                ),
                (None, None) => Box::new((repetition.min as usize..).flat_map(mapper)),
            }
        }
        Capture(capture) => iterate_all(&capture.sub, max_length),
        Concat(hirs) => Box::new(
            MultiCartesianProduct::new(
                hirs.iter()
                    .map(move |hir| move || iterate_all(&hir, max_length))
                    .collect(),
            )
            .map(|x| x.into_iter().flatten().collect()),
        ),
        Alternation(hirs) => Box::new(
            hirs.iter()
                .map(move |h| iterate_all(h, max_length))
                .into_iter()
                .flatten(),
        ),
    };
    if let Some(max_length) = max_length {
        Box::new(result.filter(move |v| v.len() <= max_length))
    } else {
        result
    }
}

fn is_unbounded(hir: &Hir) -> bool {
    match hir.kind() {
        Repetition(repetition) => repetition.max.is_none(),
        Capture(capture) => is_unbounded(&capture.sub),
        Concat(hirs) | Alternation(hirs) => hirs.iter().any(|hir| is_unbounded(hir)),
        _ => false,
    }
}

#[test]
fn test_unbounded() {
    let hir = Parser::new().parse("a*b*").unwrap();
    let patterns: Vec<_> = iterate_all(&hir, Some(5))
        .map(|s| String::from_utf8_lossy(&s).into_owned())
        .collect();
    assert_eq!(
        patterns,
        [
            "", "a", "aa", "aaa", "aaaa", "aaaaa", "b", "ab", "aab", "aaab", "aaaab", "bb", "abb",
            "aabb", "aaabb", "bbb", "abbb", "aabbb", "bbbb", "abbbb", "bbbbb"
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>()
    )
}

/// Regex iterator
#[derive(ClapParser)]
struct Args {
    /// Pattern to iterate over
    password_pattern: String,

    /// Minimum result length
    #[clap(short = 'i', long, default_value_t = 0)]
    min_length: usize,

    /// Maximum result length
    #[clap(short = 'x', long)]
    max_length: Option<usize>,

    /// Maximum number of results to yield
    #[clap(short = 'n', long)]
    num: Option<usize>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let hir = Parser::new().parse(&args.password_pattern)?;
    if is_unbounded(&hir) && args.num.is_none() && args.max_length.is_none() {
        Err(
            "Regex contains infinite range: program will spin forever unless a max length or number of results is specified.",
        )?
    }
    for (i, item) in iterate_all(&hir, args.max_length)
        .into_iter()
        .map(|v| String::from_utf8_lossy(&v).into_owned())
        .filter(|x| x.len() >= args.min_length)
        .enumerate()
    {
        println!("{item}");
        if let Some(num) = args.num {
            if i >= num {
                break;
            }
        }
    }

    Ok(())
}
