//! Pure argv grammar and renderers for the headless command surface
//! (design D1, D3, D4, D8).
//!
//! [`parse`] turns a process argv tail into a [`Command`] without doing any
//! IO: no file is opened, nothing is printed, and the process never exits. The
//! executor that reads the patch, runs the analysis and picks an exit code
//! lives elsewhere and consumes this module's output plus the `EXIT_*`
//! constants.
//!
//! The renderers continue that rule: `analyze_text` and `optimize_text` turn a
//! report into the short human summary, and `to_json` into the JSON contract
//! (design D3). Both return a `String` the executor writes; nothing here
//! touches stdout, and neither form carries a trailing newline.
//!
//! # Grammar
//!
//! ```text
//! droid_tui analyze  <patch|-> [--json] [--no-plugins] [--deny <error|warning>]
//! droid_tui optimize <patch|-> [--json] [--no-plugins] [--deny <error|warning>]
//!                             [--strategy <banner|global|minmax>]
//!                             [--weight <0.0..=1.0>] [--candidate <n>]
//!                             [--write <path> | --stdout]
//! ```
//!
//! Exactly one patch operand is required; `-` reads standard input. A lone
//! `--` ends option parsing, so `analyze -- optimize` reads a *file* named
//! `optimize` (design D1's escape for a path that spells a subcommand).
//!
//! # Why no side effects here
//!
//! Every rejection this module makes is decidable from argv alone, so making
//! it here keeps the error paths testable without a filesystem, a process or
//! a captured stream. Nothing in this module may gain an IO call: `parse` is
//! called before the window machinery exists (design D1) and must stay total
//! apart from the returned [`UsageError`].

use crate::optimize::OptimizeScope;
use crate::report::{AnalyzeReport, OptimizeReport, PatchSummary};
use crate::validation::Severity;

/// The command completed and no finding reached the deny threshold (design D4).
pub const EXIT_OK: i32 = 0;

/// The command completed with at least one finding at or above the deny
/// threshold (design D4).
pub const EXIT_FINDINGS: i32 = 1;

/// The command could not run at all: unknown or incomplete arguments, an
/// unreadable file, or a patch that does not parse (design D4).
pub const EXIT_USAGE: i32 = 2;

/// Where the patch text comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchSource {
    /// Read the patch from this path.
    File(String),
    /// Read the patch from standard input (`-`); read-only (design D8).
    Stdin,
}

/// Where the selected optimization candidate is emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputTarget {
    /// Save-as to this path (never the source; design D7).
    Write(String),
    /// Emit the rendered patch text on standard output (design D7).
    Stdout,
}

/// A parsed invocation.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// `--help` / `-h` was requested; print [`usage`] and exit [`EXIT_OK`].
    Help,
    /// Run the analysis report for one patch.
    Analyze(AnalyzeRequest),
    /// Report section reorderings for one patch.
    Optimize(OptimizeRequest),
}

/// Options shared by both subcommands, plus the analysis-only set.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalyzeRequest {
    /// Patch to report on.
    pub patch: PatchSource,
    /// Emit the JSON contract instead of the human summary (design D3).
    pub json: bool,
    /// Restrict the run to the embedded schema (design D5).
    pub no_plugins: bool,
    /// Severity gate: a finding fails the run when
    /// `finding.severity <= deny`. [`Severity`] orders `Error` first, so the
    /// default [`Severity::Error`] fails only on errors while
    /// `--deny warning` fails on warnings and errors alike (design D4).
    pub deny: Severity,
}

/// Options for `optimize`, i.e. the shared set plus the search and output
/// selectors.
#[derive(Debug, Clone, PartialEq)]
pub struct OptimizeRequest {
    /// Patch to report on.
    pub patch: PatchSource,
    /// Emit the JSON contract instead of the human summary (design D3).
    pub json: bool,
    /// Restrict the run to the embedded schema (design D5).
    pub no_plugins: bool,
    /// Severity gate, as for [`AnalyzeRequest::deny`].
    pub deny: Severity,
    /// Which candidate strategies to search.
    pub strategy: OptimizeScope,
    /// Objective weight in `0.0..=1.0` trading total latency against the
    /// worst edge; `0.0` means total latency only.
    pub weight: f32,
    /// Index of the candidate the output option acts on; `0` is the best.
    pub candidate: usize,
    /// Emit the selected candidate, or `None` to report only.
    pub output: Option<OutputTarget>,
}

/// A structured argument error. Carries a message naming the offending
/// argument; rendering and exiting are the caller's job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageError {
    message: String,
}

