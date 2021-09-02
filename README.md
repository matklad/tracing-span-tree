Consumer of `tracing` data, which prints a hierarchical profile.

Based on https://github.com/davidbarsky/tracing-tree, but does less, while
actually printing timings for spans by default.

Usage:

```rust
tracing_span_tree::span_tree()
    .aggregate(true)
    .enable();
```

Example output:

```text
8.37ms           top_level
  1.09ms           middle
    1.06ms           leaf
  1.06ms           middle
  3.12ms           middle
    1.06ms           leaf
  3.06ms           middle
```

Same data, but with `.aggregate(true)`:

```text
8.39ms           top_level
  8.35ms    4      middle
    2.13ms    2      leaf
```
