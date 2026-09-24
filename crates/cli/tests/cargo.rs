#![cfg(target_os = "macos")]
//! DG1-C08: real Cargo builds of small offline workspaces through
//! `devguard exec` with the Cargo adapters. A shell RUSTC_WRAPPER logs when
//! each compilation starts and ends and which descriptors it inherited, so the
//! observed parallelism and descriptor hygiene are measured, not assumed.

mod support;
use devguard_contract::Budget;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use support::*;

#[test]
#[ignore = "the CLI process of the Cargo tests"]
fn cli_child() {
    support::cli_child();
}

const WRAPPER: &str = r#"#!/bin/sh
log="$DEVGUARD_TEST_RUSTC_LOG"
name=""
prev=""
for arg in "$@"; do
  if [ "$prev" = "--crate-name" ]; then name="$arg"; fi
  prev="$arg"
done
if [ -z "$name" ] || [ -z "$log" ]; then exec "$@"; fi
now() { /usr/bin/perl -MTime::HiRes=time -e 'printf "%.6f", time'; }
fds=""
fd=3
while [ $fd -lt 32 ]; do
  if [ -e /dev/fd/$fd ]; then fds="$fds,$fd"; fi
  fd=$((fd+1))
done
echo "start $name $(now) $$ ${fds#,} ${CARGO_MAKEFLAGS:-}" >> "$log"
case "$name" in dgc*) sleep "${DEVGUARD_TEST_RUSTC_SLEEP:-0.4}" ;; esac
"$@"
status=$?
echo "end $name $(now) $$ $status" >> "$log"
exit $status
"#;

/// Jobs a test needs; a host whose work capacity cannot fit them records the
/// case as not run rather than passing it.
fn fits(fixture: &Fixture, budgets: &[Budget]) -> bool {
    let capacity = fixture.capacity();
    let total = budgets.iter().fold(Budget::ZERO, |sum, budget| {
        sum.checked_add(*budget).unwrap()
    });
    total.fits(capacity)
}

fn not_run(name: &str, fixture: &Fixture, budgets: &[Budget]) {
    record(
        name,
        json!({"status": "not_run",
               "reason": "this host's work capacity cannot fit the Cargo jobs the case needs",
               "capacity": fixture.capacity(), "needed": budgets}),
        &fixture.secret(),
    );
}

fn jobs_budget(jobs: u64) -> Budget {
    devguard_cargo::budget_for(jobs, 32)
}

fn budget_args(budget: Budget) -> Vec<String> {
    vec![
        "--cpu".into(),
        budget.cpu_milli.to_string(),
        "--memory".into(),
        budget.memory_bytes.to_string(),
        "--tasks".into(),
        budget.tasks.to_string(),
    ]
}