impl UsageError {
    /// Build an error from a message that names the offending argument.
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// The human-readable reason, without a usage trailer.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for UsageError {}

/// `true` for exactly the two subcommand names.
///
/// A path that merely resembles one (`analyz`, `Optimize`, `analyze.ini`) is
/// not a subcommand, which is what keeps `droid_tui <path>` opening a window.
#[must_use]
pub fn is_subcommand(arg: &str) -> bool {
    matches!(arg, "analyze" | "optimize")
}

/// A short usage block for both commands, suitable as the `--help` body and
/// as the trailer of a usage error.
#[must_use]
pub fn usage() -> &'static str {
    "Usage:\n\
     \x20 droid_tui analyze  <patch|-> [--json] [--no-plugins] [--deny <error|warning>]\n\
     \x20 droid_tui optimize <patch|-> [--json] [--no-plugins] [--deny <error|warning>]\n\
     \x20                       [--strategy <banner|global|minmax>]\n\
     \x20                       [--weight <0.0..=1.0>] [--candidate <n>]\n\
     \x20                       [--write <path> | --stdout]\n\
     \n\
     <patch> is a patch path; `-` reads standard input. `--` ends option parsing.\n\
     Exit codes: 0 clean, 1 findings at or above the deny threshold, 2 could not run."
}

/// Which subcommand is being parsed; only used to scope the option set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Analyze,
    Optimize,
}

/// Parse `args`, where `args[0]` is the subcommand and the rest are options
/// plus exactly one patch operand.
///
/// Pure: no IO, no process exit. `--help`/`-h` anywhere in option position
/// yields [`Command::Help`] rather than an error, so a caller can print
/// [`usage`] and exit [`EXIT_OK`].
pub fn parse(args: &[String]) -> Result<Command, UsageError> {
    let Some(first) = args.first() else {
        return Err(UsageError::new(
            "missing subcommand: expected `analyze` or `optimize`",
        ));
    };
    if first == "--help" || first == "-h" {
        return Ok(Command::Help);
    }
    let mode = match first.as_str() {
        "analyze" => Mode::Analyze,
        "optimize" => Mode::Optimize,
        other => {
            return Err(UsageError::new(format!(
                "unknown subcommand `{other}`: expected `analyze` or `optimize`"
            )));
        }
    };

    let mut json = false;
    let mut no_plugins = false;
    let mut deny = Severity::Error;
    let mut strategy = OptimizeScope::Banner;
    let mut weight = 0.0_f32;
    let mut candidate = 0_usize;
    let mut write: Option<String> = None;
    let mut stdout = false;
    let mut patch: Option<PatchSource> = None;
    let mut options_done = false;

    let mut i = 1;
    while i < args.len() {
        let arg = args[i].as_str();

        if !options_done && arg == "--" {
            options_done = true;
            i += 1;
            continue;
        }

        // A lone `-` is the stdin operand, not an option.
        if !options_done && arg.starts_with('-') && arg != "-" {
            match arg {
                "--help" | "-h" => return Ok(Command::Help),
                "--json" => json = true,
                "--no-plugins" => no_plugins = true,
                "--deny" => {
                    let raw = value_of(args, &mut i, "--deny")?;
                    deny = match raw {
                        "error" => Severity::Error,
                        "warning" => Severity::Warning,
                        other => {
                            return Err(UsageError::new(format!(
                                "invalid value `{other}` for `--deny`: expected `error` or `warning`"
                            )));
                        }
                    };
                }
                "--strategy" => {
                    require_optimize(mode, "--strategy")?;
                    let raw = value_of(args, &mut i, "--strategy")?;
                    strategy = match raw {
                        "banner" => OptimizeScope::Banner,
                        "global" => OptimizeScope::Global,
                        "minmax" => OptimizeScope::MinMax,
                        other => {
                            return Err(UsageError::new(format!(
                                "invalid value `{other}` for `--strategy`: expected `banner`, `global` or `minmax`"
                            )));
                        }
                    };
                }
                "--weight" => {
                    require_optimize(mode, "--weight")?;
                    let raw = value_of(args, &mut i, "--weight")?;
                    let parsed: f32 = raw.parse().map_err(|_| {
                        UsageError::new(format!(
                            "invalid value `{raw}` for `--weight`: expected a number in 0.0..=1.0"
                        ))
                    })?;
                    if !(0.0..=1.0).contains(&parsed) {
                        return Err(UsageError::new(format!(
                            "value `{raw}` for `--weight` is outside 0.0..=1.0"
                        )));
                    }
                    weight = parsed;
                }
                "--candidate" => {
                    require_optimize(mode, "--candidate")?;
                    let raw = value_of(args, &mut i, "--candidate")?;
                    candidate = raw.parse().map_err(|_| {
                        UsageError::new(format!(
                            "invalid value `{raw}` for `--candidate`: expected a non-negative integer"
                        ))
                    })?;
                }
                "--write" => {
                    require_optimize(mode, "--write")?;
                    write = Some(value_of(args, &mut i, "--write")?.to_owned());
                }
                "--stdout" => {
                    require_optimize(mode, "--stdout")?;
                    stdout = true;
                }
                other => {
                    return Err(UsageError::new(format!(
                        "unknown option `{other}`: see `--help`"
                    )));
                }
            }
            i += 1;
            continue;
        }

        if patch.is_some() {
            return Err(UsageError::new(format!(
                "unexpected extra operand `{arg}`: exactly one patch is accepted"
            )));
        }
        patch = Some(if arg == "-" {
            PatchSource::Stdin
        } else {
            PatchSource::File(arg.to_owned())
        });
        i += 1;
    }

    let patch = patch.ok_or_else(|| {
        UsageError::new("missing patch operand: give a path or `-` for standard input")
    })?;

    if write.is_some() && stdout {
        return Err(UsageError::new(
            "options `--write` and `--stdout` are mutually exclusive: choose one output target",
        ));
    }

    match mode {
        Mode::Analyze => Ok(Command::Analyze(AnalyzeRequest {
            patch,
            json,
            no_plugins,
            deny,
        })),
        Mode::Optimize => {
            let output = if let Some(path) = write {
                // `write_to_ini` refuses to overwrite its source and derives
                // the collision suffix from the destination, so it needs a
                // real source path rather than a fabricated one (design D8).
                if matches!(patch, PatchSource::Stdin) {
                    return Err(UsageError::new(
                        "`--write` needs a patch path: standard input cannot be written back, use `--stdout`",
                    ));
                }
                Some(OutputTarget::Write(path))
            } else if stdout {
                Some(OutputTarget::Stdout)
            } else {
                None
            };
            Ok(Command::Optimize(OptimizeRequest {
                patch,
                json,
                no_plugins,
                deny,
                strategy,
                weight,
                candidate,
                output,
            }))
        }
    }
}

