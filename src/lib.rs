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
    fmt, mem,
    time::{Duration, Instant},
};

use tracing::{
    debug,
    field::{Field, Visit},
    span::Attributes,
    Event, Id, Metadata, Subscriber,
};
use tracing_subscriber::{
    layer::Context,
    prelude::*,
    registry::{LookupSpan, Registry},
    Layer,
};

pub fn span_tree() -> SpanTree {
    SpanTree::default()
}

#[derive(Default)]
pub struct SpanTree {
    aggregate: bool,
    formatter: Option<Box<dyn Fn(&Metadata) -> String + Send + Sync>>,
}

impl SpanTree {
    /// Merge identical sibling spans together.
    pub fn aggregate(self, yes: bool) -> SpanTree {
        SpanTree { aggregate: yes, ..self }
    }

    /// Set a custom formatter for spans
    pub fn with_formatter_fn(
        self,
        formatter: impl Fn(&Metadata) -> String + Send + Sync + 'static,
    ) -> SpanTree {
        SpanTree { formatter: Some(Box::new(formatter)), ..self }
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
    fn into_node(self, name: String) -> Node {
        Node { name, count: 1, duration: self.start.elapsed(), children: self.children }
    }
}

impl Visit for Data {
    fn record_debug(&mut self, _field: &Field, _value: &dyn fmt::Debug) {}
}

impl<S> Layer<S> for SpanTree
where
    S: Subscriber + for<'span> LookupSpan<'span> + fmt::Debug,
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

        let name = if let Some(formatter) = &self.formatter {
            formatter(span.metadata())
        } else {
            span.name().to_owned()
        };

        let mut node = data.into_node(name);

        match span.parent() {
            Some(parent_span) => {
                parent_span.extensions_mut().get_mut::<Data>().unwrap().children.push(node);
            }
            None => {
                if self.aggregate {
                    node.aggregate()
                }
                node.print()
            }
        }
    }
}

#[derive(Default)]
struct Node {
    name: String,
    count: u32,
    duration: Duration,
    children: Vec<Node>,
}

impl Node {
    fn print(&self) {
        self.go(0)
    }
    fn go(&self, level: usize) {
        let bold = "\u{001b}[1m";
        let reset = "\u{001b}[0m";

        let duration = format!("{:3.2?}", self.duration);
        let count = if self.count > 1 { self.count.to_string() } else { String::new() };
        eprintln!(
            "{:width$}  {:<9} {:<6} {bold}{}{reset}",
            "",
            duration,
            count,
            self.name,
            bold = bold,
            reset = reset,
            width = level * 2
        );
        for child in &self.children {
            child.go(level + 1)
        }
        if level == 0 {
            eprintln!()
        }
    }

    fn aggregate(&mut self) {
        if self.children.is_empty() {
            return;
        }

        self.children.sort_by_cached_key(|it| it.name.clone());
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