/// A workspace of `members` tiny library crates named dgc1, dgc2, and so on.
fn workspace(root: &Path, members: usize) -> PathBuf {
    std::fs::create_dir_all(root).unwrap();
    let names: Vec<String> = (1..=members).map(|index| format!("dgc{index}")).collect();
    std::fs::write(
        root.join("Cargo.toml"),
        format!(
            "[workspace]\nresolver = \"2\"\nmembers = [{}]\n",
            names
                .iter()
                .map(|name| format!("\"{name}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    )
    .unwrap();
    for (index, name) in names.iter().enumerate() {
        std::fs::create_dir_all(root.join(name).join("src")).unwrap();
        std::fs::write(
            root.join(name).join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
        )
        .unwrap();
        std::fs::write(
            root.join(name).join("src/lib.rs"),
            format!("pub fn value() -> u32 {{ {index} }}\n"),
        )
        .unwrap();
    }
    root.to_path_buf()
}

/// A `cargo` shim placed first on PATH: it records the descriptors it
/// inherited beyond the standard three, then execs the real Cargo with the
/// same arguments. It shows exactly what DevGuard handed the command.
const SHIM: &str = r#"#!/bin/sh
fds=""
fd=3
while [ $fd -lt 64 ]; do
  if [ -e /dev/fd/$fd ]; then fds="$fds,$fd"; fi
  fd=$((fd+1))
done
echo "cargo ${fds#,}" >> "$DEVGUARD_TEST_ROOT_FDS"
exec "$DEVGUARD_TEST_REAL_CARGO" "$@"
"#;

/// The same scan, for the first line of a pipeline script.
const SCAN: &str = r#"fds=""; fd=3; while [ $fd -lt 64 ]; do if [ -e /dev/fd/$fd ]; then fds="$fds,$fd"; fi; fd=$((fd+1)); done; echo "script ${fds#,}" >> "$DEVGUARD_TEST_ROOT_FDS""#;

struct Build {
    wrapper: PathBuf,
    log: PathBuf,
    target: PathBuf,
    bin: PathBuf,
    root_fds: PathBuf,
}

fn executable(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn build_files(work: &Path, name: &str) -> Build {
    let wrapper = work.join(format!("{name}-rustc-wrapper.sh"));
    executable(&wrapper, WRAPPER);
    let bin = work.join(format!("{name}-bin"));
    std::fs::create_dir_all(&bin).unwrap();
    executable(&bin.join("cargo"), SHIM);
    Build {
        wrapper,
        log: work.join(format!("{name}-rustc.log")),
        target: work.join(format!("{name}-target")),
        bin,
        root_fds: work.join(format!("{name}-root-fds.log")),
    }
}

/// The real Cargo on this process's PATH.
fn real_cargo() -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join("cargo"))
        .find(|candidate| candidate.is_file())
        .expect("cargo on PATH")
}

/// The CLI's environment for a build: the test's wrapper and log, a target
/// directory, the recording `cargo` shim first on PATH, and no jobserver
/// inherited from the harness unless a test sets one.
fn build_environment(command: &mut Command, build: &Build) {
    let path = std::env::join_paths(
        std::iter::once(build.bin.clone())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    command
        .env("PATH", path)
        .env("DEVGUARD_TEST_REAL_CARGO", real_cargo())
        .env("DEVGUARD_TEST_ROOT_FDS", &build.root_fds)
        .env("RUSTC_WRAPPER", &build.wrapper)
        .env("DEVGUARD_TEST_RUSTC_LOG", &build.log)
        .env("CARGO_TARGET_DIR", &build.target)
        .env_remove("CARGO_MAKEFLAGS")
        .env_remove("MAKEFLAGS")
        .env_remove("MFLAGS");
}

/// What each recorded process inherited: `(role, descriptors)`.
fn root_fds(build: &Build) -> Vec<(String, String)> {
    std::fs::read_to_string(&build.root_fds)
        .unwrap_or_default()
        .lines()
        .map(|line| {
            let (role, fds) = line.split_once(' ').unwrap_or((line, ""));
            (role.to_string(), fds.to_string())
        })
        .collect()
}

/// Nothing DevGuard created reached the command: every recorded process
/// inherited only its standard descriptors.
fn assert_only_standard_descriptors(build: &Build, expected_records: usize) {
    let records = root_fds(build);
    assert_eq!(records.len(), expected_records, "{records:?}");
    assert!(records.iter().all(|(_, fds)| fds.is_empty()), "{records:?}");
}

#[derive(Debug)]
struct Compilation {
    name: String,
    start: f64,
    end: Option<f64>,
    fds: BTreeSet<i32>,
    makeflags: String,
}

fn compilations(log: &Path) -> Vec<Compilation> {
    let text = std::fs::read_to_string(log).unwrap_or_default();
    let mut runs: Vec<(String, Compilation)> = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.splitn(6, ' ').collect();
        match fields[0] {
            "start" => {
                let fds = fields
                    .get(4)
                    .map(|list| {
                        list.split(',')
                            .filter(|fd| !fd.is_empty())
                            .map(|fd| fd.parse().unwrap())
                            .collect()
                    })
                    .unwrap_or_default();
                runs.push((
                    fields[3].to_string(),
                    Compilation {
                        name: fields[1].to_string(),
                        start: fields[2].parse().unwrap(),
                        end: None,
                        fds,
                        makeflags: fields.get(5).copied().unwrap_or_default().to_string(),
                    },
                ));
            }
            "end" => {
                if let Some((_, run)) = runs.iter_mut().find(|(pid, run)| {
                    pid == fields[3] && run.name == fields[1] && run.end.is_none()
                }) {
                    run.end = Some(fields[2].parse().unwrap());
                }
            }
            _ => {}
        }
    }
    runs.into_iter().map(|(_, run)| run).collect()
}

fn max_concurrency(runs: &[Compilation]) -> usize {
    let mut events: Vec<(f64, i32)> = Vec::new();
    for run in runs {
        events.push((run.start, 1));
        events.push((run.end.unwrap_or(f64::MAX), -1));
    }
    events.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let (mut current, mut peak) = (0i32, 0i32);
    for (_, delta) in events {
        current += delta;
        peak = peak.max(current);
    }
    peak as usize
}

/// Observed compilations. The descriptors Cargo leaves open to its compilers
/// are Cargo's own and vary with its jobserver; they are recorded, not judged.
fn summary(runs: &[Compilation]) -> Value {
    let compiler_fds: BTreeSet<i32> = runs
        .iter()
        .flat_map(|run| run.fds.iter().copied())
        .collect();
    let announced: BTreeSet<&str> = runs
        .iter()
        .filter(|run| run.name.starts_with("dgc"))
        .map(|run| run.makeflags.as_str())
        .collect();
    json!({"compilations": runs.len(), "max_concurrency": max_concurrency(runs),
           "compiler_descriptors_from_cargo": compiler_fds,
           "jobserver_announced_to_compilers": announced})
}

fn exec_args(adapter: &str, budget: Budget, receipt: &Path, program: &[&str]) -> Vec<String> {
    let mut args = vec!["exec".to_string(), "--adapter".into(), adapter.into()];
    args.extend(budget_args(budget));
    args.push("--receipt".into());
    args.push(receipt.to_string_lossy().into_owned());
    args.push("--".into());
    args.extend(program.iter().map(|arg| arg.to_string()));
    args
}

#[test]
fn a_direct_build_runs_within_the_reserved_jobs_and_keeps_its_paths() {
    let fixture = Fixture::start();
    let budget = jobs_budget(2);
    if !fits(&fixture, &[budget]) {
        return not_run("direct-build", &fixture, &[budget]);
    }
    let work = fixture.work("work");
    let root = workspace(&work.join("ws"), 6);
    let build = build_files(&work, "direct");
    let receipt = work.join("receipt.json");
    let args = exec_args("cargo", budget, &receipt, &["cargo", "build", "--offline"]);
    let mut cli = cli(&fixture, &args, |command| {
        quiet(command);
        command.current_dir(&root);
        build_environment(command, &build);
    });
    let status = wait_exit(&mut cli, Duration::from_secs(180));
    assert!(status.success(), "{status:?}");
    let receipt = read_json(&receipt);
    assert_eq!(receipt["adapter"]["adapter"], "cargo");
    assert_eq!(receipt["adapter"]["parallelism"], "inserted");
    assert_eq!(
        receipt["command"]["transformed_args"],
        json!(["build", "--jobs", "2", "--offline"])
    );
    let runs = compilations(&build.log);
    let members = runs
        .iter()
        .filter(|run| run.name.starts_with("dgc"))
        .count();
    assert_eq!(members, 6, "{runs:?}");
    assert!((1..=2).contains(&max_concurrency(&runs)), "{runs:?}");
    assert_only_standard_descriptors(&build, 1);
    // The target directory is the one the caller chose; none appears beside the sources.
    assert!(build.target.join("debug/libdgc1.rlib").exists());
    assert!(!root.join("target").exists());
    assert_eq!(receipt["observed_after_reap"]["phase"], "released");
    record(
        "direct-build",
        json!({"transformed_args": receipt["command"]["transformed_args"],
               "adapter": receipt["adapter"], "observed": summary(&runs),
               "target_preserved": true, "released": receipt["observed_after_reap"]["release_reason"]}),
        &fixture.secret(),
    );
}

#[test]
fn explicit_jobs_are_clamped_to_the_reservation_or_kept_within_it() {
    let fixture = Fixture::start();
    let budget = jobs_budget(2);
    if !fits(&fixture, &[budget]) {
        return not_run("explicit-jobs", &fixture, &[budget]);
    }
    let work = fixture.work("work");
    let root = workspace(&work.join("ws"), 5);
    let mut cases = Vec::new();
    for (name, given, expected, action, bound) in [
        ("clamped", "8", "2", "clamped", 2usize),
        ("kept", "1", "1", "kept", 1usize),
    ] {
        let build = build_files(&work, name);
        let receipt = work.join(format!("{name}.json"));
        let args = exec_args(
            "cargo",
            budget,
            &receipt,
            &["cargo", "build", "--offline", "-j", given],
        );
        let mut cli = cli(&fixture, &args, |command| {
            quiet(command);
            command.current_dir(&root);
            build_environment(command, &build);
        });
        assert!(wait_exit(&mut cli, Duration::from_secs(180)).success());
        let receipt = read_json(&receipt);
        assert_eq!(receipt["adapter"]["parallelism"], action);
        assert_eq!(
            receipt["command"]["transformed_args"],
            json!(["build", "--offline", "-j", expected])
        );
        assert_eq!(receipt["adapter"]["detail"]["jobs"]["original"], given);
        let runs = compilations(&build.log);
        assert!(max_concurrency(&runs) <= bound, "{name}: {runs:?}");
        assert_only_standard_descriptors(&build, 1);
        cases.push(json!({"case": name, "given": given, "applied": expected,
                          "jobs": receipt["adapter"]["detail"]["jobs"], "observed": summary(&runs)}));
    }
    record("explicit-jobs", json!({"cases": cases}), &fixture.secret());
}

#[test]
fn conflicting_jobs_unsupported_commands_and_small_reservations_start_nothing() {
    let fixture = Fixture::start();
    let work = fixture.work("work");
    let root = workspace(&work.join("ws"), 1);
    let small = Budget {
        cpu_milli: 500,
        memory_bytes: 4 << 30,
        tasks: 8,
    };
    let two = jobs_budget(2);
    let mut cases = Vec::new();
    for (name, adapter, budget, program) in [
        (
            "conflicting-jobs",
            "cargo",
            two,
            &["cargo", "build", "-j", "2", "--jobs", "3"][..],
        ),
        (
            "unsupported-subcommand",
            "cargo",
            two,
            &["cargo", "metadata"],
        ),
        (
            "reservation-below-one-job",
            "cargo",
            small,
            &["cargo", "build"],
        ),
        ("not-cargo", "cargo", two, &["/bin/echo", "build"]),
    ] {
        let receipt = work.join(format!("{name}.json"));
        let args = exec_args(adapter, budget, &receipt, program);
        let build = build_files(&work, name);
        let mut cli = cli(&fixture, &args, |command| {
            quiet(command);
            command.current_dir(&root);
            build_environment(command, &build);
        });
        assert_eq!(wait_exit(&mut cli, EXIT_LIMIT).code(), Some(125), "{name}");
        let receipt = read_json(&receipt);
        assert_eq!(receipt["result"], "not_started", "{name}");
        assert!(receipt["attempts"].as_array().unwrap().is_empty(), "{name}");
        assert!(!build.log.exists(), "{name}: nothing compiled");
        assert!(root_fds(&build).is_empty(), "{name}: nothing started");
        cases.push(json!({"case": name, "reason": receipt["reason"]}));
    }
    // Auto falls back to the generic adapter for a Cargo command that compiles nothing.
    let receipt = work.join("auto.json");
    let mut args = vec!["exec".to_string()];
    args.extend(budget_args(Budget {
        cpu_milli: 50,
        memory_bytes: 64 << 20,
        tasks: 4,
    }));
    args.extend(["--receipt".into(), receipt.to_string_lossy().into_owned()]);
    args.extend(["--".into(), "cargo".into(), "--version".into()]);
    let mut cli = cli(&fixture, &args, |command| {
        quiet(command);
        command.current_dir(&root);
    });
    assert!(wait_exit(&mut cli, EXIT_LIMIT).success());
    let auto = read_json(&receipt);
    assert_eq!(auto["adapter"]["adapter"], "generic");
    assert_eq!(auto["adapter"]["selected_by"], "auto");
    assert!(auto["adapter"]["detail"]["selection"]
        .as_str()
        .unwrap()
        .contains("compiles nothing"));
    cases.push(json!({"case": "auto-generic", "adapter": auto["adapter"]}));
    record("cargo-refusals", json!({"cases": cases}), &fixture.secret());
}

#[test]
fn a_pipeline_shares_one_jobserver_across_its_cargo_runs() {
    let fixture = Fixture::start();
    let budget = jobs_budget(3);
    if !fits(&fixture, &[budget]) {
        return not_run("pipeline-shared", &fixture, &[budget]);
    }
    let work = fixture.work("work");
    let first = workspace(&work.join("first"), 5);
    let second = workspace(&work.join("second"), 5);
    let build = build_files(&work, "pipeline");
    let report = work.join("reports/validation.txt");
    let script = work.join("pipeline.sh");
    // Two concurrent Cargo runs, each asking for more jobs than the pool has,
    // and a report written where the caller chose.
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nset -e\n{SCAN}\ncargo build --offline -j 8 --manifest-path '{}/Cargo.toml' --target-dir '{}/one' &\ncargo build --offline -j 8 --manifest-path '{}/Cargo.toml' --target-dir '{}/two' &\nwait\nmkdir -p '{}'\necho done > '{}'\n",
            first.display(),
            build.target.display(),
            second.display(),
            build.target.display(),
            report.parent().unwrap().display(),
            report.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let receipt = work.join("receipt.json");
    let args = exec_args(
        "cargo-pipeline",
        budget,
        &receipt,
        &["/bin/sh", script.to_str().unwrap()],
    );
    let mut cli = cli(&fixture, &args, |command| {
        quiet(command);
        command.current_dir(&work);
        build_environment(command, &build);
    });
    assert!(wait_exit(&mut cli, Duration::from_secs(240)).success());
    let receipt = read_json(&receipt);
    assert_eq!(receipt["adapter"]["parallelism"], "shared_jobserver");
    let jobserver = receipt["adapter_after"][0]["jobserver"].clone();
    assert_eq!(jobserver["jobs"], 3);
    assert_eq!(jobserver["tokens_after_run"], jobserver["tokens_expected"]);
    let fifo = PathBuf::from(jobserver["path"].as_str().unwrap());
    assert!(!fifo.exists(), "the jobserver is removed after the run");
    let runs = compilations(&build.log);
    assert_eq!(
        runs.iter()
            .filter(|run| run.name.starts_with("dgc"))
            .count(),
        10
    );
    // Three jobs in the pool, plus the implicit job of each of the two
    // concurrently started top-level Cargo runs.
    assert!(max_concurrency(&runs) <= 4, "{runs:?}");
    assert_only_standard_descriptors(&build, 3);
    assert!(report.exists(), "the pipeline's report path is kept");
    record(
        "pipeline-shared",
        json!({"environment_set": receipt["command"]["environment_set"],
               "adapter": receipt["adapter"], "jobserver_after": jobserver,
               "observed": summary(&runs), "unshared_bound": "16: two Cargo runs asked for 8 jobs each",
               "report_path_kept": true}),
        &fixture.secret(),
    );
}

#[test]
fn nested_cargo_in_a_build_script_shares_the_outer_jobserver() {
    let fixture = Fixture::start();
    let budget = jobs_budget(2);
    if !fits(&fixture, &[budget]) {
        return not_run("nested-cargo", &fixture, &[budget]);
    }
    let work = fixture.work("work");
    let inner = workspace(&work.join("inner"), 4);
    let outer = workspace(&work.join("outer"), 3);
    // One outer crate builds the inner workspace from its build script.
    let nested = outer.join("dgcnest");
    std::fs::create_dir_all(nested.join("src")).unwrap();
    std::fs::write(
        nested.join("Cargo.toml"),
        "[package]\nname = \"dgcnest\"\nversion = \"0.1.0\"\nedition = \"2021\"\nbuild = \"build.rs\"\n",
    )
    .unwrap();
    std::fs::write(nested.join("src/lib.rs"), "pub fn nested() {}\n").unwrap();
    std::fs::write(
        nested.join("build.rs"),
        format!(
            "fn main() {{\n    let out = std::env::var(\"OUT_DIR\").unwrap();\n    let status = std::process::Command::new(std::env::var(\"CARGO\").unwrap())\n        .args([\"build\", \"--offline\", \"--manifest-path\", \"{}/Cargo.toml\", \"--target-dir\"])\n        .arg(format!(\"{{out}}/inner\"))\n        .status()\n        .unwrap();\n    assert!(status.success());\n}}\n",
            inner.display()
        ),
    )
    .unwrap();
    let manifest = std::fs::read_to_string(outer.join("Cargo.toml")).unwrap();
    std::fs::write(
        outer.join("Cargo.toml"),
        manifest.replace("members = [", "members = [\"dgcnest\", "),
    )
    .unwrap();
    let build = build_files(&work, "nested");
    let receipt = work.join("receipt.json");
    let args = exec_args("cargo", budget, &receipt, &["cargo", "build", "--offline"]);
    let mut cli = cli(&fixture, &args, |command| {
        quiet(command);
        command.current_dir(&outer);
        build_environment(command, &build);
    });
    assert!(wait_exit(&mut cli, Duration::from_secs(240)).success());
    let runs = compilations(&build.log);
    let inner_runs = runs
        .iter()
        .filter(|run| run.name.starts_with("dgc") && run.name != "dgcnest")
        .count();
    // The nested Cargo runs as $CARGO, the real binary, so only the outer
    // launch is recorded by the shim.
    assert_only_standard_descriptors(&build, 1);
    // Three outer members, four inner members and the nesting crate itself.
    assert_eq!(inner_runs, 7, "{runs:?}");
    assert!(max_concurrency(&runs) <= 2, "{runs:?}");
    let receipt = read_json(&receipt);
    record(
        "nested-cargo",
        json!({"adapter": receipt["adapter"], "observed": summary(&runs),
               "bound": "the two jobs of the outer reservation, shared through Cargo's own jobserver"}),
        &fixture.secret(),
    );
}

#[test]
fn an_inherited_jobserver_bounds_the_build_and_is_preserved() {
    let fixture = Fixture::start();
    let budget = jobs_budget(4);
    if !fits(&fixture, &[budget]) {
        return not_run("inherited-jobserver", &fixture, &[budget]);
    }
    let work = fixture.work("work");
    let root = workspace(&work.join("ws"), 6);
    // A caller's pool of two jobs: one token plus the implicit job.
    let outer = devguard_cargo::Jobserver::create(2).unwrap();
    let build = build_files(&work, "inherited");
    let receipt = work.join("receipt.json");
    let args = exec_args("cargo", budget, &receipt, &["cargo", "build", "--offline"]);
    let mut cli = cli(&fixture, &args, |command| {
        quiet(command);
        command.current_dir(&root);
        build_environment(command, &build);
        command.env(
            "CARGO_MAKEFLAGS",
            format!("-j --jobserver-auth={}", outer.auth()),
        );
    });
    assert!(wait_exit(&mut cli, Duration::from_secs(180)).success());
    let receipt = read_json(&receipt);
    assert_eq!(receipt["adapter"]["parallelism"], "inherited_jobserver");
    assert_eq!(
        receipt["command"]["transformed_args"],
        json!(["build", "--offline"])
    );
    assert_eq!(receipt["command"]["environment_set"], json!({}));
    let runs = compilations(&build.log);
    assert!(max_concurrency(&runs) <= 2, "{runs:?}");
    assert_only_standard_descriptors(&build, 1);
    assert_eq!(
        outer.available(),
        1,
        "the build returned the caller's token"
    );
    record(
        "inherited-jobserver",
        json!({"adapter": receipt["adapter"], "observed": summary(&runs),
               "caller_pool_jobs": 2, "tokens_after_run": outer.available()}),
        &fixture.secret(),
    );
}

#[test]
fn stale_inherited_descriptors_are_removed_before_cargo_runs() {
    let fixture = Fixture::start();
    let budget = jobs_budget(2);
    if !fits(&fixture, &[budget]) {
        return not_run("stale-jobserver", &fixture, &[budget]);
    }
    let work = fixture.work("work");
    let root = workspace(&work.join("ws"), 4);
    let build = build_files(&work, "stale");
    let receipt = work.join("receipt.json");
    let stderr = work.join("stderr.txt");
    let args = exec_args("cargo", budget, &receipt, &["cargo", "build", "--offline"]);
    let mut cli = cli(&fixture, &args, |command| {
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::fs::File::create(&stderr).unwrap());
        command.current_dir(&root);
        build_environment(command, &build);
        // Descriptors 97 and 98 are not open in the CLI: the references are stale.
        command
            .env(
                "CARGO_MAKEFLAGS",
                "-j4 --jobserver-fds=97,98 --jobserver-auth=97,98",
            )
            .env("MAKEFLAGS", "-k -j --jobserver-auth=97,98");
    });
    assert!(wait_exit(&mut cli, Duration::from_secs(180)).success());
    let receipt = read_json(&receipt);
    assert_eq!(
        receipt["command"]["environment_removed"],
        json!(["CARGO_MAKEFLAGS"])
    );
    assert_eq!(
        receipt["command"]["environment_set"],
        json!({"MAKEFLAGS": "-k"})
    );
    assert_eq!(receipt["adapter"]["parallelism"], "inserted");
    assert_only_standard_descriptors(&build, 1);
    let text = std::fs::read_to_string(&stderr).unwrap();
    assert!(
        !text.contains("failed to connect to jobserver"),
        "Cargo saw a stale jobserver: {text}"
    );
    let runs = compilations(&build.log);
    assert!(max_concurrency(&runs) <= 2, "{runs:?}");
    record(
        "stale-jobserver",
        json!({"environment_removed": receipt["command"]["environment_removed"],
               "environment_set": receipt["command"]["environment_set"],
               "jobserver": receipt["adapter"]["detail"]["jobserver"], "observed": summary(&runs)}),
        &fixture.secret(),
    );
}

#[test]
fn concurrent_consumers_each_build_within_their_own_reservation() {
    let fixture = Fixture::start();
    let budget = jobs_budget(1);
    if !fits(&fixture, &[budget, budget]) {
        return not_run("concurrent-consumers", &fixture, &[budget, budget]);
    }
    let work = fixture.work("work");
    let mut running = Vec::new();
    for name in ["left", "right"] {
        let root = workspace(&work.join(name), 5);
        let build = build_files(&work, name);
        let receipt = work.join(format!("{name}.json"));
        let args = exec_args("cargo", budget, &receipt, &["cargo", "build", "--offline"]);
        let cli = cli(&fixture, &args, |command| {
            quiet(command);
            command.current_dir(&root);
            build_environment(command, &build);
        });
        running.push((cli, build, receipt));
    }
    // Both reservations are charged at once, each within the host's capacity.
    let mut peak = Budget::ZERO;
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while std::time::Instant::now() < deadline && peak != budget.checked_add(budget).unwrap() {
        let committed = fixture.authority.committed().unwrap();
        if committed.cpu_milli > peak.cpu_milli {
            peak = committed;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut observed = Vec::new();
    for (mut cli, build, receipt) in running {
        assert!(wait_exit(&mut cli, Duration::from_secs(180)).success());
        let runs = compilations(&build.log);
        assert!(max_concurrency(&runs) <= 1, "{runs:?}");
        assert_only_standard_descriptors(&build, 1);
        let receipt = read_json(&receipt);
        assert_eq!(
            receipt["command"]["transformed_args"],
            json!(["build", "--jobs", "1", "--offline"])
        );
        observed.push(summary(&runs));
    }
    assert_eq!(peak, budget.checked_add(budget).unwrap());
    record(
        "concurrent-consumers",
        json!({"peak_committed": peak, "each": observed}),
        &fixture.secret(),
    );
}

#[test]
fn cancelling_a_pipeline_stops_its_builds_and_removes_the_jobserver() {
    let fixture = Fixture::start();
    let budget = jobs_budget(2);
    if !fits(&fixture, &[budget]) {
        return not_run("pipeline-cancelled", &fixture, &[budget]);
    }
    let work = fixture.work("work");
    let root = workspace(&work.join("ws"), 6);
    let build = build_files(&work, "cancel");
    let receipt = work.join("receipt.json");
    let args = exec_args(
        "cargo-pipeline",
        budget,
        &receipt,
        &["cargo", "build", "--offline"],
    );
    let mut cli = cli(&fixture, &args, |command| {
        quiet(command);
        command.current_dir(&root);
        build_environment(command, &build);
        command.env("DEVGUARD_TEST_RUSTC_SLEEP", "5");
    });
    wait_for(Duration::from_secs(120), || {
        compilations(&build.log)
            .iter()
            .any(|run| run.name.starts_with("dgc"))
    });
    // SAFETY: signalling the CLI process this test started.
    assert_eq!(unsafe { libc::kill(cli.id() as i32, libc::SIGINT) }, 0);
    let status = wait_exit(&mut cli, Duration::from_secs(60));
    assert_eq!(status.signal(), Some(libc::SIGINT), "{status:?}");
    let receipt = read_json(&receipt);
    let jobserver = receipt["adapter_after"][0]["jobserver"].clone();
    let fifo = PathBuf::from(jobserver["path"].as_str().unwrap());
    assert!(
        !fifo.exists(),
        "the jobserver is removed after a cancelled run"
    );
    // Every member compilation was interrupted; only Cargo's own quick probe
    // of the compiler may have finished.
    let runs = compilations(&build.log);
    assert!(
        runs.iter()
            .filter(|run| run.name.starts_with("dgc"))
            .all(|run| run.end.is_none()),
        "{runs:?}"
    );
    assert_only_standard_descriptors(&build, 1);
    wait_for(Duration::from_secs(30), || {
        fixture.authority.committed().unwrap() == Budget::ZERO
    });
    record(
        "pipeline-cancelled",
        json!({"cli_signal": status.signal(), "signals": receipt["signals"],
               "jobserver_after": jobserver, "started": runs.len(), "released": true}),
        &fixture.secret(),
    );
}
