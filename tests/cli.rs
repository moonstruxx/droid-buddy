//! End-to-end contract tests for the headless CLI (`analyze`, `optimize`).
//!
//! Every test spawns the real binary, so these pin the process boundary — exit
//! codes, stream discipline and the JSON contract — rather than the in-memory
//! `cli::execute` surface already covered by the unit tests in `src/cli.rs`.
//!
//! Isolation: each child gets `HOME`, `XDG_CONFIG_HOME` and `APPDATA` pointed at
//! a temporary directory, so no test reads the developer's config or plugins and
//! nothing is written outside that directory. Repository fixtures are only ever
//! read; files that change (`--write`) live in a temporary copy.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use droid_tui::patch::Patch;

/// Run the binary with `args` under a fresh, empty platform data directory.
fn run(args: &[&str]) -> Output {
    let dir = tempfile::tempdir().expect("create an isolated data directory");
    run_env(args, dir.path())
}

/// Run the binary with `args`, resolving every platform data directory to `dir`
/// on all three platforms the resolver supports.
fn run_env(args: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_droid_tui"))
        .args(args)
        .env("HOME", dir)
        .env("XDG_CONFIG_HOME", dir)
        .env("APPDATA", dir)
        .output()
        .expect("spawn the droid_tui binary")
}

/// The child's standard output as text.
fn stdout_text(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).expect("stdout is UTF-8")
}

/// The child's standard error as text.
fn stderr_text(output: &Output) -> &str {
    std::str::from_utf8(&output.stderr).expect("stderr is UTF-8")
}

/// The child's exit code.
fn exit_code(output: &Output) -> i32 {
    output.status.code().expect("the child exited normally")
}

/// Parse standard output as the single JSON document the command printed.
fn json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).expect("stdout is one JSON document")
}

/// Absolute path of a repository fixture.
fn fixture(relative: &str) -> String {
    format!("{}/{relative}", env!("CARGO_MANIFEST_DIR"))
}

/// The `candidates` list of an `optimize` report.
fn candidates(report: &serde_json::Value) -> &[serde_json::Value] {
    report["candidates"]
        .as_array()
        .expect("candidates is a list")
}

/// A candidate's half-open section-index permutation.
fn section_order(candidate: &serde_json::Value) -> Vec<u64> {
    candidate["order"]
        .as_array()
        .expect("order is a list")
        .iter()
        .map(|index| index.as_u64().expect("order entries are section indices"))
        .collect()
}

/// A candidate's strategy label.
fn label(candidate: &serde_json::Value) -> &str {
    candidate["label"].as_str().expect("label is a string")
}

// ---------------------------------------------------------------------------
// Analysis contract (task 4.1)
// ---------------------------------------------------------------------------

#[test]
fn analyze_json_report_exposes_required_top_level_fields() {
    let patch = fixture("fixtures/own_buttons.ini");
    let output = run(&["analyze", patch.as_str(), "--json"]);

    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr_text(&output));
    assert!(
        output.stderr.is_empty(),
        "a clean analysis must not warn: {}",
        stderr_text(&output)
    );

    let report = json(&output);
    for field in [
        "report_version",
        "tool",
        "patch",
        "schema",
        "master",
        "ram",
        "validation",
        "topology",
        "latency",
        "groups",
    ] {
        assert!(
            report.get(field).is_some(),
            "missing top-level field `{field}`: {report}"
        );
    }

    assert_eq!(report["report_version"], 1);
    assert_eq!(report["tool"]["name"], "droid_tui");
    assert_eq!(report["patch"]["path"], patch.as_str());
    assert_eq!(report["patch"]["sections"], 10);
    assert!(
        report["patch"]["bytes"]
            .as_u64()
            .expect("bytes is a number")
            > 0
    );

    assert_eq!(report["schema"]["source"], "embedded");
    assert!(report["schema"]["plugins"]
        .as_array()
        .expect("plugins is a list")
        .is_empty());
    assert!(
        report["schema"]["circuits"]
            .as_u64()
            .expect("circuits is a number")
            > 0
    );

    let model = report["master"]["model"]
        .as_str()
        .expect("master.model is a string");
    assert!(
        matches!(model, "MASTER" | "MASTER18"),
        "unexpected master model: {model}"
    );
    let source = report["master"]["source"]
        .as_str()
        .expect("master.source is a string");
    assert!(
        matches!(source, "declared" | "inferred" | "default"),
        "unexpected master source: {source}"
    );
    assert!(report["master"]["satisfiable"].is_boolean());

    assert!(
        report["ram"]["budget"]
            .as_u64()
            .expect("ram.budget is a number")
            > 0
    );
    assert!(report["validation"].is_array());
    assert!(report["topology"].is_array());

    let worst = report["latency"]["worst"]
        .as_array()
        .expect("latency.worst is a list");
    assert!(worst.len() <= 10, "worst-edge list must stay bounded");
    assert!(report["latency"]["avg"].is_number());
    assert!(report["latency"]["max"].is_number());

    assert!(
        !report["groups"]
            .as_array()
            .expect("groups is a list")
            .is_empty(),
        "the fixture has an implicit banner group"
    );
}