/// Take the next argv entry as an option value, advancing the cursor past it.
fn value_of<'a>(
    args: &'a [String],
    cursor: &mut usize,
    option: &str,
) -> Result<&'a str, UsageError> {
    *cursor += 1;
    args.get(*cursor)
        .map(String::as_str)
        .ok_or_else(|| UsageError::new(format!("option `{option}` requires a value")))
}

/// Reject an optimize-only option used under `analyze`.
fn require_optimize(mode: Mode, option: &str) -> Result<(), UsageError> {
    if mode == Mode::Optimize {
        Ok(())
    } else {
        Err(UsageError::new(format!(
            "option `{option}` is only valid for `optimize`"
        )))
    }
}

/// How many of the reported worst edges the human summary prints.
///
/// The report keeps up to `report::WORST_EDGE_LIMIT`; the summary is prose for a
/// terminal, so it shows only the slowest few.
const WORST_EDGE_LINES: usize = 3;

/// The one-line patch heading both summaries start with: name and counts.
fn patch_heading(patch: &PatchSummary) -> String {
    let name = &patch.name;
    let (sections, circuits) = (patch.sections, patch.circuits);
    let (cables, hw_tokens) = (patch.cables, patch.hw_tokens);
    format!(
        "{name} · {sections} sections · {circuits} circuits · {cables} cables · {hw_tokens} hw tokens"
    )
}

