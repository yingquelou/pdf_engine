//! Type 4 PostScript 计算器函数（ISO 32000-1 §7.10.5，Table 43）。
//!
//! 函数体是流中的 PostScript 计算器代码：输入压栈后执行，取栈顶 n 个
//! 值作为输出。实现规范算子子集（算术 / 关系 / 布尔 / 栈操作 / 条件 /
//! 常量），布尔以 0.0/1.0 表示。无循环结构，天然终止；求值时限制栈深
//! 防御失控数据。

use super::{err, FunctionError, Result};
use crate::pdf::types::Dict;

/// 求值栈深度上限（防御异常数据；规范未规定上限）。
const MAX_STACK: usize = 512;

/// Type 4 PostScript 计算器函数。
#[derive(Debug, Clone, PartialEq)]
pub struct CalculatorFunction {
    /// 定义域。
    pub(crate) domain: Vec<(f64, f64)>,
    /// 值域（Type 4 必填）。
    pub(crate) range: Vec<(f64, f64)>,
    /// 已解析的指令序列。
    program: Vec<Instr>,
}

impl CalculatorFunction {
    pub(super) fn from_dict(dict: &Dict, stream: Option<&[u8]>) -> Result<Self> {
        let domain = super::parse_ordered_pairs(dict, "Domain")?;
        let range = super::parse_ordered_pairs(dict, "Range")?;
        let data = stream
            .ok_or_else(|| FunctionError("type 4 function requires its /Code stream data".into()))?;
        let program = parse_program(data)?;
        Ok(Self {
            domain,
            range,
            program,
        })
    }

    pub(super) fn eval(&self, input: &[f64]) -> Result<Vec<f64>> {
        let m = self.domain.len();
        let n_out = self.range.len();
        let input = super::check_input(input, m)?;

        let mut stack: Vec<Val> = input.iter().map(|&x| Val::Num(x)).collect();
        execute(&self.program, &mut stack)?;

        if stack.len() < n_out {
            return err(format!(
                "type 4 function left {} value(s) on the stack, need {n_out}",
                stack.len()
            ));
        }
        let mut out = Vec::with_capacity(n_out);
        for v in &stack[stack.len() - n_out..] {
            match v {
                Val::Num(x) => out.push(*x),
                Val::Proc(_) => return err("typecheck: procedure left on output stack"),
            }
        }
        super::clip_output(&mut out, Some(&self.range));
        Ok(out)
    }
}

// ---------- 指令与操作符 ----------

/// 一条指令。
#[derive(Debug, Clone, PartialEq)]
enum Instr {
    /// 压入字面数值。
    Push(f64),
    /// 执行操作符。
    Op(Operator),
    /// 过程字面量 `{ ... }`（供 `if` / `ifelse` 消费）。
    Proc(Vec<Instr>),
}

/// 规范支持的算子子集。
#[derive(Debug, Clone, Copy, PartialEq)]
enum Operator {
    // 算术
    Add,
    Sub,
    Mul,
    Div,
    Idiv,
    Mod,
    Neg,
    Abs,
    Ceiling,
    Floor,
    Round,
    Truncate,
    Sqrt,
    Exp,
    Ln,
    Log,
    // 关系
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    // 布尔
    And,
    Or,
    Xor,
    Not,
    // 栈操作
    Dup,
    Exch,
    Pop,
    Copy,
    Index,
    Roll,
    // 条件
    If,
    IfElse,
    // 常量
    Pi,
    True,
    False,
}

fn parse_operator(name: &str) -> Result<Operator> {
    let op = match name {
        "add" => Operator::Add,
        "sub" => Operator::Sub,
        "mul" => Operator::Mul,
        "div" => Operator::Div,
        "idiv" => Operator::Idiv,
        "mod" => Operator::Mod,
        "neg" => Operator::Neg,
        "abs" => Operator::Abs,
        "ceiling" => Operator::Ceiling,
        "floor" => Operator::Floor,
        "round" => Operator::Round,
        "truncate" => Operator::Truncate,
        "sqrt" => Operator::Sqrt,
        "exp" => Operator::Exp,
        "ln" => Operator::Ln,
        "log" => Operator::Log,
        "eq" => Operator::Eq,
        "ne" => Operator::Ne,
        "gt" => Operator::Gt,
        "ge" => Operator::Ge,
        "lt" => Operator::Lt,
        "le" => Operator::Le,
        "and" => Operator::And,
        "or" => Operator::Or,
        "xor" => Operator::Xor,
        "not" => Operator::Not,
        "dup" => Operator::Dup,
        "exch" => Operator::Exch,
        "pop" => Operator::Pop,
        "copy" => Operator::Copy,
        "index" => Operator::Index,
        "roll" => Operator::Roll,
        "if" => Operator::If,
        "ifelse" => Operator::IfElse,
        "pi" => Operator::Pi,
        "true" => Operator::True,
        "false" => Operator::False,
        other => return Err(FunctionError(format!("unsupported operator: {other}"))),
    };
    Ok(op)
}

