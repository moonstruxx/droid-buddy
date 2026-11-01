//! Pure master-module resolution: which master model (MASTER or MASTER18) can
//! host a patch, and which of the patch's uses it cannot host.
//!
//! The rules are the hardware's, not heuristics (design.md D3, spec
//! "Master-module resolution and fit"):
//!
//! | Use | Requires |
//! |---|---|
//! | any `I{n}` with `n >= 3` | MASTER — MASTER18's only inputs are `I1`/`I2` |
//! | `I1`/`I2` bound to a non-`gate`/non-`trigger` param | MASTER — MASTER18 reads those jacks as logic levels |
//! | a `vcotuner` or `sinfonionlink` section | MASTER18 — those circuits do not exist on the MASTER |
//! | `O1..O8` | either |
//!
//! Nothing else constrains the model: gate jacks and LED registers are outside
//! the fit (proposal.md Non-goals). Resolution is a pure function over the
//! parsed patch and the schema — no I/O, file order for conflicts, so repeated
//! calls over the same inputs produce equal plans.

use crate::patch::{MasterModel, Patch, Span};
use crate::schema::{CircuitParam, Schema};

/// Where the resolved model came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasterSource {
    /// The patch's `# LABELS: master=16|18` declaration; authoritative.
    Declared,
    /// No declaration, but exactly one model hosts every use in the patch.
    Inferred,
    /// No declaration and nothing to infer from: MASTER, the named default.
    Default,
}

/// One use of the patch that constrains which master can host it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MasterConflict {
    /// The use as the user reads it: a jack token (`"I3"`, `"I1"`) or a
    /// section name (`"vcotuner"`).
    pub usage: String,
    /// The model that use requires.
    pub needs: MasterModel,
    /// Where to jump in the source: the entry's value span for a jack, the
    /// section header span for a circuit.
    pub span: Span,
    /// Short user-facing why, e.g. `"MASTER18 has no CV inputs"`.
    pub reason: &'static str,
}

/// The resolution verdict for one patch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MasterPlan {
    /// The model the patch is drawn for.
    pub model: MasterModel,
    /// Whether that model was declared, inferred, or defaulted.
    pub source: MasterSource,
    /// Uses `model` cannot host, in file order. Empty when it hosts everything.
    /// When `satisfiable` is `false` this lists the uses from *both* directions,
    /// so the caller can name both sides of the contradiction.
    pub conflicts: Vec<MasterConflict>,
    /// `false` when no model hosts every use — the patch cannot run anywhere.
    pub satisfiable: bool,
}

/// Circuits that exist only on MASTER18.
const MASTER18_ONLY_SECTIONS: &[&str] = &["vcotuner", "sinfonionlink"];

/// Parameter types MASTER18 can read on `I1`/`I2`; every other type needs the
/// MASTER's CV-capable jacks (design D5).
const LOGIC_PARAM_TYPES: &[&str] = &["gate", "trigger"];

const REASON_NO_DEEP_JACK: &str = "MASTER18 has only the input jacks I1 and I2";
const REASON_LOGIC_ONLY: &str = "MASTER18 reads I1 and I2 as logic levels only";
const REASON_MASTER18_CIRCUIT: &str = "this circuit does not exist on the MASTER";

/// Resolve the master model for `patch` and list every use the resolved model
/// cannot host.
///
/// A declaration wins. Without one, the single model that hosts every use is
/// inferred; if neither does, the plan reports `satisfiable == false` and lists
/// both directions of the contradiction. A patch with no constraining use at
/// all defaults to MASTER, which hosts every CV jack MASTER18 does and more.
pub fn resolve(patch: &Patch, schema: &Schema) -> MasterPlan {
    let requirements = requirements(patch, schema);
    let master_hosts = requirements
        .iter()
        .all(|use_| use_.needs != MasterModel::Master18);
    let master18_hosts = requirements
        .iter()
        .all(|use_| use_.needs != MasterModel::Master);
    let satisfiable = master_hosts || master18_hosts;

    let (model, source) = match patch.master_declaration {
        Some(declared) => (declared, MasterSource::Declared),
        // Nothing constrains the model, so both host it equally well: the
        // design's named default.
        None if requirements.is_empty() => (MasterModel::Master, MasterSource::Default),
        None if master_hosts => (MasterModel::Master, MasterSource::Inferred),
        None if master18_hosts => (MasterModel::Master18, MasterSource::Inferred),
        // Neither model hosts the patch; MASTER keeps rendering deterministic.
        None => (MasterModel::Master, MasterSource::Default),
    };

    let conflicts = if satisfiable {
        requirements
            .into_iter()
            .filter(|use_| use_.needs != model)
            .collect()
    } else {
        requirements
    };

    MasterPlan {
        model,
        source,
        conflicts,
        satisfiable,
    }
}