/// Render the human summary of an `analyze` run.
///
/// Compact, deterministic, no ANSI and no trailing newline: the caller writes
/// the string and owns the line ending. Every number is printed with `{:.4}`,
/// matching the rounding the report already applied, so the text and the JSON
/// document agree.
#[must_use]
pub fn analyze_text(report: &AnalyzeReport) -> String {
    let mut lines = vec![patch_heading(&report.patch)];

    let master = &report.master;
    let mut master_line = format!(
        "master: {model} ({source})",
        model = master.model,
        source = master.source
    );
    if !master.satisfiable {
        master_line.push_str(" unsatisfiable");
    }
    lines.push(master_line);

    let ram = &report.ram;
    lines.push(format!(
        "ram: {used}/{budget} bytes ({model})",
        used = ram.used,
        budget = ram.budget,
        model = ram.model
    ));

    let latency = &report.latency;
    if latency.worst.is_empty() {
        lines.push(String::from("latency: none"));
    } else {
        lines.push(format!(
            "latency: avg {avg:.4} · max {max:.4} · {back_edges} back edges",
            avg = latency.avg,
            max = latency.max,
            back_edges = latency.back_edges
        ));
        for edge in latency.worst.iter().take(WORST_EDGE_LINES) {
            lines.push(format!(
                "  worst: {cable}  {source} → {sink}  {latency:.4}",
                cable = edge.cable,
                source = edge.source,
                sink = edge.sink,
                latency = edge.latency
            ));
        }
    }

    let schema = &report.schema;
    let mut schema_line = format!(
        "schema: {circuits} circuits ({source}",
        circuits = schema.circuits,
        source = schema.source
    );
    if !schema.plugins.is_empty() {
        let plugins = schema.plugins.join(", ");
        schema_line.push_str(", plugins: ");
        schema_line.push_str(&plugins);
    }
    schema_line.push(')');
    lines.push(schema_line);

    let mut errors = 0_usize;
    let mut warnings = 0_usize;
    let mut hints = 0_usize;
    for finding in &report.validation {
        match finding.severity.as_str() {
            "error" => errors += 1,
            "warning" => warnings += 1,
            "hint" => hints += 1,
            _ => {}
        }
    }
    lines.push(format!(
        "validation: {errors} error(s), {warnings} warning(s), {hints} hint(s)"
    ));
    for finding in &report.validation {
        lines.push(format!(
            "  {line}:{col}  {severity}  {code}  {message}",
            line = finding.line,
            col = finding.col_start,
            severity = finding.severity,
            code = finding.code,
            message = finding.message
        ));
    }

    lines.push(format!("topology: {} finding(s)", report.topology.len()));
    for finding in &report.topology {
        lines.push(format!(
            "  {cable}  {severity}  {message}",
            cable = finding.cable,
            severity = finding.severity,
            message = finding.message
        ));
    }

    lines.push(format!("groups: {}", report.groups.len()));
    for group in &report.groups {
        lines.push(format!(
            "  {title}  [{start}..{end})",
            title = group.title,
            start = group.start,
            end = group.end
        ));
    }

    lines.join("\n")
}

/// Render the human summary of an `optimize` run.
///
/// One line for the baseline the candidates share and one line per candidate,
/// best first and indexed as `--candidate` indexes them. Signed deltas use
/// `{:+.4}`, so a negative number is an improvement. No trailing newline.
#[must_use]
pub fn optimize_text(report: &OptimizeReport) -> String {
    let mut lines = vec![patch_heading(&report.patch)];

    let baseline = &report.baseline;
    lines.push(format!(
        "baseline: avg {avg:.4} · max {max:.4} · {back_edges} back edges",
        avg = baseline.avg,
        max = baseline.max,
        back_edges = baseline.back_edges
    ));

    if report.candidates.is_empty() {
        lines.push(String::from("candidates: none"));
    }
    for (index, candidate) in report.candidates.iter().enumerate() {
        let label = &candidate.label;
        let (before_avg, after_avg) = (candidate.before.avg, candidate.after.avg);
        let (before_max, after_max) = (candidate.before.max, candidate.after.max);
        let (delta_avg, delta_max) = (candidate.delta_avg, candidate.delta_max);
        let mut line = format!(
            "[{index}] {label}  avg {before_avg:.4} → {after_avg:.4} ({delta_avg:+.4})  \
             max {before_max:.4} → {after_max:.4} ({delta_max:+.4})"
        );
        if let Some(path) = &candidate.wrote {
            line.push_str("  wrote: ");
            line.push_str(path);
        }
        lines.push(line);
    }

    lines.join("\n")
}

/// Serialize `value` as one pretty-printed JSON document.
///
/// The returned string starts with `{` and carries no trailing newline, so a
/// caller that writes it plus `\n` emits exactly one document. Errors are only
/// a serializer's own failures, which a report never produces.
pub fn to_json<T: serde::Serialize>(value: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(value)
}

#[cfg(test)]
mod tests {
    use super::{
        analyze_text, is_subcommand, optimize_text, parse, to_json, usage, AnalyzeRequest, Command,
        OptimizeRequest, OutputTarget, PatchSource,
    };
    use crate::graph::{clusters_from_patch, Graph, GraphOptions};
    use crate::latency::CostModel;
    use crate::optimize::{generate_candidates, OptimizeScope};
    use crate::patch::Patch;
    use crate::report::{analyze_report, optimize_report, AnalyzeReport};
    use crate::schema::load_schema;
    use crate::validation::Severity;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn analyze(req: &[&str]) -> AnalyzeRequest {
        match parse(&args(req)) {
            Ok(Command::Analyze(r)) => r,
            other => panic!("expected analyze, got {other:?}"),
        }
    }