#[test]
fn analyze_error_finding_exits_one_with_the_report_still_printed() {
    let patch = fixture("fixtures/validation/unknown_param.ini");
    let output = run(&["analyze", patch.as_str(), "--json"]);

    assert_eq!(
        exit_code(&output),
        1,
        "an error-severity finding must fail the run; stderr: {}",
        stderr_text(&output)
    );

    let report = json(&output);
    let validation = report["validation"]
        .as_array()
        .expect("validation is a list");
    assert_eq!(
        validation
            .iter()
            .filter(|finding| finding["severity"] == "error")
            .count(),
        1
    );
    let error = validation
        .iter()
        .find(|finding| finding["severity"] == "error")
        .expect("the error finding is reported");
    assert_eq!(error["code"], "unknown_param");
    assert_eq!(error["line"], 3);
    assert!(
        report.get("patch").is_some(),
        "the report is printed on exit 1"
    );
}

#[test]
fn analyze_missing_path_exits_two_with_empty_stdout_and_a_reason() {
    let missing = format!("{}/does-not-exist.ini", env!("CARGO_MANIFEST_DIR"));
    let output = run(&["analyze", missing.as_str()]);

    assert_eq!(exit_code(&output), 2);
    assert!(
        output.stdout.is_empty(),
        "stdout must carry nothing on exit 2: {}",
        stdout_text(&output)
    );
    let stderr = stderr_text(&output);
    assert!(!stderr.is_empty(), "the failure reason belongs on stderr");
    assert!(
        stderr.contains("does-not-exist.ini"),
        "the reason must name the path: {stderr}"
    );
}

#[test]
fn analyze_repeated_runs_are_byte_identical() {
    let dir = tempfile::tempdir().expect("create an isolated data directory");
    let patch = fixture("fixtures/own_buttons.ini");

    let json_args = ["analyze", patch.as_str(), "--json"];
    let first_json = run_env(&json_args, dir.path());
    let second_json = run_env(&json_args, dir.path());
    assert_eq!(exit_code(&first_json), 0);
    assert_eq!(exit_code(&second_json), 0);
    assert_eq!(
        first_json.stdout, second_json.stdout,
        "the JSON report must be byte-identical across runs"
    );

    let text_args = ["analyze", patch.as_str()];
    let first_text = run_env(&text_args, dir.path());
    let second_text = run_env(&text_args, dir.path());
    assert_eq!(exit_code(&first_text), 0);
    assert_eq!(exit_code(&second_text), 0);
    assert_eq!(
        first_text.stdout, second_text.stdout,
        "the human summary must be byte-identical across runs"
    );
}

