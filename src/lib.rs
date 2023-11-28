//! Consumer of `tracing` data, which prints a hierarchical profile.
//!
//! Based on https://github.com/davidbarsky/tracing-tree, but does less, while
//! actually printing timings for spans by default.
//!
//! Usage:
//!
//! ```rust
//! tracing_span_tree::span_tree()
//!     .aggregate(true)
//!     .enable();
//! ```
//!
//! Example output:
//!
//! ```text
//! 8.37ms           top_level
//!   1.09ms           middle
//!     1.06ms           leaf
//!   1.06ms           middle
//!   3.12ms           middle
//!     1.06ms           leaf
//!   3.06ms           middle
//! ```
//!
//! Same data, but with `.aggregate(true)`:
//!
//! ```text
//! 8.39ms           top_level
//!  8.35ms    4      middle
//!    2.13ms    2      leaf
//! ```

use std::{
    any::TypeId,
    fmt,
    io::{self, Stderr, Write as _},
    mem,
    time::{Duration, Instant},
};

use tracing::{
    debug,
    field::{Field, Visit},
    span::Attributes,
    Event, Id, Subscriber,
};
use tracing_subscriber::{
    fmt::MakeWriter,
    layer::Context,
    prelude::*,
    registry::{LookupSpan, Registry},
    Layer,
};

const BOLD: &str = "\u{001b}[1m";
const RESET: &str = "\u{001b}[0m";

/// https://github.com/rust-lang/rust/issues/92698
///

macro_rules! let_workaround {
    (let $name:ident = style!($t:path, $c:ident, $s:expr); $($rest:tt)+) => {
        let_workaround! {
            let $name = select!(should_style::<$t>(), format_args!("{}{}{RESET}", $c, $s), format_args!("{}", $s));
            $($rest)+
        }
    };

    (let $name:ident = select!($cond:expr, $iftrue:expr, $iffalse:expr); $($rest:tt)+) => {
        select!(
            $cond,
            let_workaround!{let $name = $iftrue; $($rest)+},
            let_workaround!{let $name = $iffalse; $($rest)+}
        )
    };

    (let $name:ident = $val:expr; $($rest:tt)+) => {
        match $val {
            $name => {
                let_workaround! { $($rest)+ }
            }
        }
    };
    ($($rest:tt)+) => { $($rest)+ }
}

macro_rules! select {
    ($cond:expr, $iftrue:expr, $iffalse:expr) => {
        'outer: {
            (
                'inner: {
                    if $cond {
                        break 'inner;
                    }
                    break 'outer $iffalse;
                },
                $iftrue,
            )
                .1
        }
    };
}

pub fn span_tree() -> SpanTree {
    SpanTree { aggregate: false, writer: io::stderr }
}

pub fn span_tree_with<W: for<'a> MakeWriter<'a>>(writer: W) -> SpanTree<W> {
    SpanTree { aggregate: false, writer }
}

#[derive(Default)]
pub struct SpanTree<W = fn() -> Stderr> {
    aggregate: bool,
    writer: W,
}

impl<W> SpanTree<W>
where
    W: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    /// Merge identical sibling spans together.
    pub fn aggregate(self, yes: bool) -> Self {
        Self { aggregate: yes, ..self }
    }
    /// Set as a global subscriber
    pub fn enable(self) {
        let subscriber = Registry::default().with(self);
        tracing::subscriber::set_global_default(subscriber)
            .unwrap_or_else(|_| debug!("Global subscriber is already set"));
    }
}

struct Data {
    start: Instant,
    children: Vec<Node>,
}

impl Data {
    fn new(attrs: &Attributes<'_>) -> Self {
        let mut span = Self { start: Instant::now(), children: Vec::new() };
        attrs.record(&mut span);
        span
    }
    fn into_node(self, name: &'static str) -> Node {
        Node { name, count: 1, duration: self.start.elapsed(), children: self.children }
    }
}

impl Visit for Data {
    fn record_debug(&mut self, _field: &Field, _value: &dyn fmt::Debug) {}
}

impl<S, W> Layer<S> for SpanTree<W>
where
    W: for<'a> MakeWriter<'a> + 'static,
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    fn on_new_span(&self, attrs: &Attributes, id: &Id, ctx: Context<S>) {
        let span = ctx.span(id).unwrap();

        let data = Data::new(attrs);
        span.extensions_mut().insert(data);
    }

    fn on_event(&self, _event: &Event<'_>, _ctx: Context<S>) {}

    fn on_close(&self, id: Id, ctx: Context<S>) {
        let span = ctx.span(&id).unwrap();
        let data = span.extensions_mut().remove::<Data>().unwrap();
        let mut node = data.into_node(span.name());

        match span.parent() {
            Some(parent_span) => {
                parent_span.extensions_mut().get_mut::<Data>().unwrap().children.push(node);
            }
            None => node.print(self.aggregate, &self.writer),
        }
    }
}

#[derive(Default)]
struct Node {
    name: &'static str,
    count: u32,
    duration: Duration,
    children: Vec<Node>,
}

fn should_style<'a, W>() -> bool
where
    W: MakeWriter<'a>,
    W::Writer: 'static,
{
    if TypeId::of::<W::Writer>() == TypeId::of::<io::Stderr>() {
        true
    } else if TypeId::of::<W::Writer>() == TypeId::of::<io::Stderr>() {
        true
    } else {
        false
    }
}

impl Node {
    fn print<W: for<'a> MakeWriter<'a> + 'static>(&mut self, agg: bool, writer: &W) {
        if agg {
            self.aggregate()
        }
        self.go(0, writer)
    }
    fn go<W: for<'a> MakeWriter<'a> + 'static>(&self, level: usize, writer: &W) {
        let width = level * 2;

        let Self { name, count, duration, .. } = self;

        // avoid intermediate allocations
        let_workaround! {
            let name = style!(W, BOLD, name);
            let count = select!(*count > 1, format_args!(" {count:<6} "), format_args!(" "));
            let duration = format_args!(" {duration:3.2?} ");

            let _ = writeln!(
                writer.make_writer(),
                "{s:width$}{duration:<9}{count}{name}",
                s = "",
            );
        }

        for child in &self.children {
            child.go(level + 1, writer)
        }
        if level == 0 {
            let _ = writeln!(writer.make_writer());
        }
    }

    fn aggregate(&mut self) {
        if self.children.is_empty() {
            return;
        }

        self.children.sort_by_key(|it| it.name);
        let mut idx = 0;
        for i in 1..self.children.len() {
            if self.children[idx].name == self.children[i].name {
                let child = mem::take(&mut self.children[i]);
                self.children[idx].duration += child.duration;
                self.children[idx].count += child.count;
                self.children[idx].children.extend(child.children);
            } else {
                idx += 1;
                assert!(idx <= i);
                self.children.swap(idx, i);
            }
        }
        self.children.truncate(idx + 1);
        for child in &mut self.children {
            child.aggregate()
        }
    }
}