    fn optimize(req: &[&str]) -> OptimizeRequest {
        match parse(&args(req)) {
            Ok(Command::Optimize(r)) => r,
            other => panic!("expected optimize, got {other:?}"),
        }
    }

    fn error(req: &[&str]) -> String {
        parse(&args(req))
            .expect_err(&format!("expected an error for {req:?}"))
            .to_string()
    }

    // -- subcommand recognition -------------------------------------------

    #[test]
    fn subcommand_recognition_is_exact() {
        assert!(is_subcommand("analyze"));
        assert!(is_subcommand("optimize"));
        assert!(!is_subcommand("analyz"));
        assert!(!is_subcommand("analyze "));
        assert!(!is_subcommand("Optimize"));
        assert!(!is_subcommand("analyze.ini"));
        assert!(!is_subcommand(""));
        assert!(!is_subcommand("--self-test"));
    }

    #[test]
    fn no_args_is_a_usage_error() {
        assert!(error(&[]).contains("missing subcommand"));
    }

    #[test]
    fn unknown_subcommand_is_a_usage_error() {
        let message = error(&["frobnicate"]);
        assert!(message.contains("frobnicate"), "message was {message}");
        assert!(message.contains("analyze"), "message was {message}");
    }

    // -- patch operands ---------------------------------------------------

    #[test]
    fn analyze_file_uses_defaults() {
        assert_eq!(
            analyze(&["analyze", "p.ini"]),
            AnalyzeRequest {
                patch: PatchSource::File("p.ini".to_owned()),
                json: false,
                no_plugins: false,
                deny: Severity::Error,
            }
        );
    }

    #[test]
    fn analyze_dash_reads_stdin() {
        assert_eq!(analyze(&["analyze", "-"]).patch, PatchSource::Stdin);
    }

    #[test]
    fn missing_patch_is_a_usage_error() {
        assert!(error(&["analyze"]).contains("missing patch"));
        assert!(error(&["optimize", "--json"]).contains("missing patch"));
    }

    #[test]
    fn second_operand_is_a_usage_error() {
        let message = error(&["analyze", "p.ini", "q.ini"]);
        assert!(message.contains("q.ini"), "message was {message}");
    }

    #[test]
    fn escape_makes_a_subcommand_name_a_path() {
        assert_eq!(
            analyze(&["analyze", "--", "optimize"]).patch,
            PatchSource::File("optimize".to_owned())
        );
        assert_eq!(
            optimize(&["optimize", "--", "analyze"]).patch,
            PatchSource::File("analyze".to_owned())
        );
    }

    #[test]
    fn escape_also_makes_help_a_path() {
        assert_eq!(
            analyze(&["analyze", "--", "--help"]).patch,
            PatchSource::File("--help".to_owned())
        );
    }

    #[test]
    fn escape_then_two_operands_is_a_usage_error() {
        assert!(error(&["analyze", "--", "optimize", "p.ini"]).contains("extra operand"));
    }

    // -- shared options ---------------------------------------------------

    #[test]
    fn json_no_plugins_and_deny_apply_to_both_commands() {
        let a = analyze(&[
            "analyze",
            "p.ini",
            "--json",
            "--no-plugins",
            "--deny",
            "warning",
        ]);
        assert!(a.json);
        assert!(a.no_plugins);
        assert_eq!(a.deny, Severity::Warning);

        let o = optimize(&[
            "optimize",
            "p.ini",
            "--json",
            "--no-plugins",
            "--deny",
            "warning",
        ]);
        assert!(o.json);
        assert!(o.no_plugins);
        assert_eq!(o.deny, Severity::Warning);
    }

    #[test]
    fn deny_defaults_to_error_and_rejects_other_values() {
        assert_eq!(analyze(&["analyze", "p.ini"]).deny, Severity::Error);
        let message = error(&["analyze", "p.ini", "--deny", "fatal"]);
        assert!(message.contains("fatal"), "message was {message}");
        assert!(message.contains("warning"), "message was {message}");
    }

    #[test]
    fn options_may_precede_the_patch() {
        let request = analyze(&["analyze", "--json", "p.ini"]);
        assert!(request.json);
        assert_eq!(request.patch, PatchSource::File("p.ini".to_owned()));
    }