#[test]
fn no_plugins_reports_the_embedded_schema_against_a_plugin_directory() {
    let dir = tempfile::tempdir().expect("create an isolated data directory");
    let plugins = dir.path().join("droid-tui").join("plugins");
    fs::create_dir_all(&plugins).expect("create the plugin directory");
    fs::copy(
        fixture("fixtures/plugins/valid.toml"),
        plugins.join("valid.toml"),
    )
    .expect("install the plugin fixture");

    let patch = fixture("fixtures/own_buttons.ini");
    let with_plugins = run_env(&["analyze", patch.as_str(), "--json"], dir.path());
    assert_eq!(
        exit_code(&with_plugins),
        0,
        "stderr: {}",
        stderr_text(&with_plugins)
    );
    let with_plugins = json(&with_plugins);
    assert_eq!(with_plugins["schema"]["source"], "embedded+plugins");
    let applied = with_plugins["schema"]["plugins"]
        .as_array()
        .expect("plugins is a list");
    assert_eq!(applied.len(), 1, "one plugin file was installed");
    let applied = applied[0].as_str().expect("a plugin entry is a path");
    assert!(
        applied.ends_with("valid.toml"),
        "the report must name the applied file: {applied}"
    );
    assert!(
        Path::new(applied).starts_with(dir.path()),
        "the report must name the temporary plugin: {applied}"
    );

    let embedded_only = run_env(
        &["analyze", patch.as_str(), "--json", "--no-plugins"],
        dir.path(),
    );
    assert_eq!(
        exit_code(&embedded_only),
        0,
        "stderr: {}",
        stderr_text(&embedded_only)
    );
    let embedded_only = json(&embedded_only);
    assert_eq!(embedded_only["schema"]["source"], "embedded");
    assert!(embedded_only["schema"]["plugins"]
        .as_array()
        .expect("plugins is a list")
        .is_empty());
    assert!(
        with_plugins["schema"]["circuits"]
            .as_u64()
            .expect("circuits is a number")
            > embedded_only["schema"]["circuits"]
                .as_u64()
                .expect("circuits is a number"),
        "the plugin overlay must actually add its circuit"
    );
}

// ---------------------------------------------------------------------------
// Optimization contract (task 4.2)
// ---------------------------------------------------------------------------

#[test]
fn optimize_reports_a_baseline_and_a_reordering_candidate() {
    let patch = fixture("fixtures/optimizer_latency.ini");
    let output = run(&["optimize", patch.as_str(), "--json"]);

    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr_text(&output));
    assert!(
        output.stderr.is_empty(),
        "a clean optimization must not warn: {}",
        stderr_text(&output)
    );

    let report = json(&output);
    let list = candidates(&report);
    assert!(!list.is_empty(), "the fixture must yield a candidate");

    // The baseline is the file-order latency and is shared by every candidate.
    let baseline = &report["baseline"];
    for field in ["avg", "max", "back_edges"] {
        assert_eq!(
            baseline[field], list[0]["before"][field],
            "baseline.{field} must be the candidate's before value"
        );
    }

    let sections = usize::try_from(
        report["patch"]["sections"]
            .as_u64()
            .expect("sections is a number"),
    )
    .expect("section count fits in usize");
    let first_order = section_order(&list[0]);
    let mut sorted = first_order.clone();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        (0..sections as u64).collect::<Vec<u64>>(),
        "the candidate must permute every section index"
    );
    assert_ne!(
        first_order,
        (0..sections as u64).collect::<Vec<u64>>(),
        "the fixture is chosen so the best candidate reorders it"
    );
    assert!(
        list[0]["delta_avg"]
            .as_f64()
            .expect("delta_avg is a number")
            < 0.0,
        "the reordering must reduce the summed latency"
    );

    // The human summary names the baseline and indexes the candidates.
    let text = run(&["optimize", patch.as_str()]);
    assert_eq!(exit_code(&text), 0, "stderr: {}", stderr_text(&text));
    let text = stdout_text(&text);
    assert!(text.contains("baseline:"), "summary: {text}");
    assert!(text.contains("[0]"), "summary: {text}");
}

#[test]
fn optimize_write_saves_a_candidate_and_reports_the_written_path() {
    let dir = tempfile::tempdir().expect("create an isolated data directory");
    let source = dir.path().join("source.ini");
    fs::copy(fixture("fixtures/optimizer_latency.ini"), &source).expect("copy the fixture");
    let destination = dir.path().join("optimized.ini");

    let source_arg = source.to_str().expect("temporary path is UTF-8");
    let destination_arg = destination.to_str().expect("temporary path is UTF-8");
    let output = run_env(
        &["optimize", source_arg, "--json", "--write", destination_arg],
        dir.path(),
    );

    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr_text(&output));
    let report = json(&output);
    let wrote = candidates(&report)[0]["wrote"]
        .as_str()
        .expect("the selected candidate reports the path it wrote");
    assert_eq!(Path::new(wrote), destination.as_path());

    let written = fs::read_to_string(&destination).expect("the candidate file exists");
    let parsed = Patch::from_ini_str(&written, String::from("written")).expect("the file parses");
    assert_eq!(
        parsed.sections.len() as u64,
        report["patch"]["sections"]
            .as_u64()
            .expect("sections is a number"),
        "the written file carries every source section"
    );
}

