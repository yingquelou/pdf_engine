//! Type 1 flex: `callothersubr`/`pop` sequences expanded into curve segments.

use super::super::super::FontResult;
use super::{Flow, Interpreter};

const FLEX_POINT_COUNT: usize = 7;
const FLEX_OPERAND_COUNT: usize = FLEX_POINT_COUNT * 2 + 3;

impl Interpreter<'_> {
    pub(super) fn call_other_subr(&mut self) -> FontResult<Flow> {
        let procedure = self.pop_index("callothersubr")?;
        let argument_count = self.pop_index("callothersubr")?;
        match (procedure, argument_count) {
            (1, 0) => self.begin_flex(),
            (0, 3) => self.finish_flex(),
            (3, _) => Err(self.error("hint replacement (`callothersubr` 3) is not supported")),
            _ => Err(self.error(format!(
                "unsupported `callothersubr` procedure {procedure} with {argument_count} argument(s)"
            ))),
        }
    }

    pub(super) fn pop_other_subr_result(&mut self) -> FontResult<Flow> {
        if self.pending_pops == 0 {
            return Err(self.error("`pop` has no pending PostScript result"));
        }
        self.pending_pops -= 1;
        Ok(Flow::Continue)
    }

    /// While flexing, `rmoveto` only records one of the seven flex points.
    pub(super) fn flex_rmoveto(&mut self) -> FontResult<Flow> {
        if self.stack.len() < 2 {
            return Err(self.error("flex rmoveto requires 2 arguments"));
        }
        Ok(Flow::Continue)
    }

    pub(super) fn flex_hmoveto(&mut self) -> FontResult<Flow> {
        let dx = self.pop("flex hmoveto")?;
        self.stack.push(dx);
        self.stack.push(0.0);
        Ok(Flow::Continue)
    }

    pub(super) fn flex_vmoveto(&mut self) -> FontResult<Flow> {
        let dy = self.pop("flex vmoveto")?;
        self.stack.push(0.0);
        self.stack.push(dy);
        Ok(Flow::Continue)
    }

    fn begin_flex(&mut self) -> FontResult<Flow> {
        if self.flexing {
            return Err(self.error("flex is already active"));
        }
        self.flexing = true;
        Ok(Flow::Continue)
    }

    fn finish_flex(&mut self) -> FontResult<Flow> {
        if !self.flexing {
            return Err(self.error("flex exit requires a preceding `0 1 callothersubr`"));
        }
        if self.stack.len() < FLEX_OPERAND_COUNT {
            return Err(self.error(format!(
                "flex requires {FLEX_OPERAND_COUNT} operands for seven points, the flex depth, and the final point"
            )));
        }
        let final_y = self.pop("flex")?;
        let final_x = self.pop("flex")?;
        let _flex_depth = self.pop("flex")?;
        let p3 = self.pop_point("flex")?;
        let bcp4 = self.pop_point("flex")?;
        let bcp3 = self.pop_point("flex")?;
        let p2 = self.pop_point("flex")?;
        let bcp2 = self.pop_point("flex")?;
        let bcp1 = self.pop_point("flex")?;
        let rp = self.pop_point("flex")?;

        let control1 = [self.x + rp[0] + bcp1[0], self.y + rp[1] + bcp1[1]];
        let control2 = [control1[0] + bcp2[0], control1[1] + bcp2[1]];
        let joint = [control2[0] + p2[0], control2[1] + p2[1]];
        self.curve_to(control1, control2, joint);

        let control3 = [joint[0] + bcp3[0], joint[1] + bcp3[1]];
        let control4 = [control3[0] + bcp4[0], control3[1] + bcp4[1]];
        let end = [control4[0] + p3[0], control4[1] + p3[1]];
        self.curve_to(control3, control4, end);

        self.flexing = false;
        // `pop pop setcurrentpoint` consumes these two results.
        self.stack.push(final_x);
        self.stack.push(final_y);
        self.pending_pops = 2;
        Ok(Flow::Continue)
    }

    fn pop_point(&mut self, operator: &str) -> FontResult<[f64; 2]> {
        let y = self.pop(operator)?;
        let x = self.pop(operator)?;
        Ok([x, y])
    }
}

#[cfg(test)]
mod tests;
