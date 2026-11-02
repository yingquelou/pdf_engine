//! 状态机批量运行入口与 [`Document`] 的页面内容图形状态查询 API。

use super::super::Op;
use super::error::ContentStateError;
use super::model::{ContentGraphicsState, ExtGStateResolver};
use super::runner::ContentStateRunner;

use crate::core::reader::graphics_state::ExtGState;
use crate::core::types::Document;

/// Run the state machine over operators, snapshotting the state after each.
pub fn run_content_state(
    ops: &[Op],
    resolve_ext_g_state: ExtGStateResolver<'_>,
) -> std::result::Result<Vec<ContentGraphicsState>, ContentStateError> {
    let mut runner = ContentStateRunner::new();
    let mut trace = Vec::with_capacity(ops.len());
    for op in ops {
        runner.apply(op, resolve_ext_g_state)?;
        trace.push(runner.state().clone());
    }
    Ok(trace)
}

impl Document {
    /// Apply the page content stream to the graphics state machine,
    /// returning the state snapshot after each operator.
    ///
    /// `gs` operands are resolved through the page `/Resources/ExtGState`
    /// dictionary (with `/Parent` inheritance) and parsed by
    /// [`Document::ext_g_state`].
    pub fn page_content_state_trace(
        &self,
        page_index: usize,
    ) -> std::result::Result<Vec<ContentGraphicsState>, ContentStateError> {
        let ops = self.page_content_ops(page_index)?;
        let mut resolver =
            |name: &str| -> std::result::Result<Option<ExtGState>, ContentStateError> {
                let value = self
                    .page_resource(page_index, "ExtGState", name)
                    .map_err(|_| ContentStateError::UnknownExtGState { name: name.into() })?;
                match value {
                    Some(value) => self.ext_g_state(&value).map(Some).map_err(|error| {
                        ContentStateError::InvalidExtGState {
                            name: name.into(),
                            message: error.to_string(),
                        }
                    }),
                    None => Ok(None),
                }
            };
        run_content_state(&ops, &mut resolver)
    }

    /// Final graphics state after applying the whole page content stream.
    pub fn page_content_state(
        &self,
        page_index: usize,
    ) -> std::result::Result<ContentGraphicsState, ContentStateError> {
        let trace = self.page_content_state_trace(page_index)?;
        Ok(trace.last().cloned().unwrap_or_default())
    }
}