#[test]
fn optimize_write_suffixes_a_colliding_destination_and_names_it() {
    let dir = tempfile::tempdir().expect("create an isolated data directory");
    let source = dir.path().join("source.ini");
    fs::copy(fixture("fixtures/optimizer_latency.ini"), &source).expect("copy the fixture");
    let destination = dir.path().join("optimized.ini");
    let source_arg = source.to_str().expect("temporary path is UTF-8");
    let destination_arg = destination.to_str().expect("temporary path is UTF-8");

    let first = run_env(
        &["optimize", source_arg, "--json", "--write", destination_arg],
        dir.path(),
    );
    assert_eq!(exit_code(&first), 0, "stderr: {}", stderr_text(&first));
    let existing = fs::read_to_string(&destination).expect("the first write created the file");

    let second = run_env(
        &["optimize", source_arg, "--json", "--write", destination_arg],
        dir.path(),
    );
    assert_eq!(exit_code(&second), 0, "stderr: {}", stderr_text(&second));
    let report = json(&second);
    let wrote = candidates(&report)[0]["wrote"]
        .as_str()
        .expect("the report names the path actually written");
    let suffixed = Path::new(wrote);

    assert_ne!(suffixed, destination.as_path());
    assert_eq!(suffixed.parent(), Some(dir.path()));
    assert_eq!(
        suffixed.file_name().and_then(std::ffi::OsStr::to_str),
        Some("optimized-1.ini")
    );
    assert!(suffixed.exists(), "the suffixed file is written");
    assert_eq!(
        fs::read_to_string(&destination).expect("the original destination is readable"),
        existing,
        "an existing destination must not be overwritten"
    );
    assert_eq!(
        fs::read_to_string(suffixed).expect("the suffixed file is readable"),
        existing,
        "both files carry the same candidate"
    );
}

#[test]
fn optimize_write_naming_the_source_exits_two_and_leaves_it_unchanged() {
    let dir = tempfile::tempdir().expect("create an isolated data directory");
    let source = dir.path().join("source.ini");
    fs::copy(fixture("fixtures/optimizer_latency.ini"), &source).expect("copy the fixture");
    let before = fs::read(&source).expect("read the source before the run");
    let source_arg = source.to_str().expect("temporary path is UTF-8");

    let output = run_env(
        &["optimize", source_arg, "--json", "--write", source_arg],
        dir.path(),
    );

    assert_eq!(exit_code(&output), 2);
    assert!(
        output.stdout.is_empty(),
        "stdout must stay empty on exit 2: {}",
        stdout_text(&output)
    );
    let stderr = stderr_text(&output);
    assert!(!stderr.is_empty(), "the refusal belongs on stderr");
    assert!(
        stderr.contains(source_arg),
        "the refusal must name the source: {stderr}"
    );
    assert_eq!(
        fs::read(&source).expect("read the source after the run"),
        before,
        "the source must be untouched"
    );
    assert!(
        !dir.path().join("source-1.ini").exists(),
        "a refused write must not create a suffixed copy"
    );
}

#[test]
fn optimize_stdout_emits_the_selected_candidate_patch() {
    let dir = tempfile::tempdir().expect("create an isolated data directory");
    let patch = fixture("fixtures/optimizer_latency.ini");

    let emitted = run_env(&["optimize", patch.as_str(), "--stdout"], dir.path());
    assert_eq!(exit_code(&emitted), 0, "stderr: {}", stderr_text(&emitted));
    let text = stdout_text(&emitted);
    assert!(
        !text.trim_start().starts_with('{'),
        "stdout carries the patch, not a JSON report: {text}"
    );

    let source_text = fs::read_to_string(&patch).expect("read the fixture");
    let source =
        Patch::from_ini_str(&source_text, String::from("source")).expect("the fixture parses");
    let parsed = Patch::from_ini_str(text, String::from("emitted")).expect("emitted text parses");
    assert_eq!(parsed.sections.len(), source.sections.len());

    let report = run_env(&["optimize", patch.as_str(), "--json"], dir.path());
    let report = json(&report);
    let order = section_order(&candidates(&report)[0]);
    let expected: Vec<&str> = order
        .iter()
        .map(|&index| {
            source.sections[usize::try_from(index).expect("section index fits in usize")]
                .name
                .as_str()
        })
        .collect();
    let got: Vec<&str> = parsed
        .sections
        .iter()
        .map(|section| section.name.as_str())
        .collect();
    assert_eq!(got, expected, "the emitted order is the candidate order");
    let original: Vec<&str> = source
        .sections
        .iter()
        .map(|section| section.name.as_str())
        .collect();
    assert_ne!(got, original, "the emitted patch is actually reordered");
}

