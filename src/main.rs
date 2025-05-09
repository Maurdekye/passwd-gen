#![feature(iterator_try_collect)]
#![feature(string_from_utf8_lossy_owned)]
use std::error::Error;

use clap::Parser as ClapParser;

use regex_syntax::{
    Parser,
    hir::{Class::*, Hir, HirKind::*},
};

fn iterate_all(hir: &Hir, max_length: Option<usize>) -> Vec<Vec<u8>> {
    let result = match hir.kind() {
        Empty | Look(_) => Vec::new(),
        Literal(literal) => vec![literal.0.clone().into()],
        Class(class) => match class {
            Unicode(class_unicode) => class_unicode
                .iter()
                .map(|r| r.start()..=r.end())
                .flatten()
                .map(|c| c.encode_utf8(&mut vec![0; 4]).as_bytes().to_vec())
                .collect(),
            Bytes(class_bytes) => class_bytes
                .iter()
                .map(|r| r.start()..=r.end())
                .flatten()
                .map(|x| vec![x])
                .collect(),
        },
        Repetition(repetition) => {
            let sub_exprs = iterate_all(&repetition.sub, max_length);
            let mut super_exprs = Vec::new();
            for i in repetition.min as usize.. {
                if let Some(max) = repetition.max {
                    if i > max as usize {
                        break;
                    }
                }
                let mut new_exprs: Vec<Vec<u8>> = Vec::new();
                let mut indexes = vec![0usize; i];
                'outer: loop {
                    let new_expr: Vec<u8> = indexes
                        .iter()
                        .map(|i| sub_exprs[*i].clone())
                        .flatten()
                        .collect();
                    if max_length.map_or(true, |m| new_expr.len() <= m) {
                        new_exprs.push(new_expr);
                    }

                    'inner: {
                        for index in indexes.iter_mut() {
                            if *index < sub_exprs.len() - 1 {
                                *index += 1;
                                break 'inner;
                            } else {
                                *index = 0;
                            }
                        }
                        break 'outer;
                    }
                }
                let prior_len = super_exprs.len();
                super_exprs.extend(new_exprs);
                if super_exprs.len() == prior_len {
                    break;
                }
            }
            super_exprs
        }
        Capture(capture) => iterate_all(&capture.sub, max_length),
        Concat(hirs) => {
            let sub_exprs: Vec<_> = hirs
                .iter()
                .map(|hir| iterate_all(hir, max_length))
                .collect();
            if sub_exprs.iter().any(|x| x.is_empty()) {
                return Vec::new();
            }
            let mut indexes = vec![0usize; sub_exprs.len()];
            let mut super_exprs: Vec<Vec<u8>> = Vec::new();
            'outer: loop {
                super_exprs.push(
                    sub_exprs
                        .iter()
                        .zip(&indexes)
                        .map(|(e, i)| e[*i].clone())
                        .flatten()
                        .collect(),
                );
                'inner: {
                    for (i, index) in indexes.iter_mut().enumerate() {
                        if *index < sub_exprs[i].len() - 1 {
                            *index += 1;
                            break 'inner;
                        } else {
                            *index = 0;
                        }
                    }
                    break 'outer;
                }
            }
            super_exprs
        }
        Alternation(hirs) => hirs
            .iter()
            .map(|h| iterate_all(h, max_length))
            .into_iter()
            .flatten()
            .collect(),
    };
    result
        .into_iter()
        .filter(|x| max_length.map_or(true, |m| x.len() <= m))
        .collect()
}

#[derive(ClapParser)]
struct Args {
    password_pattern: String,

    #[clap(short = 'n', long, default_value_t = 0)]
    min_length: usize,

    #[clap(short = 'x', long)]
    max_length: Option<usize>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let hir = Parser::new().parse(&args.password_pattern)?;
    let regexes = iterate_all(&hir, args.max_length);
    for item in regexes
        .into_iter()
        .map(String::from_utf8_lossy_owned)
        .filter(|x| x.len() >= args.min_length)
    {
        println!("{item}");
    }

    Ok(())
}
