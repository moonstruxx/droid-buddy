use std::collections::HashMap;

pub fn evaluate_droid_expr(expr: &str, values: &HashMap<String, f64>) -> Option<f64> {
    let bytes = expr.as_bytes();
    let len = bytes.len();
    let mut pos = 0usize;

    let skip_ws = |pos: &mut usize| {
        while *pos < len && bytes[*pos].is_ascii_whitespace() {
            *pos += 1;
        }
    };

    skip_ws(&mut pos);
    if pos >= len {
        return None;
    }

    let mut accumulator: Option<f64> = None;
    let mut pending_op: Option<char> = None;

    if bytes[pos] == b'-' {
        accumulator = Some(0.0);
        pending_op = Some('-');
        pos += 1;
    } else if bytes[pos] == b'+' {
        return None;
    }

    let mut expect_operand = true;
    let mut has_operand = false;

    while pos < len {
        skip_ws(&mut pos);
        if pos >= len {
            break;
        }
        if expect_operand {
            let (v, np) = if let Some((v, np)) = try_parse_numeric(expr, pos) {
                (v, np)
            } else if let Some((tok, np)) = try_parse_cable(expr, pos) {
                let val = *values.get(&tok)?;
                (val, np)
            } else if let Some((tok, np)) = try_parse_register(expr, pos) {
                let val = *values.get(&tok)?;
                (val, np)
            } else {
                return None;
            };
            if let Some(op) = pending_op.take() {
                let acc = accumulator?;
                let res = match op {
                    '+' => acc + v,
                    '-' => acc - v,
                    '*' => acc * v,
                    '/' => {
                        if v == 0.0 {
                            0.0
                        } else {
                            acc / v
                        }
                    }
                    _ => return None,
                };
                accumulator = Some(res);
            } else {
                if accumulator.is_some() {
                    return None;
                }
                accumulator = Some(v);
            }
            pos = np;
            expect_operand = false;
            has_operand = true;
        } else {
            let c = bytes[pos] as char;
            if c == '+' || c == '-' || c == '*' || c == '/' {
                pending_op = Some(c);
                pos += 1;
                expect_operand = true;
            } else {
                return None;
            }
        }
    }

    if expect_operand {
        return None;
    }
    if !has_operand {
        return None;
    }
    if pending_op.is_some() {
        return None;
    }
    accumulator
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
