use std::{thread, time::Duration};

#[test]
fn not_aggregated() {
    tracing_span_tree::span_tree().enable();
    top_level()
}

#[test]
fn aggregated() {
    tracing_span_tree::span_tree().aggregate(true).enable();
    top_level()
}

fn top_level() {
    let _s = tracing::info_span!("top_level").entered();
    for i in 0..4 {
        middle(i)
    }
}

fn middle(i: u64) {
    let _s = tracing::info_span!("middle").entered();
    thread::sleep(Duration::from_millis(i));
    if i % 2 == 0 {
        leaf()
    }
}

fn leaf() {
    let _s = tracing::info_span!("leaf").entered();
    thread::sleep(Duration::from_millis(1));
}
