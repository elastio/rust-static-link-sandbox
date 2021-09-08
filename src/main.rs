mod docker;
mod environments;
mod tests;

use crate::environments::Environment;
use color_eyre::{
    eyre::Context,
    eyre::{eyre, WrapErr},
    Report, Result,
};
use std::{path::PathBuf, process::exit};
use structopt::StructOpt;
use tests::{TestCrate, TestResult};
use tracing::*;

#[derive(StructOpt)]
struct Args {
    /// Specify the environment or environments to test
    ///
    /// Default is to use all environments
    #[structopt(long = "environment", possible_values = environments::all_environment_names())]
    envs: Vec<String>,

    /// Specific tests to run by name.
    ///
    /// Default is to run all tests
    tests: Vec<String>,
}

#[tokio::main]
async fn main() {
    use tracing_subscriber::EnvFilter;

    let args = Args::from_args();
    tracing_subscriber::fmt()
        .pretty()
        .with_env_filter(EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "hyper=warn,debug".to_string()),
        ))
        .init();

    if let Err(e) = run(args).await {
        error!("{:#?}", e);
        exit(-1);
    }
}

async fn run(args: Args) -> Result<()> {
    color_eyre::install()?;

    let tests = if !args.tests.is_empty() {
        // caller specified some tests by name so only run those (if they're valid)
        tests::load_tests(args.tests)?
    } else {
        tests::load_all_tests()?
    };

    let environments: Vec<&Environment> = if !args.envs.is_empty() {
        args.envs
            .iter()
            .map(|env_name| {
                Environment::from_name(env_name)
                    .ok_or_else(|| eyre!("Environment name '{}' not valid", env_name))
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        environments::all_environments().iter().collect()
    };

    let docker = docker::connect_docker().await?;

    for test in tests {
        let span = info_span!("test case", test = %test.name);
        let _guard = span.enter();

        info!("Starting tests");

        for env in &environments {
            let span = info_span!("env", env = %env.name);
            let _guard = span.enter();

            match test.run_test(&docker, env).await {
                Ok(TestResult::StaticBinary) => {
                    info!("Yay!  Resulting binary is static!");
                }
                Ok(TestResult::NonStaticBinary { deps }) => {
                    warn!(
                        "Meh.  Resulting binary is not static: \n * {}",
                        deps.join("\n * ")
                    );
                }
                Ok(TestResult::Failed { output }) => {
                    error!("Build failed: \n{}", output);
                }
                Err(e) => {
                    error!("Couldn't attempt the build: \n{}", e)
                }
            }
        }
    }

    Ok(())
}
