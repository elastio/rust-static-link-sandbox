use crate::docker;
use crate::Environment;
use cargo_metadata::{Metadata, MetadataCommand};
use color_eyre::{
    eyre::Context,
    eyre::{eyre, WrapErr},
    Report, Result,
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use shiplift::{tty::TtyChunk, Container, Docker, Exec, ExecContainerOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use tracing::*;

/// Struct which maps to the TOML we expect to find in each test crate's `Cargo.toml` file
/// at `[package.metadata.test-crate]`
#[derive(Clone, Debug, Serialize, Deserialize)]
struct CargoTomlPackageMetadata {
    env: Vec<String>,
}

/// Describe a test crate in the `crates` directory which makes up a test
#[derive(Clone, Debug)]
pub(crate) struct TestCrate {
    /// The path to the root directory of the crate, where `Cargo.toml` is located
    path: PathBuf,

    /// The name of the crate (and thus, the test)
    name: String,

    /// The metadata about this crate as reported by cargo
    cargo_metadata: Metadata,

    /// The metadata we place in the crate's Cargo.toml to customize the test behavior
    package_metadata: CargoTomlPackageMetadata,
}

impl TestCrate {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn name(&self) -> &str {
        &self.name
    }

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

        // Get the contents of the `[package.metadata.test-crate]` metadata from this crate's Cargo.toml
        let test_crate_metadata_value = root.metadata.get("test-crate").ok_or_else(|| {
            eyre!(
                "Test crate '{}' is missing [package.metadata.test-crate] in Cargo.toml",
                path.display()
            )
        })?;

        let package_metadata: CargoTomlPackageMetadata =
            serde_json::from_value(test_crate_metadata_value.to_owned()).wrap_err_with(|| {
                eyre!(
                    "Test crate '{}' has invalid [package.metadata.test-crate] contents: {:#?}",
                    path.display(),
                    test_crate_metadata_value
                )
            })?;

        Ok(Self {
            path: root
                .manifest_path
                .as_std_path()
                .parent()
                .unwrap()
                .to_owned(),
            name: root.name.clone(),
            cargo_metadata: metadata,
            package_metadata,
        })
    }

    /// Run this test in a given environment, returning the result of the test
    pub async fn run_test<'docker>(
        &self,
        docker: &'docker Docker,
        cache_dir: &Path,
        env: &Environment,
    ) -> Result<TestResult> {
        // Prepare a new container for the test run
        let env_vars = self.env_vars();
        let volumes = self.volumes(cache_dir, env);

        let container = env.launch_container(docker, env_vars, volumes).await?;

        let result = self.run_test_in_container(docker, env, &container).await;

        // let _ = io::stdin().read(&mut [0u8]).unwrap();

        // Whether the test succeeded or failed, always terminate the container
        debug!(container_id = container.id(), "Stopping container");

        let _ = container.stop(None).await.map_err(|e| {
            error!(
                container_id = container.id(),
                "Error stopping container: {}\nStop and delete this container manually", e
            );
        });
        let _ = container.delete().await.map_err(|e| {
            // This is bad; there's not really anything we can do about it.  This container needs to be terminated
            // manually
            error!(
                container_id = container.id(),
                "Error deleting container: {}\nDelete this container manually", e
            );
        });

        result
    }

    /// Once the Docker container is launched, run the actual test
    async fn run_test_in_container<'docker>(
        &self,
        docker: &'docker Docker,
        env: &Environment,
        container: &Container<'docker>,
    ) -> Result<TestResult> {
        // Always start with a clean target dir.  We don't want a prior test run to interfere
        let (exec_details, output) =
            docker::exec_in_container(docker, container, vec!["cargo", "clean"]).await?;

        let exit_code = exec_details
            .exit_code
            .expect("Non-running process must have exit code");
        if exit_code != 0 {
            return Ok(TestResult::Failed {
                output: format!(
                    "`cargo clean` terminated with exit code {}: \n{}",
                    exit_code, output
                ),
            });
        }

        // Build the binary first; if there are any problems related to the build env or linker they will appear here
        let (exec_details, output) = docker::exec_in_container(
            docker,
            container,
            vec!["cargo", "build", "--target", env.musl_target()],
        )
        .await?;

        let exit_code = exec_details
            .exit_code
            .expect("Non-running process must have exit code");
        if exit_code != 0 {
            return Ok(TestResult::Failed {
                output: format!(
                    "`cargo build` terminated with exit code {}: \n{}",
                    exit_code, output
                ),
            });
        }

        // Now run the binary.  This is to detect problems on startup, like mixed C or C++ runtimes or missing library deps
        let (exec_details, output) = docker::exec_in_container(
            docker,
            container,
            vec!["cargo", "run", "--target", env.musl_target()],
        )
        .await?;

        let exit_code = exec_details
            .exit_code
            .expect("Non-running process must have exit code");
        if exit_code != 0 {
            return Ok(TestResult::Failed {
                output: format!(
                    "`cargo run` terminated with exit code {}: \n{}",
                    exit_code, output
                ),
            });
        }

        // Now find the binary itself so we can run ldd on it.
        let (exec_details, output) = docker::exec_in_container(
            docker,
            container,
            vec!["find", "target", "-name", self.name()],
        )
        .await?;

        let exit_code = exec_details
            .exit_code
            .expect("Non-running process must have exit code");
        if exit_code != 0 {
            return Ok(TestResult::Failed {
                output: format!(
                    "`find` terminated with exit code {}: \n{}",
                    exit_code, output
                ),
            });
        }

        // The output should be a single line with the path relative to the working directory
        let binary_path = output.trim().to_string();
        debug!(binary_path = %binary_path,
            "Checking binary for dynamic lib dependencies");

        let (exec_details, output) =
            docker::exec_in_container(docker, container, vec!["ldd", &binary_path]).await?;

        let exit_code = exec_details
            .exit_code
            .expect("Non-running process must have exit code");
        if exit_code != 0 {
            return Ok(TestResult::Failed {
                output: format!(
                    "`ldd` terminated with exit code {}: \n{}",
                    exit_code, output
                ),
            });
        }

        if output.contains("not a dynamic executable") || output.contains("statically linked") {
            // Yay!
            Ok(TestResult::StaticBinary)
        } else {
            Ok(TestResult::NonStaticBinary {
                deps: output.lines().map(|l| l.to_string()).collect(),
            })
        }
    }

    /// Get the env vars for this test
    ///
    /// Each env var is a string with a name and an optional value:
    ///  `NAME[=VALUE]`
    ///
    /// This comes from the package metadata
    fn env_vars(&self) -> Vec<String> {
        const RUSTFLAGS: &str = "-C target-feature=+crt-static";

        let mut env_vars = self.package_metadata.env.clone();

        // If there's a RUSTFLAGS env in here, combine it with the RUSTFLAGS we always add
        if let Some(var) = env_vars.iter_mut().find(|v| v.starts_with("RUSTFLAGS=")) {
            *var = format!("{} {}", var, RUSTFLAGS);
        } else {
            env_vars.push(format!("RUSTFLAGS={}", RUSTFLAGS));
        }

        env_vars
    }

    /// Get the docker volume mounts for this test
    ///
    /// Each one is in the usual docker format
    fn volumes(&self, cache_dir: &Path, env: &Environment) -> Vec<String> {
        // Use dedicated volumes for the cargo cache so repeated tests aren't starting from nothing,
        // and always mount the crate root at /build
        vec![
            format!(
                "{}/registry:{}/registry",
                cache_dir.display(),
                env.cargo_home()
            ),
            format!(
                "{}/registry-index:{}/registry/index",
                cache_dir.display(),
                env.cargo_home()
            ),
            format!(
                "{}/registry-git:{}/registry/git",
                cache_dir.display(),
                env.cargo_home()
            ),
            format!("{}/git-db:{}/git/db", cache_dir.display(), env.cargo_home()),
            format!("{}:/build", self.path.display()),
        ]
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

    test_crates.sort_unstable_by(|lhs, rhs| lhs.name.partial_cmp(&rhs.name).unwrap());

    Ok(test_crates)
}