#[test]
fn optimize_stdout_with_json_is_a_usage_error() {
    let patch = fixture("fixtures/optimizer_latency.ini");
    let output = run(&["optimize", patch.as_str(), "--stdout", "--json"]);

    assert_eq!(exit_code(&output), 2);
    assert!(
        output.stdout.is_empty(),
        "stdout must not carry a corrupted report: {}",
        stdout_text(&output)
    );
    assert!(!stderr_text(&output).is_empty());
}

#[test]
fn optimize_strategy_selects_the_candidate_set() {
    let dir = tempfile::tempdir().expect("create an isolated data directory");
    let patch = fixture("fixtures/optimizer_latency.ini");

    let banner = run_env(&["optimize", patch.as_str(), "--json"], dir.path());
    assert_eq!(exit_code(&banner), 0, "stderr: {}", stderr_text(&banner));
    let banner = json(&banner);
    assert_eq!(candidates(&banner).len(), 1);
    assert!(
        label(&candidates(&banner)[0]).contains("banner"),
        "default strategy is banner-preserving: {:?}",
        label(&candidates(&banner)[0])
    );

    let global = run_env(
        &["optimize", patch.as_str(), "--json", "--strategy", "global"],
        dir.path(),
    );
    assert_eq!(exit_code(&global), 0, "stderr: {}", stderr_text(&global));
    let global = json(&global);
    assert_eq!(candidates(&global).len(), 2);
    assert!(
        candidates(&global)
            .iter()
            .any(|candidate| label(candidate).contains("global")),
        "--strategy global must report the global min-sum candidate"
    );

    let minmax = run_env(
        &["optimize", patch.as_str(), "--json", "--strategy", "minmax"],
        dir.path(),
    );
    assert_eq!(exit_code(&minmax), 0, "stderr: {}", stderr_text(&minmax));
    let minmax = json(&minmax);
    assert_eq!(candidates(&minmax).len(), 3);
    assert!(
        candidates(&minmax)
            .iter()
            .any(|candidate| label(candidate).contains("annealing")),
        "--strategy minmax must include the min-max search"
    );
}

#[test]
fn optimize_weight_is_accepted_and_matches_the_default_at_zero() {
    let dir = tempfile::tempdir().expect("create an isolated data directory");
    let patch = fixture("fixtures/optimizer_latency.ini");

    let plain = run_env(&["optimize", patch.as_str(), "--json"], dir.path());
    assert_eq!(exit_code(&plain), 0, "stderr: {}", stderr_text(&plain));

    // `--weight 0` is documented as replaying the unweighted default objective,
    // and the report records the weight it searched with, so the only difference
    // between the two documents is that recorded value.
    let zero = run_env(
        &["optimize", patch.as_str(), "--json", "--weight", "0"],
        dir.path(),
    );
    assert_eq!(exit_code(&zero), 0, "stderr: {}", stderr_text(&zero));
    assert_eq!(
        zero.stdout, plain.stdout,
        "weight 0 must replay the unweighted default exactly"
    );

    // The weight is part of the contract, so a non-zero weight is visible in
    // the report rather than only in the command line that produced it.
    let weighted = run_env(
        &[
            "optimize",
            patch.as_str(),
            "--json",
            "--strategy",
            "minmax",
            "--weight",
            "1",
        ],
        dir.path(),
    );
    assert_eq!(
        exit_code(&weighted),
        0,
        "the weighted objective must be accepted; stderr: {}",
        stderr_text(&weighted)
    );
    let weighted = json(&weighted);
    assert_eq!(
        weighted["strategy"].as_str(),
        Some("minmax"),
        "the report must name the strategy that searched"
    );
    assert_eq!(
        weighted["weight"].as_f64(),
        Some(1.0),
        "the report must carry the objective weight it searched with"
    );
    let list = candidates(&weighted);
    assert_eq!(list.len(), 3);
    for candidate in list {
        let before = candidate["before"]["avg"]
            .as_f64()
            .expect("before.avg is a number");
        let after = candidate["after"]["avg"]
            .as_f64()
            .expect("after.avg is a number");
        let delta = candidate["delta_avg"]
            .as_f64()
            .expect("delta_avg is a number");
        let difference = (after - before - delta).abs();
        assert!(
            difference < 1e-5,
            "delta_avg must be after - before for `{}`: {difference}",
            label(candidate)
        );
    }
}

