//! 外观字典 `/AP`：`/N` `/D` `/R` 状态读取、当前 `/AS` 与无状态空 AP 的丢弃。

use crate::core::reader::annotation::*;
use crate::core::types::{Dict, Value};

use super::super::support::*;

#[test]
fn appearance_streams_expose_normal_down_rollover_and_state() {
    let ap = Dict::new()
        .insert("N", Value::Ref(20, 0))
        .insert("D", Value::Ref(21, 0))
        .insert("R", Value::Dict(Dict::new().insert("S", Value::Ref(22, 0))));
    let annotation = parse_annotation(
        &doc(vec![]),
        0,
        None,
        &base_annotation("Widget")
            .insert("AP", Value::Dict(ap))
            .insert("AS", Value::Name("S".into())),
    )
    .expect("appearance");
    let streams = annotation.appearance.expect("AP present");
    assert_eq!(streams.normal, Some(Value::Ref(20, 0)));
    assert_eq!(streams.down, Some(Value::Ref(21, 0)));
    assert!(matches!(streams.state("R"), Some(Value::Dict(_))));
    assert!(streams.state("X").is_none());
    assert_eq!(annotation.appearance_state.as_deref(), Some("S"));
}

#[test]
fn appearance_without_states_is_dropped() {
    let annotation = parse_annotation(
        &doc(vec![]),
        0,
        None,
        &base_annotation("Widget").insert("AP", Value::Dict(Dict::new())),
    )
    .expect("empty AP");
    assert!(annotation.appearance.is_none());
}