    // -- optimize options -------------------------------------------------

    #[test]
    fn optimize_full_invocation() {
        assert_eq!(
            optimize(&[
                "optimize",
                "p.ini",
                "--strategy",
                "minmax",
                "--weight",
                "0.6",
                "--candidate",
                "2",
                "--stdout",
            ]),
            OptimizeRequest {
                patch: PatchSource::File("p.ini".to_owned()),
                json: false,
                no_plugins: false,
                deny: Severity::Error,
                strategy: OptimizeScope::MinMax,
                weight: 0.6,
                candidate: 2,
                output: Some(OutputTarget::Stdout),
            }
        );
    }

    #[test]
    fn optimize_defaults() {
        let request = optimize(&["optimize", "p.ini"]);
        assert_eq!(request.strategy, OptimizeScope::Banner);
        assert_eq!(request.weight, 0.0);
        assert_eq!(request.candidate, 0);
        assert_eq!(request.output, None);
    }

    #[test]
    fn strategy_accepts_every_documented_value() {
        for (name, expected) in [
            ("banner", OptimizeScope::Banner),
            ("global", OptimizeScope::Global),
            ("minmax", OptimizeScope::MinMax),
        ] {
            assert_eq!(
                optimize(&["optimize", "p.ini", "--strategy", name]).strategy,
                expected
            );
        }
        let message = error(&["optimize", "p.ini", "--strategy", "greedy"]);
        assert!(message.contains("greedy"), "message was {message}");
        assert!(message.contains("minmax"), "message was {message}");
    }

    #[test]
    fn weight_accepts_the_closed_unit_range() {
        assert_eq!(
            optimize(&["optimize", "p.ini", "--weight", "0"]).weight,
            0.0
        );
        assert_eq!(
            optimize(&["optimize", "p.ini", "--weight", "1"]).weight,
            1.0
        );
        assert_eq!(
            optimize(&["optimize", "p.ini", "--weight", "0.25"]).weight,
            0.25
        );
    }

    #[test]
    fn weight_out_of_range_is_a_usage_error() {
        for bad in ["1.5", "-0.1", "NaN", "inf"] {
            let message = error(&["optimize", "p.ini", "--weight", bad]);
            assert!(message.contains(bad), "message was {message}");
            assert!(message.contains("0.0..=1.0"), "message was {message}");
        }
    }

    #[test]
    fn weight_not_a_number_is_a_usage_error() {
        let message = error(&["optimize", "p.ini", "--weight", "heavy"]);
        assert!(message.contains("heavy"), "message was {message}");
    }

    #[test]
    fn candidate_must_be_a_non_negative_integer() {
        assert_eq!(
            optimize(&["optimize", "p.ini", "--candidate", "7"]).candidate,
            7
        );
        let message = error(&["optimize", "p.ini", "--candidate", "-1"]);
        assert!(message.contains("-1"), "message was {message}");
        let message = error(&["optimize", "p.ini", "--candidate", "two"]);
        assert!(message.contains("two"), "message was {message}");
    }

    #[test]
    fn write_records_the_destination() {
        assert_eq!(
            optimize(&["optimize", "p.ini", "--write", "out.ini"]).output,
            Some(OutputTarget::Write("out.ini".to_owned()))
        );
    }

    #[test]
    fn write_and_stdout_together_are_rejected() {
        let message = error(&["optimize", "p.ini", "--write", "a.ini", "--stdout"]);
        assert!(message.contains("--write"), "message was {message}");
        assert!(message.contains("--stdout"), "message was {message}");
        // Order of the two options must not matter.
        let message = error(&["optimize", "p.ini", "--stdout", "--write", "a.ini"]);
        assert!(
            message.contains("mutually exclusive"),
            "message was {message}"
        );
    }

    #[test]
    fn write_from_stdin_is_rejected() {
        let message = error(&["optimize", "-", "--write", "out.ini"]);
        assert!(message.contains("--write"), "message was {message}");
        assert!(message.contains("standard input"), "message was {message}");
        // `--stdout` is the documented alternative and stays valid.
        assert_eq!(
            optimize(&["optimize", "-", "--stdout"]).output,
            Some(OutputTarget::Stdout)
        );
    }

