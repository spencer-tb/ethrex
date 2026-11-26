//! Run `blockchain_test_engine` fixtures through ethrex's own block builder, in
//! process: each payload a fixture expects to be valid is first built through
//! `testing_buildBlockV1`, on the payload's parent and from its attributes,
//! transactions and extra data, and compared with the fixture's, field by field.
//! The fixture's payload is then imported as `enginetest` imports it. Payloads
//! the fixture expects to be invalid are skipped.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use clap::Parser;
use ef_tests_engine::{BuildOutcome, EngineFixtureFile, RunOptions, run_build_fixture};
use ef_tests_runners::{
    FixtureResult, RunnerArgs, collect_json_files, report, run_files, runner_error,
};

#[derive(Parser, Debug)]
#[command(
    name = "ethrex-buildtest",
    version,
    about = "Build and run blockchain_test_engine fixtures through ethrex's block builder"
)]
struct Cli {
    #[command(flatten)]
    runner: RunnerArgs,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let started = Instant::now();
    let rt = tokio::runtime::Runtime::new().expect("failed to build the tokio runtime");
    let files = match collect_json_files(cli.runner.fixture_paths()) {
        Ok(files) => files,
        Err(errors) => {
            for e in errors {
                eprintln!("error: {e}");
            }
            return runner_error();
        }
    };
    let outcome = run_files(&files, cli.runner.workers, |file| {
        run_file(file, &cli.runner, &rt)
    });
    report(&outcome, cli.runner.json, started)
}

fn run_file(
    file: &Path,
    args: &RunnerArgs,
    rt: &tokio::runtime::Runtime,
) -> Result<Vec<FixtureResult>, String> {
    let fixtures: EngineFixtureFile = std::fs::read_to_string(file)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))?;
    let opts = RunOptions {
        precompile_cache: !args.no_precompile_cache,
        check_exception_names: false,
        ..RunOptions::from_env()
    };
    let mut results = Vec::new();
    for (name, fixture) in fixtures.iter().filter(|(name, _)| args.selects(name)) {
        let mut rejections = Vec::new();
        let mut build = BuildOutcome::default();
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            rt.block_on(run_build_fixture(
                name,
                fixture,
                &opts,
                &mut rejections,
                &mut build,
            ))
        }));
        let outcome = match outcome {
            Ok(Err(failure)) if failure.is_skip() => continue,
            Ok(result) => Ok(result.map_err(|failure| failure.to_string())),
            Err(panic) => Err(panic),
        };
        let mut result = FixtureResult::from_outcome(
            name.clone(),
            fixture.network.clone(),
            outcome,
            rejections.into_iter().map(Into::into).collect(),
        );
        result.built = Some(build.built);
        result.mismatches = Some(build.mismatches.into_iter().map(Into::into).collect());
        results.push(result);
    }
    Ok(results)
}