// ---------- 词法 / 语法 ----------

/// 词法 token。
enum Tok {
    Num(f64),
    Word(String),
    Open,
    Close,
}

/// 判断词法 token 是否为数值（避免 `f64::from_str` 把 `inf`/`nan`
/// 等误判为数值）。
fn is_number(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let has_int = i > int_start;
    let mut has_frac = false;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let frac_start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        has_frac = i > frac_start;
    }
    if !has_int && !has_frac {
        return false;
    }
    // 可选指数
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let exp_start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == exp_start {
            return false;
        }
    }
    i == b.len()
}

/// 解析函数体：切分 token 后递归下降建指令序列。
fn parse_program(code: &[u8]) -> Result<Vec<Instr>> {
    let toks = tokenize(code)?;
    parse_seq(&mut toks.into_iter(), true)
}

/// 切分为 token；数值直接解析，其余词视为操作符名（延迟到语法层校验）。
fn tokenize(code: &[u8]) -> Result<Vec<Tok>> {
    let mut toks = Vec::new();
    let mut i = 0usize;
    while i < code.len() {
        match code[i] {
            b' ' | b'\t' | b'\r' | b'\n' | 0x0c | 0x00 => i += 1,
            b'{' => {
                toks.push(Tok::Open);
                i += 1;
            }
            b'}' => {
                toks.push(Tok::Close);
                i += 1;
            }
            b'%' => {
                while i < code.len() && code[i] != b'\n' && code[i] != b'\r' {
                    i += 1;
                }
            }
            _ => {
                let start = i;
                while i < code.len()
                    && !matches!(
                        code[i],
                        b' ' | b'\t' | b'\r' | b'\n' | 0x0c | 0x00 | b'{' | b'}'
                    )
                {
                    i += 1;
                }
                let word = std::str::from_utf8(&code[start..i])
                    .map_err(|_| FunctionError("type 4 code is not valid ASCII/UTF-8".into()))?;
                if is_number(word) {
                    toks.push(Tok::Num(
                        word.parse::<f64>()
                            .map_err(|_| FunctionError(format!("invalid number: {word}")))?,
                    ));
                } else {
                    toks.push(Tok::Word(word.to_string()));
                }
            }
        }
    }
    Ok(toks)
}

/// 递归下降解析：`top=true` 时遇 `}` 报错，`top=false` 时必须以 `}` 结束。
fn parse_seq(toks: &mut std::vec::IntoIter<Tok>, top: bool) -> Result<Vec<Instr>> {
    let mut out = Vec::new();
    while let Some(t) = toks.next() {
        match t {
            Tok::Open => out.push(Instr::Proc(parse_seq(toks, false)?)),
            Tok::Close => {
                if top {
                    return err("unbalanced '}' in type 4 code");
                }
                return Ok(out);
            }
            Tok::Num(v) => out.push(Instr::Push(v)),
            Tok::Word(w) => out.push(Instr::Op(parse_operator(&w)?)),
        }
    }
    if !top {
        return err("unbalanced '{' in type 4 code");
    }
    Ok(out)
}

// ---------- 求值 ----------

/// 栈上元素：数值或过程引用（供 if / ifelse 消费）。
#[derive(Debug, Clone, Copy)]
enum Val<'a> {
    Num(f64),
    Proc(&'a [Instr]),
}

fn push<'a>(stack: &mut Vec<Val<'a>>, v: Val<'a>) -> Result<()> {
    if stack.len() >= MAX_STACK {
        return err(format!("type 4 operand stack overflow (>{MAX_STACK})"));
    }
    stack.push(v);
    Ok(())
}

