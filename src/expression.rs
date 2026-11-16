use std::collections::HashMap;

/// Left-to-right evaluation of a DROID parameter expression (`A * B + C`).
///
/// Operands are numbers (`2.5`, `1V` = tenths of a volt), cable references
/// (`_NAME`) and register references (`P1.1`); the operators `+ - * /` carry no
/// precedence, so `2 + 3 * 4` is `20`. `None` means "not an expression" or a
/// name absent from `values`.
pub fn evaluate_droid_expr(
    expr: &str,
    values: &HashMap<String, f64, impl std::hash::BuildHasher>,
) -> Option<f64> {
    let bytes = expr.as_bytes();
    let mut pos = skip_ws(bytes, 0);
    if pos >= bytes.len() {
        return None;
    }

    let mut state = ExprState::default();
    if bytes[pos] == b'-' {
        state.accumulator = Some(0.0);
        state.pending_op = Some('-');
        pos += 1;
    } else if bytes[pos] == b'+' {
        return None;
    }

    let mut expect_operand = true;
    let mut has_operand = false;

    while pos < bytes.len() {
        pos = skip_ws(bytes, pos);
        if pos >= bytes.len() {
            break;
        }
        if expect_operand {
            let (value, next) = resolve_operand(expr, pos, values)?;
            state.apply(value)?;
            pos = next;
            expect_operand = false;
            has_operand = true;
        } else {
            state.pending_op = Some(operator_at(bytes[pos])?);
            pos += 1;
            expect_operand = true;
        }
    }

    if expect_operand || !has_operand || state.pending_op.is_some() {
        return None;
    }
    state.accumulator
}

/// The running result of the walk: the value so far and the operator waiting
/// for its right-hand operand.
#[derive(Default)]
struct ExprState {
    accumulator: Option<f64>,
    pending_op: Option<char>,
}

impl ExprState {
    /// Fold `value` in under the pending operator, or start the accumulator.
    ///
    /// A pending operator without an accumulator, or a second value with no
    /// operator between them, is not an expression.
    fn apply(&mut self, value: f64) -> Option<()> {
        if let Some(op) = self.pending_op.take() {
            let acc = self.accumulator?;
            self.accumulator = Some(combine(op, acc, value)?);
            return Some(());
        }
        if self.accumulator.is_some() {
            return None;
        }
        self.accumulator = Some(value);
        Some(())
    }
}