#[test]
fn optimize_candidate_selects_the_written_candidate() {
    let dir = tempfile::tempdir().expect("create an isolated data directory");
    let source = dir.path().join("source.ini");
    fs::copy(fixture("fixtures/optimizer_latency.ini"), &source).expect("copy the fixture");
    let destination = dir.path().join("candidate-one.ini");
    let source_arg = source.to_str().expect("temporary path is UTF-8");
    let destination_arg = destination.to_str().expect("temporary path is UTF-8");

    let output = run_env(
        &[
            "optimize",
            source_arg,
            "--json",
            "--strategy",
            "minmax",
            "--candidate",
            "1",
            "--write",
            destination_arg,
        ],
        dir.path(),
    );

    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr_text(&output));
    let report = json(&output);
    let list = candidates(&report);
    assert_eq!(list.len(), 3);
    assert_eq!(
        list[1]["wrote"].as_str(),
        Some(destination_arg),
        "the selected candidate owns the written path"
    );
    assert!(
        list[0]["wrote"].is_null() && list[2]["wrote"].is_null(),
        "the other candidates report no path"
    );

    let written = fs::read_to_string(&destination).expect("the selected candidate was written");
    let parsed = Patch::from_ini_str(&written, String::from("written")).expect("the file parses");
    let source_text = fs::read_to_string(&source).expect("read the source");
    let source =
        Patch::from_ini_str(&source_text, String::from("source")).expect("the source parses");
    let expected: Vec<&str> = section_order(&list[1])
        .iter()
        .map(|&index| {
            source.sections[usize::try_from(index).expect("section index fits in usize")]
                .name
                .as_str()
        })
        .collect();
    let got: Vec<&str> = parsed
        .sections
        .iter()
        .map(|section| section.name.as_str())
        .collect();
    assert_eq!(
        got, expected,
        "--candidate 1 must write candidate 1's order"
    );
}

/// The agent skill's report reference must name every field the report emits.
///
/// The reference is prose an agent reads instead of the source, so an
/// undocumented key is silently invisible to it; this check is deliberately a
/// plain containment test rather than a parser, so the file stays readable.
#[test]
fn report_reference_documents_every_top_level_field() {
    let dir = tempfile::tempdir().expect("create an isolated data directory");
    let reference = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".agents/skills/droid-patch-cli/references/report.md"),
    )
    .expect("the skill's report reference exists");

    let analyze = run_env(
        &[
            "analyze",
            fixture("fixtures/own_buttons.ini").as_str(),
            "--json",
        ],
        dir.path(),
    );
    assert_eq!(exit_code(&analyze), 0, "stderr: {}", stderr_text(&analyze));

    let optimize = run_env(
        &[
            "optimize",
            fixture("fixtures/optimizer_latency.ini").as_str(),
            "--json",
        ],
        dir.path(),
    );
    assert_eq!(
        exit_code(&optimize),
        0,
        "stderr: {}",
        stderr_text(&optimize)
    );

    let mut missing: Vec<String> = Vec::new();
    for document in [json(&analyze), json(&optimize)] {
        for key in document
            .as_object()
            .expect("the report is an object")
            .keys()
        {
            let documented = reference.contains(&format!("`{key}`"));
            if !documented && !missing.contains(key) {
                missing.push(key.clone());
            }
        }
    }
    assert!(
        missing.is_empty(),
        "undocumented report field(s) in references/report.md: {missing:?}"
    );
}
