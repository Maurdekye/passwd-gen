#![feature(iterator_try_collect)]
#![feature(string_from_utf8_lossy_owned)]
use std::{
    error::Error,
    iter::{empty, once},
};

use clap::Parser as ClapParser;

use regex_syntax::{
    Parser,
    hir::{Class::*, Hir, HirKind::*},
};

struct Memo<I>
where
    I: Iterator,
{
    iter: Option<I>,
    memo: Vec<I::Item>,
}

impl<I> Memo<I>
where
    I: Iterator,
{
    fn new(iter: impl IntoIterator<IntoIter = I>) -> Self {
        Memo {
            iter: Some(iter.into_iter()),
            memo: Vec::new(),
        }
    }

    fn get(&mut self, index: usize) -> Option<&I::Item> {
        if let Some(iter) = &mut self.iter {
            while self.memo.len() <= index {
                if let Some(item) = iter.next() {
                    self.memo.push(item);
                } else {
                    self.iter = None;
                    break;
                }
            }
        }
        self.memo.get(index)
    }
}

struct MultiCartesianProduct<I>
where
    I: Iterator,
{
    iters: Vec<Memo<I>>,
    indexes: Vec<usize>,
    is_empty: bool,
}

impl<I> MultiCartesianProduct<I>
where
    I: Iterator,
{
    fn new<J>(iters: impl IntoIterator<Item = J>) -> Self
    where
        J: IntoIterator<IntoIter = I>,
    {
        let iters: Vec<Memo<I>> = iters.into_iter().map(Memo::new).collect();
        let indexes = vec![0; iters.len()];
        Self {
            iters,
            indexes,
            is_empty: false,
        }
    }
}

impl<I> Iterator for MultiCartesianProduct<I>
where
    I: Iterator,
    I::Item: Clone,
{
    type Item = Vec<I::Item>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.iters.is_empty() {
            return if self.is_empty {
                None
            } else {
                self.is_empty = true;
                Some(Vec::new())
            };
        }
        let new_item = 'outer: loop {
            let mut new_item = Vec::new();
            for (i, iter) in self.iters.iter_mut().enumerate() {
                if let Some(item) = iter.get(self.indexes[i]) {
                    new_item.push(item.clone());
                } else {
                    if i + 1 == self.indexes.len() {
                        return None;
                    } else {
                        for j in 0..=i {
                            self.indexes[j] = 0;
                        }
                        self.indexes[i + 1] += 1;
                    }
                    continue 'outer;
                }
            }
            break new_item;
        };
        self.indexes[0] += 1;

        Some(new_item)
    }
}

#[test]
fn test_cartesian() {
    for item in MultiCartesianProduct::new([['a', 'b'], ['f', 'g'], ['y', 'z']]) {
        println!("{:?}", item);
    }
}

#[test]
fn test_cartesian_2() {
    for item in MultiCartesianProduct::new([['a', 'b', 'c'], ['f', 'g', 'h']]) {
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
                    .map(|c| c.encode_utf8(&mut vec![0; 4]).as_bytes().to_vec()),
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
            if let Some(max) = repetition.max {
                Box::new(
                    (repetition.min as usize..=max as usize).flat_map(move |repeats| {
                        MultiCartesianProduct::new(
                            (0..repeats).map(|_| iterate_all(&repetition.sub, max_length)),
                        )
                        .map(|x| x.into_iter().flatten().collect())
                    }),
                )
            } else {
                Box::new((repetition.min as usize..).flat_map(move |repeats| {
                    MultiCartesianProduct::new(
                        (0..repeats).map(|_| iterate_all(&repetition.sub, max_length)),
                    )
                    .map(|x| x.into_iter().flatten().collect())
                }))
            }
        }
        Capture(capture) => iterate_all(&capture.sub, max_length),
        Concat(hirs) => Box::new(
            MultiCartesianProduct::new(hirs.iter().map(|hir| iterate_all(&hir, max_length)))
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
    let regexes = iterate_all(&hir, args.max_length);
    for (i, item) in regexes
        .into_iter()
        .map(String::from_utf8_lossy_owned)
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