fn pop_num(stack: &mut Vec<Val>) -> Result<f64> {
    match stack.pop() {
        Some(Val::Num(x)) => Ok(x),
        Some(Val::Proc(_)) => err("typecheck: expected number, got procedure"),
        None => err("stack underflow"),
    }
}

fn pop<'a>(stack: &mut Vec<Val<'a>>) -> Result<Val<'a>> {
    stack
        .pop()
        .ok_or_else(|| FunctionError("stack underflow".into()))
}

/// 执行指令序列；`Proc` 值压栈等待 `if`/`ifelse` 消费。
fn execute<'a>(prog: &'a [Instr], stack: &mut Vec<Val<'a>>) -> Result<()> {
    for instr in prog {
        match instr {
            Instr::Push(v) => push(stack, Val::Num(*v))?,
            Instr::Proc(p) => push(stack, Val::Proc(p))?,
            Instr::Op(op) => apply(*op, stack)?,
        }
    }
    Ok(())
}

fn as_proc<'a>(v: Val<'a>, what: &str) -> Result<&'a [Instr]> {
    match v {
        Val::Proc(p) => Ok(p),
        Val::Num(_) => Err(FunctionError(format!("typecheck: {what} expects a procedure"))),
    }
}

fn apply(op: Operator, stack: &mut Vec<Val>) -> Result<()> {
    match op {
        // ---- 算术（双目：先弹右操作数）----
        Operator::Add => binary(stack, |a, b| a + b)?,
        Operator::Sub => binary(stack, |a, b| a - b)?,
        Operator::Mul => binary(stack, |a, b| a * b)?,
        Operator::Div => {
            let b = pop_num(stack)?;
            let a = pop_num(stack)?;
            if b == 0.0 {
                return err("division by zero");
            }
            push(stack, Val::Num(a / b))?;
        }
        Operator::Idiv => {
            let b = pop_num(stack)?;
            let a = pop_num(stack)?;
            if b == 0.0 {
                return err("integer division by zero");
            }
            push(stack, Val::Num((a / b).trunc()))?;
        }
        Operator::Mod => {
            let b = pop_num(stack)?;
            let a = pop_num(stack)?;
            if b == 0.0 {
                return err("modulo by zero");
            }
            push(stack, Val::Num(a % b))?;
        }
        // ---- 算术（单目）----
        Operator::Neg => {
            let a = pop_num(stack)?;
            push(stack, Val::Num(-a))?;
        }
        Operator::Abs => unary(stack, f64::abs)?,
        Operator::Ceiling => unary(stack, f64::ceil)?,
        Operator::Floor => unary(stack, f64::floor)?,
        Operator::Round => unary(stack, f64::round)?,
        Operator::Truncate => unary(stack, f64::trunc)?,
        Operator::Sqrt => {
            let a = pop_num(stack)?;
            if a < 0.0 {
                return err("sqrt of negative number");
            }
            push(stack, Val::Num(a.sqrt()))?;
        }
        Operator::Exp => unary(stack, f64::exp)?,
        Operator::Ln => {
            let a = pop_num(stack)?;
            if a <= 0.0 {
                return err("ln of non-positive number");
            }
            push(stack, Val::Num(a.ln()))?;
        }
        Operator::Log => {
            let a = pop_num(stack)?;
            if a <= 0.0 {
                return err("log of non-positive number");
            }
            push(stack, Val::Num(a.log10()))?;
        }
        // ---- 关系 ----
        Operator::Eq => compare(stack, |a, b| a == b)?,
        Operator::Ne => compare(stack, |a, b| a != b)?,
        Operator::Gt => compare(stack, |a, b| a > b)?,
        Operator::Ge => compare(stack, |a, b| a >= b)?,
        Operator::Lt => compare(stack, |a, b| a < b)?,
        Operator::Le => compare(stack, |a, b| a <= b)?,
        // ---- 布尔：整数按位运算，0/1 视为布尔 ----
        Operator::And => binary(stack, |a, b| {
            if a == 0.0 || a == 1.0 {
                if a != 0.0 && b != 0.0 {
                    1.0
                } else {
                    0.0
                }
            } else {
                ((a as i64) & (b as i64)) as f64
            }
        })?,
        Operator::Or => binary(stack, |a, b| {
            if a == 0.0 || a == 1.0 {
                if a != 0.0 || b != 0.0 {
                    1.0
                } else {
                    0.0
                }
            } else {
                ((a as i64) | (b as i64)) as f64
            }
        })?,
        Operator::Xor => binary(stack, |a, b| {
            if a == 0.0 || a == 1.0 {
                ((a != 0.0) != (b != 0.0)) as i32 as f64
            } else {
                ((a as i64) ^ (b as i64)) as f64
            }
        })?,
        Operator::Not => {
            let a = pop_num(stack)?;
            let r = if a == 0.0 || a == 1.0 {
                (a == 0.0) as i32 as f64
            } else {
                !(a as i64) as f64
            };
            push(stack, Val::Num(r))?;
        }
        // ---- 栈操作 ----
        Operator::Dup => {
            let v = pop(stack)?;
            push(stack, v)?;
            push(stack, v)?;
        }
        Operator::Exch => {
            let b = pop(stack)?;
            let a = pop(stack)?;
            push(stack, b)?;
            push(stack, a)?;
        }
        Operator::Pop => {
            pop(stack)?;
        }
        Operator::Copy => {
            let n = pop_count(stack, "copy")?;
            if n > stack.len() {
                return err("copy: not enough elements on stack");
            }
            let src = stack.len() - n;
            for k in 0..n {
                let v = stack[src + k];
                push(stack, v)?;
            }
        }
        Operator::Index => {
            let n = pop_count(stack, "index")?;
            if n >= stack.len() {
                return err("index: not enough elements on stack");
            }
            let v = stack[stack.len() - 1 - n];
            push(stack, v)?;
        }
        Operator::Roll => {
            let j = pop_num(stack)? as i64;
            let n = pop_count(stack, "roll")?;
            if n > stack.len() {
                return err("roll: not enough elements on stack");
            }
            let split = stack.len() - n;
            let seg = &mut stack[split..];
            if n > 0 {
                let j = j.rem_euclid(n as i64) as usize;
                seg.rotate_left(j);
            }
        }
        // ---- 条件 ----
        Operator::If => {
            let p = as_proc(pop(stack)?, "if")?;
            let cond = pop_num(stack)?;
            if cond != 0.0 {
                execute(p, stack)?;
            }
        }
        Operator::IfElse => {
            let fproc = as_proc(pop(stack)?, "ifelse")?;
            let tproc = as_proc(pop(stack)?, "ifelse")?;
            let cond = pop_num(stack)?;
            execute(if cond != 0.0 { tproc } else { fproc }, stack)?;
        }
        // ---- 常量 ----
        Operator::Pi => push(stack, Val::Num(std::f64::consts::PI))?,
        Operator::True => push(stack, Val::Num(1.0))?,
        Operator::False => push(stack, Val::Num(0.0))?,
    }
    Ok(())
}