/// Every use of `patch` that constrains the master, in file order.
fn requirements(patch: &Patch, schema: &Schema) -> Vec<MasterConflict> {
    let mut out = Vec::new();
    for section in &patch.sections {
        let circuit = section.name.to_lowercase();
        if MASTER18_ONLY_SECTIONS.contains(&circuit.as_str()) {
            out.push(MasterConflict {
                usage: circuit.clone(),
                needs: MasterModel::Master18,
                span: section.header_span,
                reason: REASON_MASTER18_CIRCUIT,
            });
        }
        for entry in &section.detailed_entries {
            let param = param_definition(schema, &circuit, &entry.key);
            for (jack, number) in input_jacks(&entry.value) {
                let Some((needs, reason)) = constraint(number, param) else {
                    continue;
                };
                out.push(MasterConflict {
                    usage: jack,
                    needs,
                    span: entry.value_span,
                    reason,
                });
            }
        }
    }
    out
}

/// What binding input jack `number` to `param` requires, or `None` when the
/// use is hostable by both models (or cannot be typed because the circuit is
/// unknown).
fn constraint(number: u32, param: Option<&CircuitParam>) -> Option<(MasterModel, &'static str)> {
    // MASTER18 has no third input jack, so the parameter's type is irrelevant.
    if number >= 3 {
        return Some((MasterModel::Master, REASON_NO_DEEP_JACK));
    }
    match param {
        // `gate`/`trigger` on I1/I2 is a legitimate MASTER18 patch (design D5).
        Some(param) if LOGIC_PARAM_TYPES.contains(&param.param_type.as_str()) => None,
        Some(_) => Some((MasterModel::Master, REASON_LOGIC_ONLY)),
        None => None,
    }
}

/// The schema definition of `key` in `circuit`, matching through the
/// `prefix`/`count`/`start_at` expansion the validator uses (so a ranged param
/// such as `button1..button32` resolves by its expanded name).
fn param_definition<'a>(schema: &'a Schema, circuit: &str, key: &str) -> Option<&'a CircuitParam> {
    let def = schema.circuits.get(circuit)?;
    def.inputs
        .iter()
        .chain(&def.outputs)
        .find(|param| expanded_name_matches(param, key))
}

fn expanded_name_matches(param: &CircuitParam, key: &str) -> bool {
    match (param.prefix.as_deref(), param.count) {
        (Some(prefix), Some(count)) => {
            let start = param.start_at.unwrap_or(1);
            (0..count).any(|i| format!("{prefix}{}", start + i).eq_ignore_ascii_case(key))
        }
        _ => param.name.eq_ignore_ascii_case(key),
    }
}