/// Index of the first non-whitespace byte at or after `pos`.
fn skip_ws(bytes: &[u8], pos: usize) -> usize {
    let mut i = pos;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// The four single-character operators, or `None` for anything else.
fn operator_at(byte: u8) -> Option<char> {
    let c = byte as char;
    matches!(c, '+' | '-' | '*' | '/').then_some(c)
}

/// Resolve the operand starting at `pos` — a number, a cable reference or a
/// register reference, in that order — returning it with the index just past
/// it.
fn resolve_operand(
    expr: &str,
    pos: usize,
    values: &HashMap<String, f64, impl std::hash::BuildHasher>,
) -> Option<(f64, usize)> {
    if let Some(numeric) = try_parse_numeric(expr, pos) {
        return Some(numeric);
    }
    if let Some((tok, next)) = try_parse_cable(expr, pos) {
        return Some((*values.get(&tok)?, next));
    }
    let (tok, next) = try_parse_register(expr, pos)?;
    Some((*values.get(&tok)?, next))
}

/// Apply one operator. Division by zero yields `0.0`, matching the firmware.
fn combine(op: char, acc: f64, value: f64) -> Option<f64> {
    Some(match op {
        '+' => acc + value,
        '-' => acc - value,
        '*' => acc * value,
        '/' => {
            if value == 0.0 {
                0.0
            } else {
                acc / value
            }
        }
        _ => return None,
    })
}

fn try_parse_numeric(s: &str, pos: usize) -> Option<(f64, usize)> {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut i = pos;
    let mut seen_digit = false;
    let mut seen_dot = false;
    while i < len {
        let b = bytes[i];
        if b.is_ascii_digit() {
            seen_digit = true;
            i += 1;
        } else if b == b'.' {
            if seen_dot {
                break;
            }
            seen_dot = true;
            i += 1;
        } else {
            break;
        }
    }
    if !seen_digit {
        return None;
    }
    let mut has_v = false;
    if i < len && bytes[i] == b'V' {
        has_v = true;
        i += 1;
    }
    let num_str = if has_v { &s[pos..i - 1] } else { &s[pos..i] };
    let parsed: f64 = num_str.parse().ok()?;
    let val = if has_v { parsed / 10.0 } else { parsed };
    Some((val, i))
}

fn try_parse_cable(s: &str, pos: usize) -> Option<(String, usize)> {
    let bytes = s.as_bytes();
    let len = bytes.len();
    if bytes[pos] != b'_' {
        return None;
    }
    let mut i = pos + 1;
    while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    if i == pos + 1 {
        return None;
    }
    let tok = s[pos..i].to_string();
    Some((tok, i))
}

fn try_parse_register(s: &str, pos: usize) -> Option<(String, usize)> {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let c = bytes[pos] as char;
    const PREFIXES: [char; 11] = ['B', 'L', 'P', 'O', 'I', 'E', 'S', 'M', 'G', 'N', 'R'];
    if !PREFIXES.contains(&c) {
        return None;
    }
    if pos + 1 >= len || !bytes[pos + 1].is_ascii_digit() {
        return None;
    }
    let mut i = pos + 1;
    while i < len && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i < len && bytes[i] == b'.' && i + 1 < len && bytes[i + 1].is_ascii_digit() {
        i += 1;
        while i < len && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'.') {
        return None;
    }
    let tok = s[pos..i].to_string();
    Some((tok, i))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn empty() -> HashMap<String, f64> {
        HashMap::new()
    }

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[allow(clippy::approx_constant)] // the parser's decimal test value
    #[test]
    fn literal_evaluation() {
        assert!(approx(evaluate_droid_expr("42", &empty()).unwrap(), 42.0));
        assert!(approx(evaluate_droid_expr("3.14", &empty()).unwrap(), 3.14));
        assert!(approx(
            evaluate_droid_expr("  100 ", &empty()).unwrap(),
            100.0
        ));
        assert!(approx(evaluate_droid_expr("2 + 3", &empty()).unwrap(), 5.0));
    }

    #[test]
    fn v_suffix_divides_by_ten() {
        assert!(approx(evaluate_droid_expr("1V", &empty()).unwrap(), 0.1));
        assert!(approx(evaluate_droid_expr("10V", &empty()).unwrap(), 1.0));
        assert!(approx(evaluate_droid_expr("2.5V", &empty()).unwrap(), 0.25));
        assert!(approx(
            evaluate_droid_expr("1V + 2V", &empty()).unwrap(),
            0.3
        ));
    }

    #[test]
    fn cable_expression_left_to_right() {
        let mut vals = HashMap::new();
        vals.insert("_T1_ARP".to_string(), 0.5);
        vals.insert("_SELECTED_TRACK".to_string(), 0.2);
        let v = evaluate_droid_expr("_T1_ARP * 100 + _SELECTED_TRACK", &vals).unwrap();
        assert!(approx(v, 50.2));
    }

    #[test]
    fn left_to_right_no_precedence() {
        assert!(approx(
            evaluate_droid_expr("2 + 3 * 4", &empty()).unwrap(),
            20.0
        ));
        assert!(approx(
            evaluate_droid_expr("10 - 2 * 3", &empty()).unwrap(),
            24.0
        ));
    }

    #[test]
    fn leading_minus_as_zero_minus_operand() {
        assert!(approx(evaluate_droid_expr("-5", &empty()).unwrap(), -5.0));
        assert!(approx(
            evaluate_droid_expr("-5 + 10", &empty()).unwrap(),
            5.0
        ));
        assert!(approx(evaluate_droid_expr("-1V", &empty()).unwrap(), -0.1));
        let mut vals = HashMap::new();
        vals.insert("_A".to_string(), 2.0);
        assert!(approx(evaluate_droid_expr("-_A", &vals).unwrap(), -2.0));
        assert!(approx(
            evaluate_droid_expr("-_A * 10", &vals).unwrap(),
            -20.0
        ));
    }

    #[test]
    fn division_by_zero_yields_zero() {
        assert!(approx(evaluate_droid_expr("5 / 0", &empty()).unwrap(), 0.0));
        assert!(approx(
            evaluate_droid_expr("10 / 0 + 5", &empty()).unwrap(),
            5.0
        ));
        assert!(approx(evaluate_droid_expr("0 / 0", &empty()).unwrap(), 0.0));
    }

    #[test]
    fn unknown_token_aborts_to_none() {
        assert!(evaluate_droid_expr("foo", &empty()).is_none());
        assert!(evaluate_droid_expr("1 + foo", &empty()).is_none());
        assert!(evaluate_droid_expr("1 & 2", &empty()).is_none());
        assert!(evaluate_droid_expr("(1 + 2)", &empty()).is_none());
    }

    #[test]
    fn register_token_requires_value() {
        let mut vals = HashMap::new();
        vals.insert("P1.1".to_string(), 3.0);
        assert!(approx(evaluate_droid_expr("P1.1 + 1", &vals).unwrap(), 4.0));
        assert!(evaluate_droid_expr("P1.2 + 1", &vals).is_none());
        assert!(evaluate_droid_expr("B1.1", &empty()).is_none());
    }

    #[test]
    fn trailing_operator_aborts() {
        assert!(evaluate_droid_expr("1 +", &empty()).is_none());
        assert!(evaluate_droid_expr("1 * ", &empty()).is_none());
        assert!(evaluate_droid_expr("42 +  ", &empty()).is_none());
        assert!(evaluate_droid_expr("", &empty()).is_none());
        assert!(evaluate_droid_expr("   ", &empty()).is_none());
    }

    #[test]
    fn whitespace_anchored() {
        assert!(approx(
            evaluate_droid_expr(" 2   *   3 ", &empty()).unwrap(),
            6.0
        ));
        assert!(evaluate_droid_expr("1 2", &empty()).is_none());
    }
}