    #[test]
    fn optimize_only_options_are_rejected_under_analyze() {
        let cases: [&[&str]; 5] = [
            &["analyze", "p.ini", "--strategy", "banner"],
            &["analyze", "p.ini", "--weight", "0.5"],
            &["analyze", "p.ini", "--candidate", "1"],
            &["analyze", "p.ini", "--write", "out.ini"],
            &["analyze", "p.ini", "--stdout"],
        ];
        for case in cases {
            let option = case[2];
            let message = error(case);
            assert!(message.contains(option), "message was {message}");
            assert!(message.contains("optimize"), "message was {message}");
        }
    }

    // -- failures ---------------------------------------------------------

    #[test]
    fn unknown_option_is_a_usage_error() {
        let message = error(&["analyze", "p.ini", "--frobnicate"]);
        assert!(message.contains("--frobnicate"), "message was {message}");
        let message = error(&["optimize", "p.ini", "-x"]);
        assert!(message.contains("-x"), "message was {message}");
    }

    #[test]
    fn missing_option_value_is_a_usage_error() {
        for case in [
            &["analyze", "p.ini", "--deny"][..],
            &["optimize", "p.ini", "--strategy"][..],
            &["optimize", "p.ini", "--weight"][..],
            &["optimize", "p.ini", "--candidate"][..],
            &["optimize", "p.ini", "--write"][..],
        ] {
            let option = case[2];
            let message = error(case);
            assert!(message.contains(option), "message was {message}");
            assert!(
                message.contains("requires a value"),
                "message was {message}"
            );
        }
    }

    // -- help -------------------------------------------------------------

    #[test]
    fn help_is_a_distinct_outcome() {
        assert_eq!(parse(&args(&["--help"])), Ok(Command::Help));
        assert_eq!(parse(&args(&["-h"])), Ok(Command::Help));
        assert_eq!(parse(&args(&["analyze", "--help"])), Ok(Command::Help));
        assert_eq!(parse(&args(&["optimize", "-h"])), Ok(Command::Help));
        // Help is not an error, so it must not consume the usage trailer.
        assert!(usage().contains("optimize"));
        assert!(usage().contains("--deny"));
    }

    // -- analyze text renderer --------------------------------------------

    /// The all-forward cable chain of `fixtures/graph_latency_chain.ini`,
    /// inlined so the renderer tests need no filesystem.
    fn analyze_fixture() -> AnalyzeReport {
        let ini = concat!(
            "[clocktool]\n    output = _CLK\n",
            "[copy]\n    input = _CLK\n    output = _AUD\n",
            "[mixer]\n    input = _AUD\n    output = _MIX\n",
            "[contour]\n    gate = _MIX\n",
            "[p2b8]\n",
        );
        report_for(ini, "graph_latency_chain.ini")
    }

    /// Analyze `ini` exactly as the executor would: parse, build the graph,
    /// project the report.
    fn report_for(ini: &str, name: &str) -> AnalyzeReport {
        let patch = Patch::from_ini_str(ini, String::from(name)).expect("fixture parses");
        let graph = Graph::build_from_patch(
            &patch,
            &clusters_from_patch(&patch),
            &CostModel::default(),
            &GraphOptions::default(),
        );
        analyze_report(&patch, load_schema(), &graph, name, ini.len() as u64, &[])
    }