/// Every `I{n}` input-jack token in `value`, in reading order, with its number.
///
/// Boundary-aware like the validator's jack scan: a token must start at a word
/// boundary and end cleanly, so cable names (`_I3`) and longer identifiers
/// (`IDX1`) are not jacks.
fn input_jacks(value: &str) -> Vec<(String, u32)> {
    let bytes = value.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let boundary_ok = i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
        if bytes[i] == b'I' && boundary_ok && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
            let start = i;
            let mut end = i + 1;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            let clean_end = end >= bytes.len()
                || !(bytes[end].is_ascii_alphanumeric()
                    || bytes[end] == b'_'
                    || bytes[end] == b'.');
            if clean_end {
                if let Ok(number) = value[start + 1..end].parse::<u32>() {
                    out.push((value[start..end].to_string(), number));
                }
            }
            i = end;
            continue;
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::load_schema;

    fn parsed(ini: &str) -> Patch {
        Patch::from_ini_str(ini, String::from("test.ini")).expect("fixture must parse")
    }

    fn plan(ini: &str) -> MasterPlan {
        resolve(&parsed(ini), load_schema())
    }

    const DECLARED_18_CV_ON_I3: &str = "\
# LABELS: master=18

[copy]
input = I3
";

    #[test]
    fn declared_master18_with_cv_on_i3_reports_one_conflict() {
        let patch = parsed(DECLARED_18_CV_ON_I3);
        let plan = resolve(&patch, load_schema());

        assert_eq!(plan.source, MasterSource::Declared);
        assert_eq!(plan.model, MasterModel::Master18);
        assert!(plan.satisfiable, "MASTER hosts the patch");
        assert_eq!(plan.conflicts.len(), 1);
        let conflict = &plan.conflicts[0];
        assert_eq!(conflict.usage, "I3");
        assert_eq!(conflict.needs, MasterModel::Master);
        assert_eq!(
            conflict.span,
            patch.sections[0].detailed_entries[0].value_span
        );
    }

    #[test]
    fn declared_master18_with_vcotuner_is_clean() {
        let plan = plan(
            "\
# LABELS: master=18

[vcotuner]
concertpitch = 440

[copy]
input = O1
",
        );

        assert_eq!(plan.source, MasterSource::Declared);
        assert_eq!(plan.model, MasterModel::Master18);
        assert!(plan.satisfiable);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn declared_master16_with_sinfonionlink_reports_one_conflict() {
        let patch = parsed(
            "\
# LABELS: master=16

[sinfonionlink]

[copy]
input = O1
",
        );
        let plan = resolve(&patch, load_schema());

        assert_eq!(plan.source, MasterSource::Declared);
        assert_eq!(plan.model, MasterModel::Master);
        assert!(plan.satisfiable, "MASTER18 hosts the patch");
        assert_eq!(plan.conflicts.len(), 1);
        let conflict = &plan.conflicts[0];
        assert_eq!(conflict.usage, "sinfonionlink");
        assert_eq!(conflict.needs, MasterModel::Master18);
        assert_eq!(conflict.span, patch.sections[0].header_span);
    }

    #[test]
    fn declared_master18_gate_on_i1_is_not_a_conflict() {
        let plan = plan(
            "\
# LABELS: master=18

[contour]
gate = I1
",
        );

        assert_eq!(plan.source, MasterSource::Declared);
        assert_eq!(plan.model, MasterModel::Master18);
        assert!(plan.satisfiable);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn silence_infers_master18_from_vcotuner() {
        let plan = plan(
            "\
[vcotuner]
concertpitch = 440

[copy]
input = O1
",
        );

        assert_eq!(plan.source, MasterSource::Inferred);
        assert_eq!(plan.model, MasterModel::Master18);
        assert!(plan.satisfiable);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn silence_without_requirements_defaults_to_master() {
        let plan = plan(
            "\
[copy]
input = O1
",
        );

        assert_eq!(plan.source, MasterSource::Default);
        assert_eq!(plan.model, MasterModel::Master);
        assert!(plan.satisfiable);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn cv_on_i6_plus_vcotuner_fits_no_master() {
        let plan = plan(
            "\
[copy]
input = I6

[vcotuner]
concertpitch = 440
",
        );

        assert!(!plan.satisfiable);
        assert_eq!(plan.model, MasterModel::Master);
        let usages: Vec<&str> = plan.conflicts.iter().map(|c| c.usage.as_str()).collect();
        assert_eq!(usages, ["I6", "vcotuner"]);
        let needs: Vec<MasterModel> = plan.conflicts.iter().map(|c| c.needs).collect();
        assert_eq!(needs, [MasterModel::Master, MasterModel::Master18]);
    }

    #[test]
    fn resolution_is_deterministic() {
        let patch = parsed(DECLARED_18_CV_ON_I3);
        let schema = load_schema();
        assert_eq!(resolve(&patch, schema), resolve(&patch, schema));
    }
}
