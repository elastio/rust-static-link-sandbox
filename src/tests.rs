use crate::Environment;
use cargo_metadata::{Metadata, MetadataCommand};
use color_eyre::{
    eyre::Context,
    eyre::{eyre, WrapErr},
    Report, Result,
};
use shiplift::Docker;
use std::path::PathBuf;
use tracing::*;

/// Describe a test crate in the `crates` directory which makes up a test
#[derive(Clone, Debug)]
pub(crate) struct TestCrate {
    /// The path to the root directory of the crate, where `Cargo.toml` is located
    pub path: PathBuf,

    /// The name of the crate (and thus, the test)
    pub name: String,

    /// The metadata about this crate
    pub metadata: Metadata,
}

impl TestCrate {
    fn load(path: PathBuf) -> Result<Self> {
        debug!(path = %path.display(),
            "Loading test crate");
        let metadata = MetadataCommand::new()
            .manifest_path(path.join("Cargo.toml"))
            .exec()
            .wrap_err_with(|| eyre!("Error getting crate metadata for {}", path.display()))?;

        // The test crates always have only one package
        let root = metadata
            .root_package()
            .expect("Test crates should always have one package");

        Ok(Self {
            path: root
                .manifest_path
                .as_std_path()
                .parent()
                .unwrap()
                .to_owned(),
            name: root.name.clone(),
            metadata,
        })
    }

    /// Run this test in a given environment, returning the result of the test
    pub async fn run_test(&self, docker: &Docker, env: &Environment) -> Result<TestResult> {
        let container = env
            .run_in_docker(docker, Vec::<String>::new(), Vec::<String>::new())
            .await?;

        info!("What do I do now?");

        Ok(TestResult::StaticBinary)
    }
}

/// The result of a single build test of a single crate on a single environment
pub(crate) enum TestResult {
    /// Great success!  The build succeeded and the binary was static
    StaticBinary,

    /// Moderate success.  The build succeeded but the resulting binary depends on one or more shared objects
    NonStaticBinary { deps: Vec<String> },

    /// The cargo build command failed
    Failed { output: String },
}

/// Load specific, named tests from the test crates directory
pub(crate) fn load_tests(test_names: Vec<String>) -> Result<Vec<TestCrate>> {
    let test_crates = load_all_tests()?;

    test_names
        .into_iter()
        .map(|test_name| {
            test_crates
                .iter()
                .find(|test_crate| test_crate.name == test_name)
                .cloned()
                .ok_or_else(|| eyre!("'{}' is not a valid test name", test_name))
        })
        .collect::<Result<Vec<_>>>()
}

/// Discover all of the test crates, reading their metadata
pub(crate) fn load_all_tests() -> Result<Vec<TestCrate>> {
    let crates_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("crates");

    debug!(crates_dir = %crates_dir.display(),
        "Enumerating crates");

    let mut test_crates = Vec::new();
    for entry in crates_dir.read_dir()? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            test_crates.push(TestCrate::load(entry.path())?);
        }
    }

    Ok(test_crates)
}