/// 双目数值操作：弹出 `b`、`a`，压回 `f(a, b)`。
fn binary<F: FnOnce(f64, f64) -> f64>(stack: &mut Vec<Val>, f: F) -> Result<()> {
    let b = pop_num(stack)?;
    let a = pop_num(stack)?;
    push(stack, Val::Num(f(a, b)))
}

/// 单目数值操作。
fn unary<F: FnOnce(f64) -> f64>(stack: &mut Vec<Val>, f: F) -> Result<()> {
    let a = pop_num(stack)?;
    push(stack, Val::Num(f(a)))
}

/// 关系操作：压回 1.0 / 0.0。
fn compare<F: FnOnce(f64, f64) -> bool>(stack: &mut Vec<Val>, f: F) -> Result<()> {
    let b = pop_num(stack)?;
    let a = pop_num(stack)?;
    push(stack, Val::Num(f(a, b) as i32 as f64))
}

/// 弹出非负整数计数（copy / index / roll 用）。
fn pop_count(stack: &mut Vec<Val>, what: &str) -> Result<usize> {
    let v = pop_num(stack)?;
    if v < 0.0 || v.fract() != 0.0 || v > i32::MAX as f64 {
        return Err(FunctionError(format!(
            "{what}: count must be a non-negative integer, got {v}"
        )));
    }
    Ok(v as usize)
}