    #[test]
    fn analyze_text_prints_the_header_and_verdict_lines() {
        let report = analyze_fixture();
        let text = analyze_text(&report);
        let lines: Vec<&str> = text.lines().collect();

        // Header: patch name, then the four counts, in that order.
        let header = lines[0];
        assert_eq!(
            header,
            format!(
                "{} · {} sections · {} circuits · {} cables · {} hw tokens",
                report.patch.name,
                report.patch.sections,
                report.patch.circuits,
                report.patch.cables,
                report.patch.hw_tokens
            )
        );
        assert_eq!(
            (report.patch.sections, report.patch.cables),
            (5, 3),
            "header was {header}"
        );

        // Master verdict with its provenance, RAM against the budget, latency.
        assert_eq!(
            lines[1],
            format!("master: {} ({})", report.master.model, report.master.source)
        );
        assert_eq!(
            lines[2],
            format!(
                "ram: {}/{} bytes ({})",
                report.ram.used, report.ram.budget, report.ram.model
            )
        );
        let latency = &report.latency;
        assert_eq!(
            lines[3],
            format!(
                "latency: avg {:.4} · max {:.4} · {} back edges",
                latency.avg, latency.max, latency.back_edges
            )
        );
        assert!(
            lines.iter().any(|line| line.starts_with("  worst: ")),
            "text was {text}"
        );

        // Schema provenance, severity tally and the remaining tallies.
        assert!(text.contains(&format!(
            "schema: {} circuits (embedded)",
            report.schema.circuits
        )));
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("validation: ") && line.contains("error(s)")),
            "text was {text}"
        );
        assert!(
            lines.iter().any(|line| line.starts_with("topology: ")),
            "text was {text}"
        );
        assert!(
            lines.iter().any(|line| line.starts_with("groups: ")),
            "text was {text}"
        );
        // No ANSI and no trailing newline: the caller owns the line ending.
        assert!(!text.contains('\u{1b}'));
        assert!(!text.ends_with('\n'));
    }

    #[test]
    fn analyze_text_names_an_error_finding_with_line_and_column() {
        let report = report_for("[copy]\ninput = I1\nnotaparam = 5\n", "bad.ini");
        let finding = report
            .validation
            .iter()
            .find(|finding| finding.code == "unknown_param")
            .expect("the fixture must produce an unknown_param finding");
        assert_eq!(finding.severity, "error");

        let text = analyze_text(&report);
        let expected = format!(
            "  {}:{}  error  unknown_param  {}",
            finding.line, finding.col_start, finding.message
        );
        assert!(text.contains(&expected), "text was {text}");
        // The tally counts the severity, it does not just print a placeholder.
        assert!(text.contains("1 error(s)"), "text was {text}");
    }

    #[test]
    fn analyze_text_says_none_when_the_patch_has_no_cables() {
        let report = report_for("[copy]\ninput = I1\n", "one.ini");
        assert_eq!(report.patch.cables, 0);

        let text = analyze_text(&report);
        assert!(
            text.lines().any(|line| line == "latency: none"),
            "text was {text}"
        );
        assert!(!text.contains("  worst: "), "text was {text}");
    }

    // -- JSON serializer --------------------------------------------------

    #[test]
    fn to_json_round_trips_one_document() {
        let report = analyze_fixture();
        let json = to_json(&report).expect("a report serializes");

        // One pretty-printed document and no trailing newline.
        assert!(json.starts_with('{'), "json was {json}");
        assert!(json.trim_end().ends_with('}'));
        assert!(!json.ends_with('\n'));

        let value: serde_json::Value = serde_json::from_str(&json).expect("json round-trips");
        let object = value.as_object().expect("the document is an object");
        for key in ["report_version", "master", "latency", "validation"] {
            assert!(object.contains_key(key), "missing `{key}` in {json}");
        }
    }

    // -- optimize text renderer -------------------------------------------

    /// A reorderable four-section chain (as in `report`'s tests): file order
    /// d, b, a, c wired a→c→b→d.
    fn optimize_fixture() -> Patch {
        let ini = concat!(
            "[d]\nbutton = B1.1\nin = _BD\n",
            "[b]\noutput = _BD\nin = _CB\n",
            "[a]\noutput = _AC\n",
            "[c]\noutput = _CB\nin = _AC\n",
        );
        Patch::from_ini_str(ini, String::from("chain.ini")).expect("fixture parses")
    }

    #[test]
    fn optimize_text_prints_the_baseline_and_every_candidate_index() {
        let patch = optimize_fixture();
        let candidates = generate_candidates(&patch, &CostModel::default(), OptimizeScope::Banner);
        let report = optimize_report(&patch, load_schema(), &candidates, "chain.ini", 0);
        assert!(
            !report.candidates.is_empty(),
            "the reorderable fixture must yield a candidate"
        );

        let text = optimize_text(&report);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[0],
            format!(
                "chain.ini · {} sections · {} circuits · {} cables · {} hw tokens",
                report.patch.sections,
                report.patch.circuits,
                report.patch.cables,
                report.patch.hw_tokens
            )
        );
        let baseline = &report.baseline;
        assert_eq!(
            lines[1],
            format!(
                "baseline: avg {:.4} · max {:.4} · {} back edges",
                baseline.avg, baseline.max, baseline.back_edges
            )
        );
        for (index, candidate) in report.candidates.iter().enumerate() {
            assert!(
                text.contains(&format!("[{index}] {}", candidate.label)),
                "missing candidate {index} in {text}"
            );
        }
        assert!(!text.contains("candidates: none"), "text was {text}");
    }

    #[test]
    fn optimize_text_says_none_without_candidates() {
        let patch = optimize_fixture();
        let report = optimize_report(&patch, load_schema(), &[], "chain.ini", 0);

        let text = optimize_text(&report);
        assert!(
            text.lines().any(|line| line == "candidates: none"),
            "text was {text}"
        );
        assert!(
            text.lines().any(|line| line.starts_with("baseline: ")),
            "text was {text}"
        );
    }
}
